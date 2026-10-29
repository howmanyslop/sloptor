package transformer_test

import (
	"errors"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/rojo"
	"rotor/internal/transformer"
)

// The luauTypes verification harness: fixtures under testdata/luautypes are
// rendered with the option on and handed to a real Luau analyzer, so every
// annotation sloptor emits is judged by the type checker it is written for
// rather than by string comparison alone.
//
// Tool facts the harness rests on (verified 2026-10-29):
//   - `luau-lsp analyze` 1.70.1 honors `--definitions=<file>` (it logs
//     `Loading definitions file`), defaults to the old solver unless
//     `--flag:LuauSolverV2=true` is passed, and under its default roblox
//     platform exposes every `Enum<Name>` extern type as the type
//     `Enum.<Name>`. Without a definitions file it warns about the roblox
//     platform; `--platform=standard` silences that for stub-free runs.
//   - `luau-analyze` 0.730 has no definitions flag, silently ignores unknown
//     flags and stdin, and `--mode=strict` works on regular files.
//   - Both exit 1 whenever a type error is reported and 0 for a file whose
//     only findings are lints, so exit 1 is never a failure on its own; only
//     TypeError and SyntaxError lines count.
//   - sloptor emits `--!strict` only when the TS file carries `//!strict`
//     (sourcefile.go), so rendered fixtures have no mode directive and
//     luau-lsp, which has no mode flag, would check them nonstrict. Both
//     tools honor a `.luaurc` next to the module with
//     `{"languageMode": "strict"}`; the harness writes one. A file's own
//     directive still wins, which is what the probes rely on.
//   - In strict mode, rbxtsc's untyped class and numeric-enum shapes do not
//     analyze clean: `local Foo` followed by `do Foo = setmetatable(...) end`
//     types `Foo` as `T?`, so `self:constructor(...)` and `Codes.B` are
//     rejected. luauTypesStrictDebt carries those fixtures until WP7 lands
//     the typed shape.

// luauAnalyzerKind names which analyzer findLuauAnalyzer resolved.
type luauAnalyzerKind int

const (
	// luauAnalyzerLSP is `luau-lsp analyze`, the only one of the two that
	// loads a definitions file.
	luauAnalyzerLSP luauAnalyzerKind = iota
	// luauAnalyzerAnalyze is the plain `luau-analyze` binary.
	luauAnalyzerAnalyze
)

// luauAnalyzer is a resolved analyzer binary plus the kind that decides its
// command line.
type luauAnalyzer struct {
	path string
	kind luauAnalyzerKind
}

// luauModuleName is the file name every analyzed module is written under;
// diagnostics come back as `module.luau(line,col): Kind: message`.
const luauModuleName = "module.luau"

// luauStubName is the definitions file under testdata/luautypes that declares
// every global a compiled fixture may reach.
const luauStubName = "sloptor.d.luau"

// luauStrictConfig is the .luaurc written next to every analyzed module; it
// is how luau-lsp (no mode flag) is put into strict mode.
const luauStrictConfig = "{\"languageMode\": \"strict\"}\n"

var (
	// luauDiagnosticPosition pulls the line number out of a type or syntax
	// error. The file name is not matched: luau-analyze prefixes it with
	// `./`, and both tools drop it from some messages (`Metatable was not a
	// table` arrives as a bare `(line,col)`).
	luauDiagnosticPosition = regexp.MustCompile(`\((\d+),\d+\): (?:TypeError|SyntaxError): `)
	// luauUnknownName captures the leading identifier of an unknown global or
	// type (`Enum.Material` reports as `Enum`).
	luauUnknownName = regexp.MustCompile(`Unknown (?:global|type) '([A-Za-z_][A-Za-z0-9_]*)`)
	// luauStubDeclaredName captures every name the stub declares, value or
	// extern type.
	luauStubDeclaredName = regexp.MustCompile(`(?m)^declare (?:extern type )?([A-Za-z_][A-Za-z0-9_]*)`)
)

// findLuauAnalyzer prefers luau-lsp (definitions support), falls back to
// luau-analyze, and skips the test when neither is installed. Both are mise
// tools, so a binary missing from PATH is also looked up through
// `mise which`.
func findLuauAnalyzer(t *testing.T) luauAnalyzer {
	t.Helper()
	if path := lookupLuauTool("luau-lsp"); path != "" {
		return luauAnalyzer{path: path, kind: luauAnalyzerLSP}
	}
	if path := lookupLuauTool("luau-analyze"); path != "" {
		return luauAnalyzer{path: path, kind: luauAnalyzerAnalyze}
	}
	t.Skip("no Luau analyzer found; install one with `mise install luau-lsp` (preferred) or `mise install luau`")
	return luauAnalyzer{}
}

