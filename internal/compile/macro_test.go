package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ----------------------------------------------------------------------------
// $nameof / $keys / $file / $git / $buildTime compile-time macros — rotor
// extensions (no rbxtsc counterpart).
//
// The fixtures deliberately do NOT declare these macros in their globals.d.ts:
// the type surface must come from the synthetic in-memory __rotor_macros.d.ts
// that newProjectProgramFromFS injects (macrodecl.go), so these tests cover
// both the declaration injection and the transformer inlining.
//
// $git / $buildTime vary per checkout/build, so the golden injects a stable
// fake StampProvider via stampProviderOverride (the test seam on State.Stamps).
// ----------------------------------------------------------------------------

// fakeStamps is a deterministic StampProvider for the $git / $buildTime golden.
type fakeStamps struct{}

func (fakeStamps) GitSHA() string    { return "a1b2c3d" }
func (fakeStamps) GitBranch() string { return "main" }
func (fakeStamps) GitTag() string    { return "v1.2.0" }
func (fakeStamps) GitDirty() bool    { return true }
func (fakeStamps) BuildTime() string { return "2026-06-13T09:41:05Z" }

// TestMacrosModel is the end-to-end golden: every macro inlined into main.luau.
// $nameof(a.b.c) -> "c"; $keys<{x,y}>() -> { "x", "y" }; $file(config.json) ->
// the exact Luau table (sorted keys, null member dropped, int/float preserved);
// $file(./notes.txt) -> a Luau string; $git / $buildTime -> the fake values.
func TestMacrosModel(t *testing.T) {
	old := stampProviderOverride
	stampProviderOverride = fakeStamps{}
	t.Cleanup(func() { stampProviderOverride = old })

	files := compileRuntimeLibProject(t, "macros_model")

	want := "-- Compiled with sloptor v2.3.0\n" +
		"-- $nameof: trailing property name, and a bare identifier.\n" +
		"print(\"Health\")\n" +
		"print(\"foo\")\n" +
		"-- $keys<T>(): inline the type's string keys as an array.\n" +
		"print({ \"x\", \"y\" })\n" +
		"-- $file: a project-relative JSON file -> a Luau table; an importer-relative\n" +
		"-- text file -> a Luau string.\n" +
		"print({\n" +
		"\tenabled = true,\n" +
		"\tlevels = { \"forest\", \"cave\", \"castle\" },\n" +
		"\tmeta = {\n" +
		"\t\tauthor = \"uproot\",\n" +
		"\t\tbuild = 42,\n" +
		"\t},\n" +
		"\tname = \"Adventure Quest\",\n" +
		"\tversion = 3,\n" +
		"\tvolume = 0.8,\n" +
		"})\n" +
		"print([[hello from a text file\n" +
		"]])\n" +
		"-- $git / $buildTime: build/VCS stamping (values supplied by the injected fake\n" +
		"-- provider in the test so the golden is stable).\n" +
		"print(\"a1b2c3d\")\n" +
		"print(\"main\")\n" +
		"print(\"v1.2.0\")\n" +
		"print(true)\n" +
		"print(\"2026-06-13T09:41:05Z\")\n" +
		"return nil\n"
	if got := files["out/main.luau"]; got != want {
		t.Errorf("out/main.luau:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// assertMacroDiag compiles a single diag-fixture file and asserts it produced
// no output and a diagnostic with the given code (never a hard error).
func assertMacroDiag(t *testing.T, relPath, wantCode string) {
	t.Helper()
	text, diags, err := CompileFileDetailed(filepath.Join("testdata", "macros_diag_model"), relPath)
	if err != nil {
		t.Fatalf("CompileFileDetailed returned hard error (want diagnostic): %v", err)
	}
	if text != "" {
		t.Errorf("expected no output text, got:\n%s", text)
	}
	for _, d := range diags {
		if d.Code == wantCode {
			return
		}
	}
	t.Errorf("diagnostics = %+v, want one with code %s", diags, wantCode)
}

// A bare `$keys` outside a call has no runtime value — a diagnostic, not a panic.
func TestKeysMacroBareUsageDiagnostic(t *testing.T) {
	assertMacroDiag(t, "src/bare.ts", "rotorKeysBadUsage")
}

// $nameof of a call expression has no statically-knowable trailing name.
func TestNameofMacroInvalidArgDiagnostic(t *testing.T) {
	assertMacroDiag(t, "src/nameofcall.ts", "rotorNameofInvalid")
}

// A bare `$nameof` outside a call is a diagnostic, not a panic.
func TestNameofMacroBareUsageDiagnostic(t *testing.T) {
	assertMacroDiag(t, "src/nameofbare.ts", "rotorNameofBadUsage")
}

// A non-literal $git field cannot be inlined at compile time.
func TestGitMacroNonLiteralArgDiagnostic(t *testing.T) {
	assertMacroDiag(t, "src/gitnonliteral.ts", "rotorGitNonLiteralArg")
}

// $file of a path with no file on disk is a clear diagnostic.
func TestFileMacroNotFoundDiagnostic(t *testing.T) {
	assertMacroDiag(t, "src/missing.ts", "rotorFileNotFound")
}

// $file of a .json file with invalid JSON is a clear diagnostic.
func TestFileMacroInvalidJSONDiagnostic(t *testing.T) {
	assertMacroDiag(t, "src/badjson.ts", "rotorFileInvalidJSON")
}

// A macro-using project built through the disk-writing pipeline must gain the
// consolidated rotor.d.ts editor companion (and report the macro usage on the
// result); a project that never mentions a macro must NOT gain the file.
func TestBuildWritesMacroTypesForMacroProjects(t *testing.T) {
	old := stampProviderOverride
	stampProviderOverride = fakeStamps{}
	t.Cleanup(func() { stampProviderOverride = old })

	t.Run("macro project", func(t *testing.T) {
		dir := t.TempDir()
		copyDir(t, filepath.Join("testdata", "macros_model"), dir)

		result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("build failed: %v\n%s", err, strings.Join(diags, "\n"))
		}
		if !result.UsesMacros {
			t.Error("UsesMacros = false, want true")
		}
		if !result.WroteRotorTypes {
			t.Error("WroteRotorTypes = false, want true on first build")
		}
		data, err := os.ReadFile(filepath.Join(dir, RotorTypesFileName))
		if err != nil {
			t.Fatalf("rotor.d.ts not written: %v", err)
		}
		if string(data) != RotorTypesFileText {
			t.Error("rotor.d.ts content differs from RotorTypesFileText")
		}

		// Second build: file current, no rewrite.
		result, diags, err = BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("rebuild failed: %v\n%s", err, strings.Join(diags, "\n"))
		}
		if !result.UsesMacros || result.WroteRotorTypes {
			t.Errorf("rebuild: UsesMacros = %v, WroteRotorTypes = %v; want true, false",
				result.UsesMacros, result.WroteRotorTypes)
		}
	})

	t.Run("non-macro project", func(t *testing.T) {
		dir := t.TempDir()
		copyDir(t, filepath.Join("testdata", "macros_model"), dir)
		if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("print(1);\n"), 0o644); err != nil {
			t.Fatal(err)
		}

		result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("build failed: %v\n%s", err, strings.Join(diags, "\n"))
		}
		if result.UsesMacros || result.WroteRotorTypes {
			t.Errorf("UsesMacros = %v, WroteRotorTypes = %v; want false, false",
				result.UsesMacros, result.WroteRotorTypes)
		}
		if _, err := os.Stat(filepath.Join(dir, RotorTypesFileName)); err == nil {
			t.Error("rotor.d.ts written for a project that never references a macro")
		}
	})
}

