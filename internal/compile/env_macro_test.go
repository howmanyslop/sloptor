package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ----------------------------------------------------------------------------
// $env compile-time environment macro — rotor extension (no rbxtsc
// counterpart; replaces the community rbxts-transform-env plugin).
//
// The fixture deliberately does NOT declare `$env` in its globals.d.ts: the
// type surface must come from the synthetic in-memory __rotor_env.d.ts that
// newProjectProgramFromFS injects (envdecl.go), so this test covers both the
// declaration injection and the transformer inlining (envmacro.go).
//
// Value resolution priority (internal/dotenv): process env >
// .env.<NODE_ENV> > .env, with NODE_ENV itself resolved from the process
// env first, then .env.
// ----------------------------------------------------------------------------

func TestEnvMacroModel(t *testing.T) {
	// MUST be set before compiling: $env inlines at transform time.
	t.Setenv("ROTOR_TEST_ENV_PROC", "from-process") // overrides the .env value "from-file"
	t.Setenv("NODE_ENV", "studio")                  // selects .env.studio (OVERLAY=from-studio)

	files := compileRuntimeLibProject(t, "env_model")

	// In order: .env value; process-env override; fallback arg; unset -> nil;
	// dot access; element access; 2-arity with a set name (fallback unused);
	// .env.<NODE_ENV> override; double/single quote stripping.
	want := "-- Compiled with sloptor v2.3.2\n" +
		"print(\"Adventure Quest\")\n" +
		"print(\"from-process\")\n" +
		"print(\"fallback-value\")\n" +
		"print(nil)\n" +
		"print(\"Adventure Quest\")\n" +
		"print(\"Adventure Quest\")\n" +
		"print(\"Adventure Quest\")\n" +
		"print(\"from-studio\")\n" +
		"print(\"hello world\")\n" +
		"print(\"single quoted\")\n" +
		"return nil\n"
	if got := files["out/main.luau"]; got != want {
		t.Errorf("out/main.luau:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// A non-literal $env argument cannot be inlined at compile time and must
// produce a clear rotor diagnostic — never a panic (which would surface as
// an "internal compiler error").
func TestEnvMacroNonLiteralArgDiagnostic(t *testing.T) {
	text, diags, err := CompileFileDetailed(filepath.Join("testdata", "env_diag_model"), "src/main.ts")
	if err != nil {
		t.Fatalf("CompileFileDetailed returned hard error (want diagnostic): %v", err)
	}
	if text != "" {
		t.Errorf("expected no output text, got:\n%s", text)
	}
	found := false
	for _, d := range diags {
		if d.Code == "rotorEnvNonLiteralArg" {
			found = true
			if !strings.Contains(d.Message, "string literal") {
				t.Errorf("diagnostic message not descriptive: %q", d.Message)
			}
		}
	}
	if !found {
		t.Errorf("diagnostics = %+v, want one with code rotorEnvNonLiteralArg", diags)
	}
}

// A $env-using project built through the disk-writing pipeline must gain the
// consolidated rotor.d.ts editor companion (and report the macro usage on the
// result); a project that never mentions any macro must NOT gain the file.
func TestBuildWritesEnvTypesForEnvProjects(t *testing.T) {
	t.Run("env project", func(t *testing.T) {
		dir := t.TempDir()
		copyDir(t, filepath.Join("testdata", "env_model"), dir)

		result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("build failed: %v\n%s", err, strings.Join(diags, "\n"))
		}
		if !result.UsesEnvMacro {
			t.Error("UsesEnvMacro = false, want true")
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

		// Second build: file is current, no rewrite.
		result, diags, err = BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("rebuild failed: %v\n%s", err, strings.Join(diags, "\n"))
		}
		if !result.UsesEnvMacro || result.WroteRotorTypes {
			t.Errorf("rebuild: UsesEnvMacro = %v, WroteRotorTypes = %v; want true, false",
				result.UsesEnvMacro, result.WroteRotorTypes)
		}
	})

	t.Run("non-macro project", func(t *testing.T) {
		dir := t.TempDir()
		copyDir(t, filepath.Join("testdata", "env_model"), dir)
		// Replace the only source file with one that never mentions a macro.
		if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("print(1);\n"), 0o644); err != nil {
			t.Fatal(err)
		}

		result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("build failed: %v\n%s", err, strings.Join(diags, "\n"))
		}
		if result.UsesEnvMacro || result.WroteRotorTypes {
			t.Errorf("UsesEnvMacro = %v, WroteRotorTypes = %v; want false, false",
				result.UsesEnvMacro, result.WroteRotorTypes)
		}
		if _, err := os.Stat(filepath.Join(dir, RotorTypesFileName)); err == nil {
			t.Error("rotor.d.ts written for a project that never references a macro")
		}
	})
}

// Coexistence parity guard: a project that carries the generated on-disk
// rotor.d.ts in its program (tsconfig include) AND uses $env must compile
// without a duplicate-identifier error — the injector skips the synthetic
// declaration when the identical on-disk one is already a root file — and
// must produce byte-identical output to the same project without the file.
func TestEnvMacroCoexistsWithOnDiskDeclaration(t *testing.T) {
	t.Setenv("ROTOR_TEST_ENV_PROC", "from-process")
	t.Setenv("NODE_ENV", "studio")

	baseline := compileRuntimeLibProject(t, "env_model")

	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "env_model"), dir)
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
		t.Fatalf("coexistence compile failed (duplicate $env declaration?): %v\n%s", err, strings.Join(diags, "\n"))
	}
	if files["out/main.luau"] != baseline["out/main.luau"] {
		t.Errorf("output with on-disk rotor-env.d.ts differs from baseline:\ngot:\n%s\nwant:\n%s",
			files["out/main.luau"], baseline["out/main.luau"])
	}
}

// A bare `$env` outside call/property/element position (e.g. aliasing it)
// has no runtime value to emit — also a diagnostic, not a panic.
func TestEnvMacroBareUsageDiagnostic(t *testing.T) {
	text, diags, err := CompileFileDetailed(filepath.Join("testdata", "env_diag_model"), "src/bare.ts")
	if err != nil {
		t.Fatalf("CompileFileDetailed returned hard error (want diagnostic): %v", err)
	}
	if text != "" {
		t.Errorf("expected no output text, got:\n%s", text)
	}
	found := false
	for _, d := range diags {
		if d.Code == "rotorEnvBadUsage" {
			found = true
		}
	}
	if !found {
		t.Errorf("diagnostics = %+v, want one with code rotorEnvBadUsage", diags)
	}
}