// lookupLuauTool resolves name on PATH, then through `mise which name`, and
// returns "" when neither knows it.
func lookupLuauTool(name string) string {
	if path, err := exec.LookPath(name); err == nil {
		return path
	}
	mise, err := exec.LookPath("mise")
	if err != nil {
		return ""
	}
	out, err := exec.Command(mise, "which", name).Output()
	if err != nil {
		return ""
	}
	path := strings.TrimSpace(string(out))
	if info, statErr := os.Stat(path); statErr != nil || info.IsDir() {
		return ""
	}
	return path
}

// luauStubPath is the absolute path of testdata/luautypes/sloptor.d.luau.
func luauStubPath(t *testing.T) string {
	t.Helper()
	path, err := filepath.Abs(filepath.Join("testdata", "luautypes", luauStubName))
	if err != nil {
		t.Fatal(err)
	}
	return path
}

// analyzeLuauStrict writes source to <t.TempDir()>/module.luau next to a
// strict-mode .luaurc, runs the analyzer over it, and returns every output
// line that contains "TypeError" or "SyntaxError"; lint findings never
// count. With withStub, luau-lsp loads sloptor.d.luau through
// `--definitions=`.
// luau-analyze cannot load definitions, so under it a module that reaches a
// stubbed global is skipped rather than reported: its unknown-global errors
// would only be noise. Any outcome other than a clean run or exit status 1
// (the analyzers' "diagnostics reported" status) fails the test, as does an
// `[ERROR]` line (luau-lsp failed to read the stub) or an exit 1 that says
// nothing about the module (missing file, bad argument).
func analyzeLuauStrict(t *testing.T, analyzer luauAnalyzer, source string, withStub bool) []string {
	t.Helper()

	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, luauModuleName), []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".luaurc"), []byte(luauStrictConfig), 0o644); err != nil {
		t.Fatal(err)
	}

	var args []string
	switch analyzer.kind {
	case luauAnalyzerLSP:
		args = []string{"analyze", "--flag:LuauSolverV2=true"}
		if withStub {
			args = append(args, "--definitions="+luauStubPath(t))
		} else {
			args = append(args, "--platform=standard")
		}
	case luauAnalyzerAnalyze:
		args = []string{"--mode=strict"}
	}
	args = append(args, luauModuleName)

	cmd := exec.Command(analyzer.path, args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	output := string(out)
	if err != nil {
		var exitErr *exec.ExitError
		if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
			t.Fatalf("%s %s: %v\n%s", analyzer.path, strings.Join(args, " "), err, output)
		}
	}
	if strings.Contains(output, "[ERROR]") {
		t.Fatalf("%s reported an error:\n%s", filepath.Base(analyzer.path), output)
	}

	findings := []string{}
	for _, line := range strings.Split(output, "\n") {
		if strings.Contains(line, "TypeError") || strings.Contains(line, "SyntaxError") {
			findings = append(findings, line)
		}
	}
	if err != nil && len(findings) == 0 && !strings.Contains(output, luauModuleName+"(") {
		t.Fatalf("%s exited 1 without reporting on %s:\n%s", filepath.Base(analyzer.path), luauModuleName, output)
	}
	if withStub && analyzer.kind == luauAnalyzerAnalyze {
		if name, ok := unknownStubGlobal(t, findings); ok {
			t.Skipf("module reaches the stubbed global %q and luau-analyze cannot load %s; install luau-lsp", name, luauStubName)
		}
	}
	return findings
}

// unknownStubGlobal reports the first unknown global or type among findings
// that the stub declares, which is how a luau-analyze run tells "this module
// needs the stub" apart from a genuine unknown name.
func unknownStubGlobal(t *testing.T, findings []string) (string, bool) {
	t.Helper()
	stub, err := os.ReadFile(luauStubPath(t))
	if err != nil {
		t.Fatal(err)
	}
	declared := map[string]bool{}
	for _, match := range luauStubDeclaredName.FindAllStringSubmatch(string(stub), -1) {
		declared[match[1]] = true
	}
	for _, finding := range findings {
		if match := luauUnknownName.FindStringSubmatch(finding); match != nil && declared[match[1]] {
			return match[1], true
		}
	}
	return "", false
}

// luauErrorLines reduces findings to the sorted set of line numbers they
// point at. A finding without a position (a definitions-file SyntaxError,
// for one) fails the test: it is never about the module.
func luauErrorLines(t *testing.T, findings []string) []int {
	t.Helper()
	lines := []int{}
	for _, finding := range findings {
		match := luauDiagnosticPosition.FindStringSubmatch(finding)
		if match == nil {
			t.Fatalf("diagnostic without a position in %s: %s", luauModuleName, finding)
		}
		line, err := strconv.Atoi(match[1])
		if err != nil {
			t.Fatal(err)
		}
		if !slices.Contains(lines, line) {
			lines = append(lines, line)
		}
	}
	slices.Sort(lines)
	return lines
}

