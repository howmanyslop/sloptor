package main

import (
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/compile"
)

// writeCleanProject lays down a minimal project tree: a tsconfig with the given
// outDir, a src/ tree, an out/ tree, an include/ folder, and the generated
// editor companions, so a clean run has something to remove.
func writeCleanProject(t *testing.T, outDir string) string {
	t.Helper()
	dir := t.TempDir()
	tsconfig := `{
	"compilerOptions": {
		"rootDir": "src",
		"outDir": "` + outDir + `"
	},
	"include": ["src"]
}`
	mustWrite(t, filepath.Join(dir, "tsconfig.json"), tsconfig)
	mustWrite(t, filepath.Join(dir, "src", "main.ts"), "export {};\n")
	mustWrite(t, filepath.Join(dir, "src", "nested", "mod.ts"), "export const x = 1;\n")
	mustWrite(t, filepath.Join(dir, filepath.FromSlash(outDir), "main.luau"), "print('hi')\n")
	mustWrite(t, filepath.Join(dir, filepath.FromSlash(outDir), "nested", "mod.luau"), "return {}\n")
	mustWrite(t, filepath.Join(dir, "include", "RuntimeLib.luau"), "return {}\n")
	mustWrite(t, filepath.Join(dir, compile.RotorTypesFileName), "// generated\n")
	mustWrite(t, filepath.Join(dir, compile.EnvDeclFileName), "// generated\n")
	mustWrite(t, filepath.Join(dir, compile.MacroDeclFileName), "// generated\n")
	return dir
}

func mustWrite(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestCmdCleanRemovesOutputs(t *testing.T) {
	dir := writeCleanProject(t, "out")

	if code := cmdClean([]string{dir}); code != 0 {
		t.Fatalf("cmdClean exit = %d, want 0", code)
	}

	// out/ and include/ are gone.
	if dirExists(filepath.Join(dir, "out")) {
		t.Error("out/ still present after clean")
	}
	if dirExists(filepath.Join(dir, "include")) {
		t.Error("include/ still present after clean")
	}
	// src/ is untouched.
	if !fileExists(filepath.Join(dir, "src", "main.ts")) {
		t.Error("src/main.ts was removed — clean must never touch source")
	}
	if !fileExists(filepath.Join(dir, "src", "nested", "mod.ts")) {
		t.Error("src/nested/mod.ts was removed — clean must never touch source")
	}
	// Without --types the companions survive.
	if !fileExists(filepath.Join(dir, compile.RotorTypesFileName)) {
		t.Error("rotor.d.ts removed without --types")
	}
	if !fileExists(filepath.Join(dir, compile.EnvDeclFileName)) {
		t.Error("rotor-env.d.ts removed without --types")
	}
}

func TestCmdCleanTypesRemovesCompanions(t *testing.T) {
	dir := writeCleanProject(t, "out")

	if code := cmdClean([]string{dir, "--types"}); code != 0 {
		t.Fatalf("cmdClean --types exit = %d, want 0", code)
	}
	if fileExists(filepath.Join(dir, compile.RotorTypesFileName)) {
		t.Error("rotor.d.ts still present after --types clean")
	}
	if fileExists(filepath.Join(dir, compile.EnvDeclFileName)) {
		t.Error("rotor-env.d.ts still present after --types clean")
	}
	if fileExists(filepath.Join(dir, compile.MacroDeclFileName)) {
		t.Error("rotor-asset.d.ts still present after --types clean")
	}
	if dirExists(filepath.Join(dir, "out")) {
		t.Error("out/ still present after --types clean")
	}
}

func TestCmdCleanDryRunRemovesNothing(t *testing.T) {
	dir := writeCleanProject(t, "out")

	if code := cmdClean([]string{dir, "--dry-run", "--types"}); code != 0 {
		t.Fatalf("cmdClean --dry-run exit = %d, want 0", code)
	}
	// Everything must still be on disk.
	for _, p := range []string{
		filepath.Join(dir, "out", "main.luau"),
		filepath.Join(dir, "include", "RuntimeLib.luau"),
		filepath.Join(dir, compile.RotorTypesFileName),
		filepath.Join(dir, compile.EnvDeclFileName),
		filepath.Join(dir, compile.MacroDeclFileName),
	} {
		if !fileExists(p) {
			t.Errorf("%s removed by --dry-run", p)
		}
	}
}

func TestCmdCleanCustomOutDir(t *testing.T) {
	dir := writeCleanProject(t, "build/luau")

	if code := cmdClean([]string{dir}); code != 0 {
		t.Fatalf("cmdClean exit = %d, want 0", code)
	}
	if dirExists(filepath.Join(dir, "build", "luau")) {
		t.Error("custom outDir build/luau still present after clean")
	}
}

func TestCmdCleanNothingToClean(t *testing.T) {
	dir := t.TempDir()
	mustWrite(t, filepath.Join(dir, "tsconfig.json"), `{"compilerOptions":{"outDir":"out"}}`)
	// No out/, no include/, no companions.
	if code := cmdClean([]string{dir}); code != 0 {
		t.Fatalf("cmdClean exit = %d, want 0", code)
	}
}

func TestCmdCleanNoTsConfig(t *testing.T) {
	dir := t.TempDir()
	if code := cmdClean([]string{dir}); code != 1 {
		t.Fatalf("cmdClean without tsconfig exit = %d, want 1", code)
	}
}

func TestCmdCleanUnknownFlag(t *testing.T) {
	if code := cmdClean([]string{"--bogus"}); code != 1 {
		t.Fatalf("cmdClean unknown flag exit = %d, want 1", code)
	}
}
