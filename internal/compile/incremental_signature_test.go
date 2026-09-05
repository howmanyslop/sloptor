package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
)

func contentHash(text string) string {
	sum := sha256.Sum256([]byte(text))
	return hex.EncodeToString(sum[:])
}

func selectedNames(files []*ast.SourceFile) []string {
	names := make([]string, 0, len(files))
	for _, file := range files {
		names = append(names, filepath.Base(file.FileName()))
	}
	slices.Sort(names)
	return names
}

// signatureFixture is main.ts -> util.ts, plus an unrelated side.ts, with
// util.ts the file that changed.
func signatureFixture() (current, previous *incrementalManifest) {
	main := filepath.FromSlash("/proj/src/main.ts")
	util := filepath.FromSlash("/proj/src/util.ts")
	side := filepath.FromSlash("/proj/src/side.ts")
	current = &incrementalManifest{
		Version: 2,
		Salt:    "salt",
		Files: map[string]incrementalFileState{
			main: {Hash: "main-1", Refs: []string{util}},
			util: {Hash: "util-2"},
			side: {Hash: "side-1"},
		},
		Outputs: map[string]string{},
	}
	previous = &incrementalManifest{
		Version: 2,
		Salt:    "salt",
		Files: map[string]incrementalFileState{
			main: {Hash: "main-1", Refs: []string{util}},
			util: {Hash: "util-1"},
			side: {Hash: "side-1"},
		},
		Outputs: map[string]string{},
	}
	return current, previous
}

func signatureSelection(t *testing.T, declarationText map[string]string, recorded map[string]string) (declarationSignatureSelection, *[]string) {
	t.Helper()
	var emitted []string
	selection := declarationSignatureSelection{
		projectDir: filepath.FromSlash("/proj"),
		declarationPath: func(sourcePath string) string {
			base := strings.TrimSuffix(filepath.Base(sourcePath), ".ts")
			return filepath.Join(filepath.FromSlash("/proj/out"), base+".d.ts")
		},
		previousOutputs: recorded,
		emit: func(files []*ast.SourceFile) ([]declarationEmitFile, error) {
			produced := make([]declarationEmitFile, 0, len(files))
			for _, file := range files {
				base := strings.TrimSuffix(filepath.Base(file.FileName()), ".ts")
				emitted = append(emitted, base)
				text, ok := declarationText[base]
				if !ok {
					continue
				}
				produced = append(produced, declarationEmitFile{
					FileName: filepath.Join(filepath.FromSlash("/proj/out"), base+".d.ts"),
					Text:     text,
				})
			}
			return produced, nil
		},
	}
	return selection, &emitted
}

func TestSelectByDeclarationSignatureStopsAtUnchangedDeclaration(t *testing.T) {
	current, previous := signatureFixture()
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts")
	declarations := map[string]string{"main": "declare main;", "util": "export declare const VALUE: number;"}
	recorded := map[string]string{
		"out/util.d.ts": contentHash(declarations["util"]),
		"out/main.d.ts": contentHash(declarations["main"]),
	}
	selection, emitted := signatureSelection(t, declarations, recorded)

	// The rule this replaces pulls in every transitive importer of the edited
	// file, whatever the edit did.
	if got := selectedNames(selectIncrementalSourceFiles(files, current, previous)); !slices.Equal(got, []string{"main.ts", "util.ts"}) {
		t.Fatalf("closure rule selected %v, want main.ts and util.ts", got)
	}

	selected, reused, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"util.ts"}) {
		t.Fatalf("selected = %v, want only util.ts (main.ts imports it but its declaration held)", got)
	}
	if !slices.Equal(*emitted, []string{"util"}) {
		t.Fatalf("emitted declarations for %v, want only util", *emitted)
	}
	if len(reused) != 1 {
		t.Fatalf("reused declarations = %d, want the one that was emitted", len(reused))
	}
}