// renderLuauTypes renders a testdata/luautypes fixture with luauTypes on,
// through TransformSourceFile so the header, the runtime-lib import and the
// export table are part of what the analyzer sees.
func renderLuauTypes(t *testing.T, relPath string) string {
	t.Helper()
	projDir := filepath.Join("testdata", "luautypes")
	s := buildState(t, projDir, relPath)
	s.LuauTypes = true
	s.Rojo = luauTypesRojoContext(t, projDir)
	statements := transformer.TransformSourceFile(s)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
	return render.RenderAST(statements)
}

// luauTypesRojoContext gives the fixture project just enough Rojo data for a
// relative import to resolve (`TS.import(script, script.Parent, "lib",
// "shapes")`): a synthetic resolver over a never-written out/ directory and
// the matching src -> out path translator. RuntimeLibRbxPath stays nil, so
// the header takes the `local TS = _G[script]` form the stub declares `TS`
// for.
func luauTypesRojoContext(t *testing.T, projDir string) *transformer.RojoContext {
	t.Helper()
	dir, err := filepath.Abs(projDir)
	if err != nil {
		t.Fatal(err)
	}
	outDir := filepath.Join(dir, "out")
	return &transformer.RojoContext{
		Resolver:       rojo.Synthetic(outDir),
		PathTranslator: rojo.NewPathTranslator(filepath.Join(dir, "src"), outDir, "", false, true),
		ProjectPath:    dir,
	}
}

