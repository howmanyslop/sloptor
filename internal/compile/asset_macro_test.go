package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ----------------------------------------------------------------------------
// $asset compile-time asset macro — rotor extension (the headline 2.0
// feature; no rbxtsc counterpart).
//
// The fixture deliberately does NOT declare `$asset` in its globals.d.ts: the
// type surface must come from the synthetic in-memory __rotor_asset.d.ts that
// newProjectProgramFromFS injects (assetdecl.go), so this test covers both the
// declaration injection and the transformer inlining (assetmacro.go).
//
// asset_model carries a committed tiny PNG and a pre-seeded rotor-lock.json
// mapping that PNG's content hash to a stable id, so the compile is fully
// offline/deterministic: a cache hit inlines `"rbxassetid://<id>"`.
// ----------------------------------------------------------------------------

// The inlined id from asset_model/rotor-lock.json (assets/logo.png).
const assetModelInlinedID = "rbxassetid://987654321"

func TestAssetMacroModelCacheHitInlines(t *testing.T) {
	// The lockfile hit means no network/API key is ever consulted.
	files := compileRuntimeLibProject(t, "asset_model")

	// main.ts references the same asset twice; both inline the cached id.
	want := "-- Compiled with sloptor v2.3.3\n" +
		"print(\"" + assetModelInlinedID + "\")\n" +
		"print(\"" + assetModelInlinedID + "\")\n" +
		"return nil\n"
	if got := files["out/main.luau"]; got != want {
		t.Errorf("out/main.luau:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// A non-literal $asset path cannot be resolved at compile time and must
// produce a clear rotor diagnostic — never a panic.
func TestAssetMacroNonLiteralArgDiagnostic(t *testing.T) {
	assertAssetDiag(t, "src/nonliteral.ts", "rotorAssetNonLiteralArg")
}

// A bare `$asset` outside call position has no runtime value — a diagnostic,
// not a panic.
func TestAssetMacroBareUsageDiagnostic(t *testing.T) {
	assertAssetDiag(t, "src/bare.ts", "rotorAssetBadUsage")
}

// A $asset path to a file that does not exist on disk is a clear diagnostic.
func TestAssetMacroFileNotFoundDiagnostic(t *testing.T) {
	assertAssetDiag(t, "src/missing.ts", "rotorAssetFileNotFound")
}

// A $asset reference to a real file with no lockfile entry and no cloud client
// (offline build, no ROBLOX_API_KEY / no creator) cannot produce an id and
// must surface rotorAssetNotCached.
func TestAssetMacroNotCachedDiagnostic(t *testing.T) {
	// Defensive: even if the test host has a key set, the diag fixture has no
	// rotor.toml, so newAssetResolver builds no creator/client and stays
	// offline. Unset to be certain the not-cached path is exercised.
	t.Setenv("ROBLOX_API_KEY", "")
	assertAssetDiag(t, "src/notcached.ts", "rotorAssetNotCached")
}

// assertAssetDiag compiles a single diag-fixture file and asserts it produced
// no output and a diagnostic with the given code (and never a hard error,
// which would surface as an "internal compiler error").
func assertAssetDiag(t *testing.T, relPath, wantCode string) {
	t.Helper()
	text, diags, err := CompileFileDetailed(filepath.Join("testdata", "asset_diag_model"), relPath)
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

// A $asset-using project built through the disk-writing pipeline must gain the
// consolidated rotor.d.ts editor companion (and report the macro usage on the
// result); a cache-hit build must NOT rewrite the lockfile.
func TestBuildWritesAssetTypesForAssetProjects(t *testing.T) {
	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "asset_model"), dir)

	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("build failed: %v\n%s", err, strings.Join(diags, "\n"))
	}
	if !result.UsesAssetMacro {
		t.Error("UsesAssetMacro = false, want true")
	}
	if !result.WroteRotorTypes {
		t.Error("WroteRotorTypes = false, want true on first build")
	}
	if result.WroteLockfile {
		t.Error("WroteLockfile = true on a pure cache-hit build (no upload happened)")
	}
	data, err := os.ReadFile(filepath.Join(dir, RotorTypesFileName))
	if err != nil {
		t.Fatalf("rotor.d.ts not written: %v", err)
	}
	if string(data) != RotorTypesFileText {
		t.Error("rotor.d.ts content differs from RotorTypesFileText")
	}

	// Rebuild: companion is current, no rewrite; still a cache hit.
	result, diags, err = BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("rebuild failed: %v\n%s", err, strings.Join(diags, "\n"))
	}
	if !result.UsesAssetMacro || result.WroteRotorTypes || result.WroteLockfile {
		t.Errorf("rebuild: UsesAssetMacro=%v WroteRotorTypes=%v WroteLockfile=%v; want true,false,false",
			result.UsesAssetMacro, result.WroteRotorTypes, result.WroteLockfile)
	}
}

// Coexistence parity guard: a project carrying the generated on-disk
// rotor.d.ts in its program (tsconfig include) AND using $asset must compile
// without a duplicate-identifier error (the injector skips the synthetic
// declaration) and produce byte-identical output to the baseline.
func TestAssetMacroCoexistsWithOnDiskDeclaration(t *testing.T) {
	baseline := compileRuntimeLibProject(t, "asset_model")

	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "asset_model"), dir)
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
		t.Fatalf("coexistence compile failed (duplicate $asset declaration?): %v\n%s", err, strings.Join(diags, "\n"))
	}
	if files["out/main.luau"] != baseline["out/main.luau"] {
		t.Errorf("output with on-disk rotor-asset.d.ts differs from baseline:\ngot:\n%s\nwant:\n%s",
			files["out/main.luau"], baseline["out/main.luau"])
	}
}