// Coexistence parity guard: a project carrying the generated on-disk rotor.d.ts
// in its program (tsconfig include) AND using the macros must compile without a
// duplicate-identifier error (the injector skips the synthetic declaration) and
// produce byte-identical output to the baseline.
func TestMacrosCoexistWithOnDiskDeclaration(t *testing.T) {
	old := stampProviderOverride
	stampProviderOverride = fakeStamps{}
	t.Cleanup(func() { stampProviderOverride = old })

	baseline := compileRuntimeLibProject(t, "macros_model")

	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "macros_model"), dir)
	if _, err := WriteRotorTypes(dir); err != nil {
		t.Fatal(err)
	}
	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfig, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	patched := strings.Replace(string(tsconfig), `"include": ["src"]`, `"include": ["src", "rotor.d.ts"]`, 1)
	if patched == string(tsconfig) {
		t.Fatal("fixture tsconfig include not found to patch")
	}
	if err := os.WriteFile(tsconfigPath, []byte(patched), 0o644); err != nil {
		t.Fatal(err)
	}

	files, diags, err := CompileProject(dir)
	if err != nil {
		t.Fatalf("coexistence compile failed (duplicate macro declaration?): %v\n%s", err, strings.Join(diags, "\n"))
	}
	if files["out/main.luau"] != baseline["out/main.luau"] {
		t.Errorf("output with on-disk rotor-macros.d.ts differs from baseline:\ngot:\n%s\nwant:\n%s",
			files["out/main.luau"], baseline["out/main.luau"])
	}
}
