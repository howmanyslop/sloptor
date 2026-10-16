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

func TestDeclarationSignatureSelectionRebuildsImportersOfChangedDeclaration(t *testing.T) {
	const caller = "import { values } from \"./tuple\";\nconst [count, text] = values();\nprint(count, text);\n"
	const declaration = "export declare function values(): LuaTuple<[number, string]>;\n"
	dir := buildAuditProject(t)
	enableDeclarationIncrementalBuilds(t, dir)
	writeFiles(t, dir, map[string]string{
		"src/tuple.d.ts":   "export declare function values(): [number, string];\n",
		"src/tuple.luau":   "return { values = function() return 1, \"text\" end }\n",
		"src/main.ts":      caller,
		"src/unrelated.ts": "export const unrelated = 1;\n",
	})
	buildForSignatureTest(t, dir)
	before := readOutput(t, dir, "main.luau")
	writeFiles(t, dir, map[string]string{"src/tuple.d.ts": declaration})
	timings, _ := buildForSignatureTest(t, dir)
	if timings.Counts.SelectedSources != 1 {
		t.Fatalf("selected %d sources, want only the declaration importer", timings.Counts.SelectedSources)
	}

	oracleDir := buildAuditProject(t)
	enableDeclarationIncrementalBuilds(t, oracleDir)
	writeFiles(t, oracleDir, map[string]string{
		"src/tuple.d.ts":   declaration,
		"src/tuple.luau":   "return { values = function() return 1, \"text\" end }\n",
		"src/main.ts":      caller,
		"src/unrelated.ts": "export const unrelated = 1;\n",
	})
	buildForSignatureTest(t, oracleDir)
	want := readOutput(t, oracleDir, "main.luau")
	if before == want || !strings.Contains(want, "local count, text = values()") {
		t.Fatalf("fixture does not distinguish tuple lowering:\ninitial:\n%s\ncold:\n%s", before, want)
	}
	if got := readOutput(t, dir, "main.luau"); got != want {
		t.Fatalf("declaration-only warm build differs from cold build:\nwarm:\n%s\ncold:\n%s", got, want)
	}
	timings, wrote := buildForSignatureTest(t, dir)
	if timings.Counts.SelectedSources != 0 || len(wrote) != 0 {
		t.Fatalf("unchanged declaration build selected %d sources and wrote %v", timings.Counts.SelectedSources, wrote)
	}
}

func TestDeclarationSignatureSelectionRebuildsThroughDeclarationReexports(t *testing.T) {
	dir := buildAuditProject(t)
	enableDeclarationIncrementalBuilds(t, dir)
	writeFiles(t, dir, map[string]string{
		"src/tuple.d.ts":  "export declare function values(): [number, string];\n",
		"src/bridge.d.ts": "export { values } from \"./tuple\";\n",
		"src/bridge.luau": "return {}\n",
		"src/main.ts":     "import { values } from \"./bridge\";\nconst [count, text] = values();\nprint(count, text);\n",
	})
	buildForSignatureTest(t, dir)
	writeFiles(t, dir, map[string]string{"src/tuple.d.ts": "export declare function values(): LuaTuple<[number, string]>;\n"})
	buildForSignatureTest(t, dir)
	if got := readOutput(t, dir, "main.luau"); !strings.Contains(got, "local count, text = values()") {
		t.Fatalf("declaration re-export left the caller's tuple lowering stale:\n%s", got)
	}
}

func TestDeclarationSignatureSelectionRebuildsThroughSourceReexports(t *testing.T) {
	dir := buildAuditProject(t)
	enableDeclarationIncrementalBuilds(t, dir)
	writeFiles(t, dir, map[string]string{
		"src/tuple.d.ts": "export declare function values(): [number, string];\n",
		"src/tuple.luau": "return {}\n",
		"src/bridge.ts":  "export { values } from \"./tuple\";\n",
		"src/main.ts":    "import { values } from \"./bridge\";\nconst [count, text] = values();\nprint(count, text);\n",
	})
	buildForSignatureTest(t, dir)
	writeFiles(t, dir, map[string]string{"src/tuple.d.ts": "export declare function values(): LuaTuple<[number, string]>;\n"})
	buildForSignatureTest(t, dir)
	if got := readOutput(t, dir, "main.luau"); !strings.Contains(got, "local count, text = values()") {
		t.Fatalf("source re-export left the caller's tuple lowering stale:\n%s", got)
	}
}

func TestDeclarationSignatureSelectionRebuildsForChangedGlobalDeclaration(t *testing.T) {
	for name, declaration := range map[string]string{
		"script":       "declare function values(): RETURN_TYPE;\n",
		"augmentation": "export {}; declare global { function values(): RETURN_TYPE; }\n",
	} {
		t.Run(name, func(t *testing.T) {
			dir := buildAuditProject(t)
			enableDeclarationIncrementalBuilds(t, dir)
			writeFiles(t, dir, map[string]string{
				"src/values.d.ts": strings.ReplaceAll(declaration, "RETURN_TYPE", "[number, string]"),
				"src/main.ts":     "const [count, text] = values();\nprint(count, text);\n",
			})
			buildForSignatureTest(t, dir)
			writeFiles(t, dir, map[string]string{"src/values.d.ts": strings.ReplaceAll(declaration, "RETURN_TYPE", "LuaTuple<[number, string]>")})
			buildForSignatureTest(t, dir)
			if got := readOutput(t, dir, "main.luau"); !strings.Contains(got, "local count, text = values()") {
				t.Fatalf("global declaration left the caller's tuple lowering stale:\n%s", got)
			}
		})
	}
}