// Catches a changed source leaving its importer stale when the declaration
// file from the prior build was altered outside the compiler.
func TestSelectByDeclarationSignaturePropagatesWhenRecordedOutputChanged(t *testing.T) {
	current, previous := signatureFixture()
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts")
	declarations := map[string]string{"main": "declare main;", "util": "export declare const VALUE: number;"}
	recorded := map[string]string{
		"out/util.d.ts": contentHash(declarations["util"]),
		"out/main.d.ts": contentHash(declarations["main"]),
	}
	selection, _ := signatureSelection(t, declarations, recorded)
	selection.previousOutputMatches = func(path, hash string) bool {
		return false
	}

	selected, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"main.ts", "util.ts"}) {
		t.Fatalf("selected = %v, want util.ts and its importer after a changed declaration output", got)
	}
}

func TestSelectByDeclarationSignaturePropagatesOnChangedDeclaration(t *testing.T) {
	current, previous := signatureFixture()
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts")
	declarations := map[string]string{"main": "declare main;", "util": "export declare const VALUE: string;"}
	recorded := map[string]string{
		"out/util.d.ts": contentHash("export declare const VALUE: number;"),
		"out/main.d.ts": contentHash(declarations["main"]),
	}
	selection, _ := signatureSelection(t, declarations, recorded)

	selected, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"main.ts", "util.ts"}) {
		t.Fatalf("selected = %v, want util.ts and its importer main.ts", got)
	}
}

// A declaration nobody recorded a hash for cannot be shown to have held, so it
// has to propagate — the alternative is leaving an importer stale.
func TestSelectByDeclarationSignaturePropagatesWithoutRecordedHash(t *testing.T) {
	current, previous := signatureFixture()
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts")
	declarations := map[string]string{"main": "declare main;", "util": "export declare const VALUE: number;"}
	selection, _ := signatureSelection(t, declarations, map[string]string{})

	selected, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"main.ts", "util.ts"}) {
		t.Fatalf("selected = %v, want the whole closure when no signature was recorded", got)
	}
}

// A file that emits no declaration at all also cannot be shown to have held.
func TestSelectByDeclarationSignaturePropagatesWithoutEmittedDeclaration(t *testing.T) {
	current, previous := signatureFixture()
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts")
	recorded := map[string]string{"out/util.d.ts": contentHash("export declare const VALUE: number;")}
	selection, _ := signatureSelection(t, map[string]string{}, recorded)

	selected, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"main.ts", "util.ts"}) {
		t.Fatalf("selected = %v, want the whole closure when nothing was emitted", got)
	}
}

// A deleted dependency emits nothing to compare, so its importers are seeded
// directly rather than skipped.
func TestSelectByDeclarationSignatureSeedsImportersOfDeletedFiles(t *testing.T) {
	current, previous := signatureFixture()
	util := filepath.FromSlash("/proj/src/util.ts")
	delete(current.Files, util)
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/side.ts")
	declarations := map[string]string{"main": "declare main;"}
	recorded := map[string]string{"out/main.d.ts": contentHash(declarations["main"])}
	selection, _ := signatureSelection(t, declarations, recorded)

	selected, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"main.ts"}) {
		t.Fatalf("selected = %v, want the deleted file's importer", got)
	}
}

// A changed declaration keeps propagating past the first importer.
func TestSelectByDeclarationSignaturePropagatesTransitively(t *testing.T) {
	current, previous := signatureFixture()
	main := filepath.FromSlash("/proj/src/main.ts")
	side := filepath.FromSlash("/proj/src/side.ts")
	current.Files[side] = incrementalFileState{Hash: "side-1", Refs: []string{main}}
	previous.Files[side] = incrementalFileState{Hash: "side-1", Refs: []string{main}}
	files := fakeSourceFiles(t, "/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts")
	declarations := map[string]string{
		"main": "export declare const main: string;",
		"util": "export declare const VALUE: string;",
		"side": "export declare const side: string;",
	}
	recorded := map[string]string{
		"out/util.d.ts": contentHash("export declare const VALUE: number;"),
		"out/main.d.ts": contentHash("export declare const main: number;"),
		"out/side.d.ts": contentHash(declarations["side"]),
	}
	selection, _ := signatureSelection(t, declarations, recorded)

	selected, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, selection)
	if err != nil {
		t.Fatal(err)
	}
	if got := selectedNames(selected); !slices.Equal(got, []string{"main.ts", "side.ts", "util.ts"}) {
		t.Fatalf("selected = %v, want the whole chain once each declaration moved", got)
	}
}

