package flamework

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"
)

func openArtifactTransaction(t *testing.T, projectRoot string) *artifactTransaction {
	t.Helper()
	root, err := os.OpenRoot(projectRoot)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = root.Close() })
	return &artifactTransaction{root: root}
}

func TestPersistArtifacts_rejectsIntermediateSymlinkToExternalTree(t *testing.T) {
	for _, test := range []struct {
		name     string
		existing []byte
		data     []byte
	}{
		{name: "missing write", data: []byte("private")},
		{name: "existing write", existing: []byte("external-private"), data: []byte("replacement")},
		{name: "existing deletion", existing: []byte("external-private")},
	} {
		t.Run(test.name, func(t *testing.T) {
			// Given: a project-local intermediate component redirects to an external tree.
			projectRoot := t.TempDir()
			externalRoot := t.TempDir()
			nested := filepath.Join(externalRoot, "nested")
			if err := os.Mkdir(nested, 0o755); err != nil {
				t.Fatal(err)
			}
			if test.existing != nil {
				if err := os.WriteFile(filepath.Join(nested, "artifact.json"), test.existing, 0o640); err != nil {
					t.Fatal(err)
				}
			}
			if err := os.Symlink(externalRoot, filepath.Join(projectRoot, "metadata")); err != nil {
				t.Fatal(err)
			}
			before := snapshotArtifactTree(t, externalRoot)

			// When: persistence targets the external artifact through the redirect.
			err := PersistArtifacts(projectRoot, []Artifact{{Path: filepath.Join("metadata", "nested", "artifact.json"), Data: test.data}})

			// Then: destination, staged temp, deletion, and directory operations leave the external tree byte-for-byte unchanged.
			if err == nil {
				t.Fatal("PersistArtifacts succeeded")
			}
			after := snapshotArtifactTree(t, externalRoot)
			if !reflect.DeepEqual(after, before) {
				t.Fatalf("external tree changed:\nbefore=%v\nafter=%v", before, after)
			}
		})
	}
}

func TestPersistArtifacts_rejectsPathsOutsideProjectRoot(t *testing.T) {
	projectRoot := t.TempDir()
	for _, path := range []string{"../escape.json", filepath.Join("nested", "..", "..", "escape.json"), filepath.Join(projectRoot, "absolute.json")} {
		t.Run(path, func(t *testing.T) {
			// Given: an artifact name is not local to the trusted project root.
			artifact := Artifact{Path: path, Data: []byte("private")}

			// When: persistence validates the public boundary.
			err := PersistArtifacts(projectRoot, []Artifact{artifact})

			// Then: the outside name is rejected before a file or directory is created.
			if err == nil {
				t.Fatal("PersistArtifacts succeeded")
			}
		})
	}
}

func TestArtifactTransaction_remainsBoundWhenTrustedRootMovesAndIsReplacedBySymlink(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Windows keeps an os.Root directory handle open, so the trusted root cannot be renamed")
	}
	// Given: an opened trusted root is moved and its former path becomes an external symlink.
	parent := t.TempDir()
	projectRoot := filepath.Join(parent, "project")
	movedRoot := filepath.Join(parent, "moved-project")
	externalRoot := t.TempDir()
	if err := os.Mkdir(projectRoot, 0o755); err != nil {
		t.Fatal(err)
	}
	transaction := openArtifactTransaction(t, projectRoot)
	if err := os.Rename(projectRoot, movedRoot); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(externalRoot, projectRoot); err != nil {
		t.Fatal(err)
	}

	// When: an artifact is staged and committed through the already-open root.
	entry, err := stageArtifact(transaction, Artifact{Path: "artifact.json", Data: []byte("bound")})
	if err == nil {
		err = commitArtifact(&entry)
	}

	// Then: the artifact lands under the moved trusted directory and the external referent stays empty.
	if err != nil {
		t.Fatal(err)
	}
	if got, readErr := os.ReadFile(filepath.Join(movedRoot, "artifact.json")); readErr != nil || string(got) != "bound" {
		t.Fatalf("moved-root artifact = %q, err=%v", got, readErr)
	}
	if got := snapshotArtifactTree(t, externalRoot); len(got) != 1 {
		t.Fatalf("external tree changed: %v", got)
	}
}

func TestPersistArtifacts_refusesIntermediateSymlinkDuringRollback(t *testing.T) {
	// Given: a late commit failure swaps an intermediate project directory for an external symlink before rollback.
	projectRoot := t.TempDir()
	metadata := filepath.Join(projectRoot, "metadata")
	movedMetadata := filepath.Join(projectRoot, "moved-metadata")
	externalRoot := t.TempDir()
	if err := os.Mkdir(metadata, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"first.json", "second.json"} {
		if err := os.WriteFile(filepath.Join(metadata, name), []byte("prior"), 0o640); err != nil {
			t.Fatal(err)
		}
	}
	externalBefore := snapshotArtifactTree(t, externalRoot)
	originalRename := renameArtifact
	renameCount := 0
	renameArtifact = func(root *os.Root, oldPath, newPath string) error {
		renameCount++
		if renameCount == 2 {
			if err := os.Rename(metadata, movedMetadata); err != nil {
				return err
			}
			if err := os.Symlink(externalRoot, metadata); err != nil {
				return err
			}
			return errors.New("injected late failure")
		}
		return originalRename(root, oldPath, newPath)
	}
	defer func() { renameArtifact = originalRename }()

	// When: rollback encounters the redirected intermediate component.
	err := PersistArtifacts(projectRoot, []Artifact{
		{Path: filepath.Join("metadata", "first.json"), Data: []byte("replacement")},
		{Path: filepath.Join("metadata", "second.json"), Data: []byte("replacement")},
	})

	// Then: rollback fails closed without reading, writing, deleting, or cleaning up in the external tree.
	if err == nil {
		t.Fatal("PersistArtifacts succeeded")
	}
	if after := snapshotArtifactTree(t, externalRoot); !reflect.DeepEqual(after, externalBefore) {
		t.Fatalf("external tree changed:\nbefore=%v\nafter=%v", externalBefore, after)
	}
}

func snapshotArtifactTree(t *testing.T, root string) []string {
	t.Helper()
	var snapshot []string
	err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		relative, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		record := relative + ":" + info.Mode().String()
		if info.Mode().IsRegular() {
			contents, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			sum := sha256.Sum256(contents)
			record += ":" + hex.EncodeToString(sum[:])
		}
		snapshot = append(snapshot, record)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return snapshot
}
