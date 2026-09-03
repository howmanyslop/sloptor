package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"
)

// enableDeclarationIncrementalBuilds turns on the two options the declaration
// signature rule needs: an incremental manifest to compare against and a
// declaration output to compare.
func enableDeclarationIncrementalBuilds(t *testing.T, dir string) {
	t.Helper()
	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfigBytes, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	tsconfig := strings.Replace(string(tsconfigBytes), `"outDir": "out"`, `"outDir": "out",
		"declaration": true,
		"incremental": true,
		"tsBuildInfoFile": "out/cache.tsbuildinfo"`, 1)
	if err := os.WriteFile(tsconfigPath, []byte(tsconfig), 0o644); err != nil {
		t.Fatal(err)
	}
}

// writeSignatureFixture is util.ts <- main.ts <- consumer.ts, plus a file that
// imports none of them.
func writeSignatureFixture(t *testing.T, dir string) {
	t.Helper()
	for rel, text := range map[string]string{
		"src/util.ts":      "export const VALUE = 1;\n",
		"src/main.ts":      "import { VALUE } from \"./util\";\nexport const main = VALUE;\n",
		"src/consumer.ts":  "import { main } from \"./main\";\nexport const consumer = main;\n",
		"src/unrelated.ts": "export const unrelated = 1;\n",
	} {
		if err := os.WriteFile(filepath.Join(dir, filepath.FromSlash(rel)), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func outputTreeHashes(t *testing.T, dir string) map[string]string {
	t.Helper()
	hashes := map[string]string{}
	outDir := filepath.Join(dir, "out")
	err := filepath.WalkDir(outDir, func(path string, entry os.DirEntry, err error) error {
		// The build info and the copy-files ledger both record the project's
		// own absolute path, so two fixtures in two temp directories never
		// agree on them. Nothing about the compile is in either.
		if err != nil || entry.IsDir() || strings.HasSuffix(path, ".tsbuildinfo") || entry.Name() == "rbxts.copyfiles.json" {
			return err
		}
		contents, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		relative, relErr := filepath.Rel(outDir, path)
		if relErr != nil {
			return relErr
		}
		sum := sha256.Sum256(contents)
		hashes[filepath.ToSlash(relative)] = hex.EncodeToString(sum[:])
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return hashes
}

// buildForSignatureTest builds and reports how many files the build selected,
// which outputs it wrote, and how long the selection-time declaration emit
// took. The selection count is what the two rules disagree about: the writer
// skips byte-identical output either way, so a file the closure rule
// needlessly recompiles is invisible in the write set.
func buildForSignatureTest(t *testing.T, dir string) (*BuildTimings, []string) {
	t.Helper()
	timings := NewBuildTimings()
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings})
	if err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("build diagnostics: %v", diags)
	}
	timings.finish()
	return timings, emittedFileBases(result)
}

// touchOutputsIntoThePast makes every emitted output look old, so a later
// build's "written" set is the set it genuinely rewrote.
func touchOutputsIntoThePast(t *testing.T, dir string) {
	t.Helper()
	old := time.Unix(100, 0)
	outDir := filepath.Join(dir, "out")
	err := filepath.WalkDir(outDir, func(path string, entry os.DirEntry, err error) error {
		if err != nil || entry.IsDir() {
			return err
		}
		return os.Chtimes(path, old, old)
	})
	if err != nil {
		t.Fatal(err)
	}
}

// A comment-only edit cannot change any importer's meaning, and the declaration
// output proves it: the importers stay out of the build, and every output still
// lands byte-for-byte where a build of the edited sources from scratch would
// have put it.
func TestDeclarationSignatureSelectionSkipsImportersOnCommentOnlyEdit(t *testing.T) {
	commentEdit := "export const VALUE = 1;\n// a comment nothing can observe\n"

	dir := writeProject(t, "@scope/signature-comment-edit", "")
	enableDeclarationIncrementalBuilds(t, dir)
	writeSignatureFixture(t, dir)
	buildForSignatureTest(t, dir)
	touchOutputsIntoThePast(t, dir)
	if err := os.WriteFile(filepath.Join(dir, "src", "util.ts"), []byte(commentEdit), 0o644); err != nil {
		t.Fatal(err)
	}
	timings, wrote := buildForSignatureTest(t, dir)

	if timings.Counts.SelectedSources != 1 {
		t.Fatalf("selected %d files, want only the edited one", timings.Counts.SelectedSources)
	}
	if !slices.Contains(wrote, "util.luau") {
		t.Fatalf("wrote %v, expected the edited file to be recompiled", wrote)
	}
	if timings.Stages.DeclarationEmitMs < 0 || timings.Stages.IncrementalSelectionMs < 0 {
		t.Fatalf("stage totals went negative: declaration emit %dms, incremental selection %dms",
			timings.Stages.DeclarationEmitMs, timings.Stages.IncrementalSelectionMs)
	}

	// A build of the same sources from scratch is the oracle: the narrowed
	// build has to agree with it byte for byte.
	oracleDir := writeProject(t, "@scope/signature-comment-edit-oracle", "")
	enableDeclarationIncrementalBuilds(t, oracleDir)
	writeSignatureFixture(t, oracleDir)
	if err := os.WriteFile(filepath.Join(oracleDir, "src", "util.ts"), []byte(commentEdit), 0o644); err != nil {
		t.Fatal(err)
	}
	buildForSignatureTest(t, oracleDir)

	narrowedHashes := outputTreeHashes(t, dir)
	oracleHashes := outputTreeHashes(t, oracleDir)
	for path, hash := range oracleHashes {
		if narrowedHashes[path] != hash {
			t.Fatalf("output %s differs: full build %s, narrowed build %s", path, hash, narrowedHashes[path])
		}
	}
	if len(narrowedHashes) != len(oracleHashes) {
		t.Fatalf("output count = %d, want %d", len(narrowedHashes), len(oracleHashes))
	}
}

// An edit that moves the declaration output still has to reach the importers.
func TestDeclarationSignatureSelectionRebuildsImportersOnApiChange(t *testing.T) {
	dir := writeProject(t, "@scope/signature-api-change", "")
	enableDeclarationIncrementalBuilds(t, dir)
	writeSignatureFixture(t, dir)
	buildForSignatureTest(t, dir)
	touchOutputsIntoThePast(t, dir)

	if err := os.WriteFile(filepath.Join(dir, "src", "util.ts"), []byte("export const VALUE = \"one\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	timings, wrote := buildForSignatureTest(t, dir)
	if timings.Counts.SelectedSources != 3 {
		t.Fatalf("selected %d files, want the edited file and both importers", timings.Counts.SelectedSources)
	}

	for _, want := range []string{"util.luau", "util.d.ts", "main.d.ts"} {
		if !slices.Contains(wrote, want) {
			t.Fatalf("wrote %v, want it to include %s", wrote, want)
		}
	}
	if slices.Contains(wrote, "unrelated.luau") {
		t.Fatalf("wrote %v, want the unrelated file left alone", wrote)
	}
}

// Deleting a file has to rebuild whoever imported it, whatever their own
// declarations say.
func TestDeclarationSignatureSelectionRebuildsImportersOfADeletedFile(t *testing.T) {
	dir := writeProject(t, "@scope/signature-delete", "")
	enableDeclarationIncrementalBuilds(t, dir)
	writeSignatureFixture(t, dir)
	buildForSignatureTest(t, dir)
	touchOutputsIntoThePast(t, dir)

	if err := os.Remove(filepath.Join(dir, "src", "unrelated.ts")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const main = 2;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	timings, _ := buildForSignatureTest(t, dir)
	if timings.Counts.SelectedSources != 2 {
		t.Fatalf("selected %d files, want main.ts and its importer consumer.ts", timings.Counts.SelectedSources)
	}
}

// A build that selected nothing must reach the write path with no declarations
// to write, and must leave the tree exactly as it found it.
func TestDeclarationSignatureSelectionIsANoOpWithoutChanges(t *testing.T) {
	dir := writeProject(t, "@scope/signature-no-change", "")
	enableDeclarationIncrementalBuilds(t, dir)
	writeSignatureFixture(t, dir)
	buildForSignatureTest(t, dir)
	before := outputTreeHashes(t, dir)
	touchOutputsIntoThePast(t, dir)

	timings, wrote := buildForSignatureTest(t, dir)
	if timings.Counts.SelectedSources != 0 {
		t.Fatalf("selected %d files with nothing changed, want none", timings.Counts.SelectedSources)
	}
	if len(wrote) != 0 {
		t.Fatalf("wrote %v with nothing changed, want nothing", wrote)
	}
	after := outputTreeHashes(t, dir)
	for path, hash := range before {
		if after[path] != hash {
			t.Fatalf("output %s changed on a no-op build", path)
		}
	}
}

func addPathAliases(t *testing.T, dir string) {
	t.Helper()
	path := filepath.Join(dir, "tsconfig.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	updated := strings.Replace(string(data), `"rootDir": "src"`, `"rootDir": "src",
		"baseUrl": ".",
		"paths": { "@app/*": ["src/*"] }`, 1)
	if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
		t.Fatal(err)
	}
}
