package flamework

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"
)

func TestPrepareArtifacts_whenOptionalMetadataIsAbsent(t *testing.T) {
	// Given: a package build with no runtime metadata.
	root := t.TempDir()
	info := NewBuildInfo(filepath.Join(root, "flamework.build"), "1.3.2")

	// When: the complete artifact transaction is prepared.
	artifacts, err := PrepareArtifacts(info, filepath.Join(root, "include", "flamework"), nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	// Then: stale metadata deletions precede the build-info write deterministically.
	got := make([]string, len(artifacts))
	for index, artifact := range artifacts {
		got[index] = filepath.Base(artifact.Path)
	}
	want := []string{"config.json", "globs.json", "flamework.build"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("artifact order = %v, want %v", got, want)
	}
	if artifacts[0].Data != nil || artifacts[1].Data != nil || artifacts[2].Data == nil {
		t.Fatalf("artifact deletion/write intents = %#v", artifacts)
	}
}

func TestPersistArtifacts_whenLateCommitFails(t *testing.T) {
	// Given: two prior artifacts and a deterministic failure on the second commit rename.
	root := t.TempDir()
	firstPath := filepath.Join(root, "first.json")
	secondPath := filepath.Join(root, "second.json")
	prior := []byte("prior bytes")
	for _, path := range []string{firstPath, secondPath} {
		if err := os.WriteFile(path, prior, 0o640); err != nil {
			t.Fatal(err)
		}
	}
	priorInfo, statErr := os.Stat(firstPath)
	if statErr != nil {
		t.Fatal(statErr)
	}
	originalRename := renameArtifact
	renameCount := 0
	renameArtifact = func(root *os.Root, oldPath, newPath string) error {
		renameCount++
		if renameCount == 2 {
			return errors.New("injected second rename failure")
		}
		return originalRename(root, oldPath, newPath)
	}
	defer func() { renameArtifact = originalRename }()
	artifacts := []Artifact{
		{Path: filepath.Base(firstPath), Data: []byte("replacement")},
		{Path: filepath.Base(secondPath), Data: []byte("replacement")},
	}

	// When: the second rename fails after the first commit.
	err := PersistArtifacts(root, artifacts)

	// Then: the transaction fails and restores byte-for-byte prior state.
	if err == nil {
		t.Fatal("PersistArtifacts succeeded")
	}
	after, readErr := os.ReadFile(firstPath)
	if readErr != nil {
		t.Fatal(readErr)
	}
	if !reflect.DeepEqual(after, prior) {
		t.Fatalf("first artifact after rollback = %q, want %q", after, prior)
	}
	secondAfter, readErr := os.ReadFile(secondPath)
	if readErr != nil {
		t.Fatal(readErr)
	}
	if !reflect.DeepEqual(secondAfter, prior) {
		t.Fatalf("second artifact after rollback = %q, want %q", secondAfter, prior)
	}
	info, statErr := os.Stat(firstPath)
	if statErr != nil {
		t.Fatal(statErr)
	}
	if runtime.GOOS != "windows" && info.Mode().Perm() != priorInfo.Mode().Perm() {
		t.Fatalf("first artifact mode = %o, want %o", info.Mode().Perm(), priorInfo.Mode().Perm())
	}
}

func TestPersistArtifacts_whenDeletingAndWriting(t *testing.T) {
	// Given: stale config/globs files and a prepared build-info write.
	root := t.TempDir()
	configPath := filepath.Join(root, "config.json")
	globsPath := filepath.Join(root, "globs.json")
	buildPath := filepath.Join(root, "flamework.build")
	for _, path := range []string{configPath, globsPath} {
		if err := os.WriteFile(path, []byte("stale"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	// When: one all-or-nothing transaction is persisted.
	err := PersistArtifacts(root, []Artifact{
		{Path: filepath.Base(configPath), Data: nil},
		{Path: filepath.Base(globsPath), Data: []byte{}},
		{Path: filepath.Base(buildPath), Data: []byte("build")},
	})
	if err != nil {
		t.Fatal(err)
	}

	// Then: nil deletes, non-nil empty writes, and normal bytes persist.
	if _, err := os.Stat(configPath); !os.IsNotExist(err) {
		t.Fatalf("config stat error = %v, want not exist", err)
	}
	globs, err := os.ReadFile(globsPath)
	if err != nil {
		t.Fatal(err)
	}
	if globs == nil || len(globs) != 0 {
		t.Fatalf("globs bytes = %#v, want non-nil empty", globs)
	}
	build, err := os.ReadFile(buildPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(build) != "build" {
		t.Fatalf("build bytes = %q", build)
	}
}

func TestPersistArtifacts_whenStagingFailsAfterCreatingParent(t *testing.T) {
	// Given: an empty artifact tree and a later destination beneath a regular file.
	root := t.TempDir()
	blocker := filepath.Join(root, "blocker")
	if err := os.WriteFile(blocker, []byte("file"), 0o644); err != nil {
		t.Fatal(err)
	}
	artifacts := []Artifact{
		{Path: filepath.Join("include", "flamework", "config.json"), Data: []byte("{}")},
		{Path: filepath.Join("blocker", "globs.json"), Data: []byte("{}")},
	}

	// When: staging the later artifact fails before commit.
	err := PersistArtifacts(root, artifacts)

	// Then: no new artifact directories or files remain.
	if err == nil {
		t.Fatal("PersistArtifacts succeeded")
	}
	if _, statErr := os.Stat(filepath.Join(root, "include")); !os.IsNotExist(statErr) {
		t.Fatalf("staging changed artifact tree: %v", statErr)
	}
}

func TestPersistArtifacts_whenExistingArtifactIsExternalSymlink(t *testing.T) {
	// Given: the first artifact is a symlink to external bytes and a later commit would fail.
	root := t.TempDir()
	externalRoot := t.TempDir()
	externalPath := filepath.Join(externalRoot, "secret.json")
	artifactPath := filepath.Join(root, "first.json")
	secondPath := filepath.Join(root, "second.json")
	external := []byte("external-private-bytes")
	if err := os.WriteFile(externalPath, external, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(externalPath, artifactPath); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(secondPath, []byte("second-prior"), 0o640); err != nil {
		t.Fatal(err)
	}
	originalRename := renameArtifact
	renameCount := 0
	renameArtifact = func(root *os.Root, oldPath, newPath string) error {
		renameCount++
		if renameCount == 2 {
			return errors.New("injected late failure")
		}
		return originalRename(root, oldPath, newPath)
	}
	defer func() { renameArtifact = originalRename }()

	// When: the artifact transaction is attempted.
	err := PersistArtifacts(root, []Artifact{
		{Path: filepath.Base(artifactPath), Data: []byte("replacement")},
		{Path: filepath.Base(secondPath), Data: []byte("replacement")},
	})

	// Then: staging rejects the leaf without reading or replacing its referent.
	if err == nil {
		t.Fatal("PersistArtifacts succeeded")
	}
	info, statErr := os.Lstat(artifactPath)
	if statErr != nil {
		t.Fatal(statErr)
	}
	if info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("artifact mode = %v, want symlink", info.Mode())
	}
	after, readErr := os.ReadFile(externalPath)
	if readErr != nil {
		t.Fatal(readErr)
	}
	if !reflect.DeepEqual(after, external) {
		t.Fatalf("external bytes = %q, want %q", after, external)
	}
}

func TestPersistArtifacts_whenDestinationIsDanglingSymlink(t *testing.T) {
	// Given: an artifact leaf is a dangling symlink.
	root := t.TempDir()
	path := filepath.Join(root, "artifact.json")
	if err := os.Symlink(filepath.Join(root, "missing"), path); err != nil {
		t.Fatal(err)
	}

	// When: persistence attempts to stage the artifact.
	err := PersistArtifacts(root, []Artifact{{Path: filepath.Base(path), Data: []byte("replacement")}})

	// Then: the nonregular leaf remains untouched.
	if err == nil {
		t.Fatal("PersistArtifacts succeeded")
	}
	info, statErr := os.Lstat(path)
	if statErr != nil || info.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("dangling symlink changed: mode=%v err=%v", info.Mode(), statErr)
	}
}

func TestPersistArtifacts_whenDestinationIsNonRegular(t *testing.T) {
	// Given: an artifact leaf is a non-regular filesystem object.
	root := t.TempDir()
	path := filepath.Join(root, "artifact.fifo")
	createNonRegularTarget(t, path)

	// When: persistence attempts to stage the artifact.
	err := PersistArtifacts(root, []Artifact{{Path: filepath.Base(path), Data: []byte("replacement")}})

	// Then: the non-regular target remains untouched without being opened.
	if err == nil {
		t.Fatal("PersistArtifacts succeeded")
	}
	info, statErr := os.Lstat(path)
	if statErr != nil || !isNonRegularTarget(info) {
		t.Fatalf("non-regular target changed: mode=%v err=%v", info.Mode(), statErr)
	}
}

func TestCommitArtifact_whenDestinationLeafIsSwapped(t *testing.T) {
	// Given: a staged transaction whose destination is replaced before commit.
	root := t.TempDir()
	path := filepath.Join(root, "artifact.json")
	if err := os.WriteFile(path, []byte("prior"), 0o640); err != nil {
		t.Fatal(err)
	}
	entry, err := stageArtifact(openArtifactTransaction(t, root), Artifact{Path: filepath.Base(path), Data: []byte("replacement")})
	if err != nil {
		t.Fatal(err)
	}
	defer cleanupStagedArtifacts([]stagedArtifact{entry})
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	attacker := []byte("attacker")
	if err := os.WriteFile(path, attacker, 0o600); err != nil {
		t.Fatal(err)
	}

	// When: commit revalidates the destination.
	err = commitArtifact(&entry)

	// Then: commit fails and preserves the swapped leaf.
	if err == nil {
		t.Fatal("commitArtifact succeeded")
	}
	after, readErr := os.ReadFile(path)
	if readErr != nil || !reflect.DeepEqual(after, attacker) {
		t.Fatalf("swapped destination = %q, err=%v", after, readErr)
	}
}

func TestCommitArtifact_whenParentIsSwapped(t *testing.T) {
	// Given: a staged transaction whose direct parent is replaced before commit.
	root := t.TempDir()
	parent := filepath.Join(root, "metadata")
	if err := os.Mkdir(parent, 0o755); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(parent, "artifact.json")
	entry, err := stageArtifact(openArtifactTransaction(t, root), Artifact{Path: filepath.Join("metadata", "artifact.json"), Data: []byte("replacement")})
	if err != nil {
		t.Fatal(err)
	}
	oldParent := filepath.Join(root, "old-metadata")
	if err := os.Rename(parent, oldParent); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(parent, 0o755); err != nil {
		t.Fatal(err)
	}

	// When: commit revalidates the parent.
	err = commitArtifact(&entry)

	// Then: commit fails without writing into the replacement parent.
	if err == nil {
		t.Fatal("commitArtifact succeeded")
	}
	if _, statErr := os.Lstat(path); !os.IsNotExist(statErr) {
		t.Fatalf("replacement parent received artifact: %v", statErr)
	}
}

func TestCommitArtifact_whenStagedLeafIsSwapped(t *testing.T) {
	// Given: a staged transaction whose temporary leaf is replaced before commit.
	root := t.TempDir()
	path := filepath.Join(root, "artifact.json")
	entry, err := stageArtifact(openArtifactTransaction(t, root), Artifact{Path: filepath.Base(path), Data: []byte("replacement")})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(root, entry.tempPath)); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, entry.tempPath), []byte("attacker"), 0o600); err != nil {
		t.Fatal(err)
	}

	// When: commit revalidates the staged leaf.
	err = commitArtifact(&entry)

	// Then: commit fails without creating the destination.
	if err == nil {
		t.Fatal("commitArtifact succeeded")
	}
	if _, statErr := os.Lstat(path); !os.IsNotExist(statErr) {
		t.Fatalf("destination exists after rejected temp swap: %v", statErr)
	}
}

func TestRollbackArtifacts_whenCommittedDestinationIsSwapped(t *testing.T) {
	// Given: a committed transaction whose destination is replaced before rollback.
	root := t.TempDir()
	path := filepath.Join(root, "artifact.json")
	if err := os.WriteFile(path, []byte("prior"), 0o640); err != nil {
		t.Fatal(err)
	}
	entry, err := stageArtifact(openArtifactTransaction(t, root), Artifact{Path: filepath.Base(path), Data: []byte("replacement")})
	if err != nil {
		t.Fatal(err)
	}
	if err := commitArtifact(&entry); err != nil {
		t.Fatal(err)
	}
	entry.committed = true
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	attacker := []byte("attacker")
	if err := os.WriteFile(path, attacker, 0o600); err != nil {
		t.Fatal(err)
	}

	// When: rollback revalidates its committed destination.
	err = rollbackArtifacts([]stagedArtifact{entry})

	// Then: rollback fails without overwriting the swapped leaf.
	if err == nil {
		t.Fatal("rollbackArtifacts succeeded")
	}
	after, readErr := os.ReadFile(path)
	if readErr != nil || !reflect.DeepEqual(after, attacker) {
		t.Fatalf("swapped rollback destination = %q, err=%v", after, readErr)
	}
}

func TestCleanupStagedArtifacts_whenTempPathCollides(t *testing.T) {
	// Given: a staged temporary leaf is replaced by an unowned file.
	root := t.TempDir()
	entry, err := stageArtifact(openArtifactTransaction(t, root), Artifact{Path: "artifact.json", Data: []byte("replacement")})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(root, entry.tempPath)); err != nil {
		t.Fatal(err)
	}
	attacker := []byte("attacker")
	if err := os.WriteFile(filepath.Join(root, entry.tempPath), attacker, 0o600); err != nil {
		t.Fatal(err)
	}

	// When: staged cleanup runs.
	cleanupStagedArtifacts([]stagedArtifact{entry})

	// Then: cleanup leaves the unowned collision untouched.
	after, readErr := os.ReadFile(filepath.Join(root, entry.tempPath))
	if readErr != nil || !reflect.DeepEqual(after, attacker) {
		t.Fatalf("cleanup collision = %q, err=%v", after, readErr)
	}
}