func TestDeclarationSignatureSelectionRebuildsForChangedExternalDeclaration(t *testing.T) {
	dir := buildAuditProject(t)
	enableDeclarationIncrementalBuilds(t, dir)
	declarationPath := "node_modules/@rbxts/tuple/index.d.ts"
	writeSolutionFile(t, dir, declarationPath, "export declare function values(): [number, string];\n")
	writeFiles(t, dir, map[string]string{
		"src/main.ts":      "import { values } from \"@rbxts/tuple\";\nconst [count, text] = values();\nprint(count, text);\n",
		"src/unrelated.ts": "export const unrelated = 1;\n",
	})
	buildForSignatureTest(t, dir)
	writeSolutionFile(t, dir, declarationPath, "export declare function values(): LuaTuple<[number, string]>;\n")
	timings, _ := buildForSignatureTest(t, dir)
	if got := readOutput(t, dir, "main.luau"); !strings.Contains(got, "local count, text = values()") {
		t.Fatalf("external declaration left the caller's tuple lowering stale:\n%s", got)
	}
	if timings.Counts.SelectedSources != 1 {
		t.Fatalf("selected %d sources, want only the external declaration importer", timings.Counts.SelectedSources)
	}
}

// A `.json` module is copied, not compiled, so its value never reaches an
// importer's Luau — but its inferred TYPE reaches the importer's declaration
// output. Selection has to seed the importers of a changed data file, or that
// declaration goes stale.
func TestDeclarationSignatureSelectionRebuildsImportersOfChangedJSON(t *testing.T) {
	dir := writeProject(t, "@scope/signature-json", "")
	enableJSONModules(t, dir)
	enableDeclarationIncrementalBuilds(t, dir)
	writeFiles(t, dir, map[string]string{
		"src/data.json": "{\n\t\"value\": 1\n}\n",
		"src/main.ts":   "import data from \"./data.json\";\nexport const value = data.value;\n",
		"src/other.ts":  "export const other = 1;\n",
	})
	buildForSignatureTest(t, dir)
	touchOutputsIntoThePast(t, dir)

	luauBefore := readOutput(t, dir, "main.luau")
	writeFiles(t, dir, map[string]string{"src/data.json": "{\n\t\"value\": \"text\"\n}\n"})
	timings, _ := buildForSignatureTest(t, dir)

	if timings.Counts.SelectedSources != 1 {
		t.Fatalf("selected %d files, want the importer of the changed data file", timings.Counts.SelectedSources)
	}
	if got, want := readOutput(t, dir, "main.d.ts"), "export declare const value: string;\n"; got != want {
		t.Fatalf("declaration = %q, want %q — the importer was not rebuilt for the changed JSON", got, want)
	}
	// The value itself is a runtime require of the copied file, so the Luau
	// cannot have moved.
	if got := readOutput(t, dir, "main.luau"); got != luauBefore {
		t.Fatalf("a JSON value change moved the importer's Luau:\n%s\n%s", luauBefore, got)
	}
	if got, want := readOutput(t, dir, "data.json"), "{\n\t\"value\": \"text\"\n}\n"; got != want {
		t.Fatalf("copied data file = %q, want %q", got, want)
	}
}

func enableJSONModules(t *testing.T, dir string) {
	t.Helper()
	path := filepath.Join(dir, "tsconfig.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	updated := strings.Replace(string(data), `"rootDir": "src"`, `"rootDir": "src",
		"resolveJsonModule": true`, 1)
	if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
		t.Fatal(err)
	}
}

func writeFiles(t *testing.T, dir string, files map[string]string) {
	t.Helper()
	for rel, text := range files {
		if err := os.WriteFile(filepath.Join(dir, filepath.FromSlash(rel)), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func readOutput(t *testing.T, dir, name string) string {
	t.Helper()
	contents, err := os.ReadFile(filepath.Join(dir, "out", name))
	if err != nil {
		t.Fatal(err)
	}
	return string(contents)
}

// With declarationDir the rule compares the declarations where they were
// written, so a comment-only edit still skips the importers.
func TestDeclarationSignatureSelectionUsesDeclarationDir(t *testing.T) {
	dir := writeProject(t, "@scope/signature-declaration-dir", "")
	enableDeclarationIncrementalBuilds(t, dir)
	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfig := strings.Replace(string(mustReadFile(t, tsconfigPath)), `"declaration": true,`, `"declaration": true,
		"declarationDir": "types",`, 1)
	if err := os.WriteFile(tsconfigPath, []byte(tsconfig), 0o644); err != nil {
		t.Fatal(err)
	}
	writeSignatureFixture(t, dir)
	buildForSignatureTest(t, dir)
	if _, err := os.Stat(filepath.Join(dir, "types", "util.d.ts")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "util.ts"), []byte("export const VALUE = 1;\n// comment\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	timings, _ := buildForSignatureTest(t, dir)
	if timings.Counts.SelectedSources != 1 {
		t.Fatalf("selected %d files, want only the edited one", timings.Counts.SelectedSources)
	}
}