// luauTypesFixtures lists every .ts file under testdata/luautypes/src as a
// slash-separated path relative to the project, lib/ included, in walk
// order.
func luauTypesFixtures(t *testing.T) []string {
	t.Helper()
	root := filepath.Join("testdata", "luautypes")
	fixtures := []string{}
	err := filepath.WalkDir(filepath.Join(root, "src"), func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() || filepath.Ext(path) != ".ts" {
			return nil
		}
		rel, relErr := filepath.Rel(root, path)
		if relErr != nil {
			return relErr
		}
		fixtures = append(fixtures, filepath.ToSlash(rel))
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(fixtures) == 0 {
		t.Fatal("no fixtures under testdata/luautypes/src")
	}
	return fixtures
}

// TestLuauTypesClassIdiomProbes re-runs the hand-written probes that settled
// the luauTypes class and enum spelling (plan WP7, 2026-10-29) and asserts
// the exact set of lines the analyzer rejects: each probe ends in deliberate
// misuse, and every earlier line must stay clean. A probe that starts
// failing elsewhere means the analyzer, not sloptor, moved. The probes
// reference no externs, so they run without the stub on either analyzer.
func TestLuauTypesClassIdiomProbes(t *testing.T) {
	analyzer := findLuauAnalyzer(t)

	expected := map[string][]int{
		// P1f: `local bad1: string = f:get()`, `f.y`, `Foo.new("s")`.
		"class-basic.luau": {20, 21, 22},
		// P3b: generic class with `self:constructor(value) or self`;
		// `local bad: string = Box.new(1):get()`.
		"class-generic.luau": {20},
		// P2h: flat derived alias; `local bad: Derived = Base.new(1)`,
		// `local bad2: string = d:get()`, `b:both()` on the base.
		"class-inherit-flat.luau": {34, 35, 36},
		// P2k: generic intersection alias; `local bad: string = p:get()`,
		// `local bad2: Box<string> = p`.
		"class-inherit-generic.luau": {35, 36},
		// P5b: statics as `self: typeof(Foo)`; the three P1f errors plus
		// `Foo.make(f, 2)` passing an instance where the class table goes.
		"class-statics.luau": {28, 29, 30, 31},
		// P6: `local bad: Direction = "sideways"`.
		"enum-string.luau": {6},
		// P9b: colon-form methods leave `self` untyped, so `self.nope` on
		// line 19 is silently accepted and only the arithmetic over two
		// `unknown` method results on line 22 is rejected. That silence is
		// why luauTypes never emits colon-form methods.
		"method-colon-self.luau": {22},
		// P7e: `type function` with `T: type` parameters, generic alias
		// defaults, `read`/`write` members, `keyof<>`/`index<>`, packs.
		"type-function-param-annotated.luau": {},
	}

	probes, err := filepath.Glob(filepath.Join("testdata", "luautypes", "probes", "*.luau"))
	if err != nil {
		t.Fatal(err)
	}
	seen := map[string]bool{}
	for _, probe := range probes {
		name := filepath.Base(probe)
		want, ok := expected[name]
		if !ok {
			t.Errorf("probe %s has no expected-error entry", name)
			continue
		}
		seen[name] = true
		t.Run(strings.TrimSuffix(name, ".luau"), func(t *testing.T) {
			source, readErr := os.ReadFile(probe)
			if readErr != nil {
				t.Fatal(readErr)
			}
			findings := analyzeLuauStrict(t, analyzer, string(source), false)
			if got := luauErrorLines(t, findings); !slices.Equal(got, want) {
				t.Errorf("error lines: got %v, want %v\n%s", got, want, strings.Join(findings, "\n"))
			}
		})
	}
	for name := range expected {
		if !seen[name] {
			t.Errorf("expected probe %s is missing from testdata/luautypes/probes", name)
		}
	}
}

// TestLuauTypesStubDefinitionsHonored pins that the stub actually reaches
// the analyzer: with it loaded only the deliberate error survives, and
// without it the same module trips over the globals the stub declares.
// luau-analyze has no definitions flag, so the test is luau-lsp only.
func TestLuauTypesStubDefinitionsHonored(t *testing.T) {
	analyzer := findLuauAnalyzer(t)
	if analyzer.kind != luauAnalyzerLSP {
		t.Skip("luau-analyze cannot load a definitions file; install luau-lsp")
	}

	source := strings.Join([]string{
		"--!strict",
		"local TS = _G[script]",
		"local origin: Vector3 = Vector3.new(1, 2, 3)",
		"local name: string = script.Name",
		"local bad: string = origin.X",
		"return TS.import(script, name)",
		"",
	}, "\n")

	withStub := analyzeLuauStrict(t, analyzer, source, true)
	if got := luauErrorLines(t, withStub); !slices.Equal(got, []int{5}) {
		t.Errorf("with the stub only line 5 should fail: got %v\n%s", got, strings.Join(withStub, "\n"))
	}

	withoutStub := analyzeLuauStrict(t, analyzer, source, false)
	for _, want := range []string{"Unknown global 'script'", "Unknown type 'Vector3'"} {
		if !slices.ContainsFunc(withoutStub, func(line string) bool { return strings.Contains(line, want) }) {
			t.Errorf("without the stub expected %q in:\n%s", want, strings.Join(withoutStub, "\n"))
		}
	}
}

// luauTypesStrictDebt lists the fixtures whose output does not analyze clean
// in strict mode yet, each with the work package that clears it. The test
// requires these to keep failing, so an entry has to be deleted the moment
// its emission lands. A fixture that fails for a reason outside its own
// package gets an entry here naming the package that owns the fix, not a
// workaround in the fixture.
var luauTypesStrictDebt = map[string]string{
	"src/classes.ts":    "WP7: rbxtsc's `local Foo` + `do Foo = setmetatable(...) end` shape types the class table as `T?`, so `self:constructor(...)` and every `Foo.*` inside the block are rejected",
	"src/enums.ts":      "WP7: the numeric enum table is assigned inside a `do` block, so `Codes.B` reads through a `T?`",
	"src/lib/shapes.ts": "WP7: the class shape of classes.ts, plus an unannotated method return the checker asks to annotate",
}

// TestLuauTypesFixturesAnalyzeClean renders every fixture under
// testdata/luautypes/src with luauTypes on and requires the analyzer to
// report no type or syntax error against it in strict mode. Fixtures in
// luauTypesStrictDebt must still fail instead. Each WP that adds annotations
// makes the gate bite harder; today most output is untyped.
func TestLuauTypesFixturesAnalyzeClean(t *testing.T) {
	analyzer := findLuauAnalyzer(t)
	for _, relPath := range luauTypesFixtures(t) {
		t.Run(strings.TrimSuffix(strings.TrimPrefix(relPath, "src/"), ".ts"), func(t *testing.T) {
			rendered := renderLuauTypes(t, relPath)
			findings := analyzeLuauStrict(t, analyzer, rendered, true)
			if reason, inDebt := luauTypesStrictDebt[relPath]; inDebt {
				if len(findings) == 0 {
					t.Errorf("%s now analyzes clean; delete its luauTypesStrictDebt entry (%s)", relPath, reason)
				} else {
					t.Logf("%s still carries strict-mode debt (%s):\n%s", relPath, reason, strings.Join(findings, "\n"))
				}
				return
			}
			if len(findings) != 0 {
				t.Errorf("%s does not analyze clean:\n%s\n--- rendered ---\n%s", relPath, strings.Join(findings, "\n"), rendered)
			}
		})
	}
}
