package compile

import (
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestIncrementalMacroInputsRebuildWithoutSourceEdits(t *testing.T) {
	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "macros_model"), dir)
	setPortabilityCompilerOptions(t, dir, map[string]any{"incremental": true, "tsBuildInfoFile": "cache/build.rbxtsc.tsbuildinfo"})
	write := func(name, text string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("src/main.ts", "print($env.SLOPTOR_MACRO_INPUT); print($file('./../data.txt'));\n")
	write(".env", "SLOPTOR_MACRO_INPUT=first\n")
	write("data.txt", "first data")
	build := func() string {
		t.Helper()
		_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
		if err != nil {
			t.Fatalf("build: %v\n%v", err, diags)
		}
		data, err := os.ReadFile(filepath.Join(dir, "out/main.luau"))
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}
	first := build()
	write(".env", "SLOPTOR_MACRO_INPUT=second\n")
	write("data.txt", "second data")
	second := build()
	if first == second || !strings.Contains(second, "second") || !strings.Contains(second, "second data") {
		t.Fatalf("macro edits left stale output:\n%s", second)
	}
	t.Setenv("SLOPTOR_MACRO_INPUT", "process override")
	if got := build(); !strings.Contains(got, "process override") {
		t.Fatalf("process environment left stale output: %s", got)
	}
	if got := build(); !strings.Contains(got, "second data") {
		t.Fatalf("no-change build lost file macro: %s", got)
	}
	files := MacroWatchFiles(filepath.Join(dir, "tsconfig.json"))
	for _, name := range []string{".env", "data.txt"} {
		if !slices.Contains(files, filepath.Join(dir, name)) {
			t.Errorf("watch files %v omit %s", files, name)
		}
	}
}

func TestRemovedMacrosAreOrdinaryUndeclaredIdentifiers(t *testing.T) {
	for _, name := range []string{"$asset", "$git", "$buildTime"} {
		t.Run(name, func(t *testing.T) {
			dir := t.TempDir()
			copyDir(t, filepath.Join("testdata", "macros_model"), dir)
			if err := os.WriteFile(filepath.Join(dir, "src/main.ts"), []byte("print("+name+"('value'));\n"), 0o644); err != nil {
				t.Fatal(err)
			}
			_, diags, err := CompileProject(dir)
			if err == nil || !strings.Contains(strings.Join(diags, "\n"), "Cannot find name '"+name+"'") {
				t.Fatalf("removed macro compiled: %v, %v", err, diags)
			}
			if strings.Contains(RotorTypesFileText, name) {
				t.Fatalf("editor companion advertises %s", name)
			}
		})
	}
}

func TestIncrementalMacroInputsFollowCustomConfigDirectory(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "project")
	copyDir(t, filepath.Join("testdata", "macros_model"), dir)
	setPortabilityCompilerOptions(t, dir, map[string]any{"incremental": true, "tsBuildInfoFile": "cache/build.rbxtsc.tsbuildinfo", "typeRoots": []string{"../node_modules/@rbxts"}})
	if err := os.WriteFile(filepath.Join(root, "package.json"), []byte(`{"name":"macros-fixture"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src/main.ts"), []byte("print($file('config.json'));\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	opts := ProjectOptions{TsConfigPath: filepath.Join(dir, "tsconfig.json"), RojoConfigPath: filepath.Join(dir, "default.project.json"), IncludePath: filepath.Join(dir, "include")}
	for _, value := range []string{"first", "second"} {
		if err := os.WriteFile(filepath.Join(dir, "config.json"), []byte(`{"value":"`+value+`"}`), 0o644); err != nil {
			t.Fatal(err)
		}
		_, diags, err := BuildProjectWithOptions(root, opts)
		if err != nil {
			t.Fatalf("build: %v, %v", err, diags)
		}
		output, err := os.ReadFile(filepath.Join(dir, "out/main.luau"))
		if err != nil || !strings.Contains(string(output), value) {
			t.Fatalf("custom config left stale output: %s, %v", output, err)
		}
	}
}