func TestSelectByDeclarationSignatureRejectsAnIncompleteSelection(t *testing.T) {
	current, previous := signatureFixture()
	files := fakeSourceFiles(t, "/proj/src/util.ts")

	_, _, err := selectByDeclarationSignature(files, changedSourcePaths(current, previous), current, previous, declarationSignatureSelection{})
	if err == nil {
		t.Fatal("selectByDeclarationSignature accepted a selection with no emitter")
	}
}

// A wave whose declarations cannot be emitted is not a build failure: the write
// path runs the same emit and reports the diagnostics with a file and a
// position, so selection stands down to the reverse-closure rule instead of
// failing the build on a message with neither.
func TestNarrowSelectionStandsDownWhenTheSelectionEmitFails(t *testing.T) {
	dir := writeProject(t, "@scope/signature-emit-failure", "")
	enableDeclarationIncrementalBuilds(t, dir)
	writeSignatureFixture(t, dir)
	buildForSignatureTest(t, dir)

	if err := os.WriteFile(filepath.Join(dir, "src", "util.ts"), []byte("export const VALUE = 1;\n// edited\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	pathTranslator := createPathTranslator(program, true)
	sourceFiles := projectSourceFiles(program)
	manifest := &incrementalManifest{Version: 2, Salt: "salt", Files: map[string]incrementalFileState{}, Outputs: map[string]string{"out/util.d.ts": "recorded"}}
	previous := &incrementalManifest{Version: 2, Salt: "salt", Files: map[string]incrementalFileState{}, Outputs: map[string]string{}}
	for _, sourceFile := range sourceFiles {
		path := normalizeSourceFilePath(sourceFile.FileName())
		manifest.Files[path] = incrementalFileState{Hash: "new"}
		previous.Files[path] = incrementalFileState{Hash: "old"}
	}

	failing := func(program *compiler.Program, files []*ast.SourceFile) ([]declarationEmitFile, error) {
		return nil, errors.New("compile: declaration emit diagnostics")
	}
	restore := declarationTextsForSelection
	declarationTextsForSelection = failing
	t.Cleanup(func() { declarationTextsForSelection = restore })

	selected, declarations, ok, err := narrowSelectionByDeclarationSignature(
		dir, program, pathTranslator, sourceFiles, nil, manifest, previous, manifest.Outputs, nil, nil,
	)
	if err != nil {
		t.Fatalf("a failed selection emit became a build error: %v", err)
	}
	if ok {
		t.Fatal("selection claimed a result it could not compute")
	}
	if selected != nil || declarations != nil {
		t.Fatalf("selection returned %d files and %d declarations after standing down", len(selected), len(declarations))
	}
}

// The rule looks a declaration up in the manifest by the key the writer
// recorded it under, so the two have to agree character for character. Windows
// makes that a real question: a path can arrive with either slash and with
// either drive-letter case.
func TestSignatureOutputKeyMatchesTheWriterKey(t *testing.T) {
	projectDir := filepath.FromSlash("D:/proj")
	// The project directory need not exist: outputKey is pure path arithmetic,
	// and useHashes would open the directory for its unrelated secure-root
	// work.
	writer := &outputWriter{projectDir: filepath.Clean(projectDir)}
	selection := declarationSignatureSelection{projectDir: filepath.Clean(projectDir)}

	for _, path := range []string{
		"D:/proj/out/util.d.ts",
		"D:\\proj\\out\\util.d.ts",
		"D:\\proj\\out\\nested/deep.d.ts",
		"D:/proj/out/./util.d.ts",
		// A different drive-letter case is a different path to filepath.Rel, so
		// both sides have to fall back the same way rather than one of them
		// silently producing a relative key.
		"d:\\proj\\out\\util.d.ts",
	} {
		if got, want := selection.outputKey(path), writer.outputKey(filepath.FromSlash(path)); got != want {
			t.Fatalf("outputKey(%q) = %q, writer says %q", path, got, want)
		}
	}
}
