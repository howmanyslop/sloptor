package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// writeSpecLibProject is the issue #61 layout without a coordinator: the spec
// project is the -b entry and references a lib project in the same directory.
func writeSpecLibProject(t *testing.T, specRbxts, libRbxts string) string {
	t.Helper()
	root := t.TempDir()
	options := `"allowSyntheticDefaultImports":true,"composite":true,"incremental":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"]`
	mustWrite(t, filepath.Join(root, "package.json"), `{"name":"pkg"}`)
	mustWrite(t, filepath.Join(root, "include", "RuntimeLib.lua"), "return {}\n")
	if err := os.MkdirAll(filepath.Join(root, "node_modules", "@rbxts"), 0o755); err != nil {
		t.Fatal(err)
	}
	mustWrite(t, filepath.Join(root, "default.project.json"), `{"name":"pkg","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"lib":{"$path":"out"},"tests":{"$path":"out-test"}}}}`)
	mustWrite(t, filepath.Join(root, "tsconfig.lib.json"), `{"compilerOptions":{`+options+`,"rootDir":"src","outDir":"out","tsBuildInfoFile":"out/tsconfig.lib.tsbuildinfo"},`+libRbxts+`"include":["src"]}`)
	mustWrite(t, filepath.Join(root, "tsconfig.spec.json"), `{"compilerOptions":{`+options+`,"rootDir":"tests","outDir":"out-test","tsBuildInfoFile":"out-test/tsconfig.tsbuildinfo"},`+specRbxts+`"include":["tests"],"references":[{"path":"./tsconfig.lib.json"}]}`)
	mustWrite(t, filepath.Join(root, "src", "globals.d.ts"), noLibGlobalStubs)
	mustWrite(t, filepath.Join(root, "src", "value.ts"), "export const value = 1;\n")
	mustWrite(t, filepath.Join(root, "tests", "globals.d.ts"), noLibGlobalStubs)
	mustWrite(t, filepath.Join(root, "tests", "value.spec.ts"), "import { value } from \"../src/value\";\nexport const doubled = value * 2;\n")
	return root
}

func TestBuildSolutionReusesDirectReferencedProjectBuild(t *testing.T) {
	tests := []struct {
		name      string
		specRbxts string
		libRbxts  string
		flags     []string
	}{
		{name: "entry rbxts does not leak into a reference", specRbxts: `"rbxts":{"luau":false,"optimizedLoops":false},`},
		{name: "command line beats a reference's rbxts", libRbxts: `"rbxts":{"optimizedLoops":false},`, flags: []string{"--optimizedLoops"}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Given: the lib project was built on its own.
			root := writeSpecLibProject(t, tt.specRbxts, tt.libRbxts)
			libArgs := append([]string{"--project", filepath.Join(root, "tsconfig.lib.json")}, tt.flags...)
			if stdout, stderr, code := captureBuildOutput(t, libArgs); code != 0 {
				t.Fatalf("lib build exit %d:\n%s\n%s", code, stdout, stderr)
			}
			libOutput := filepath.Join(root, "out", "value.luau")
			old := time.Date(2000, 1, 1, 0, 0, 0, 0, time.UTC)
			if err := os.Chtimes(libOutput, old, old); err != nil {
				t.Fatal(err)
			}

			// When: the spec builds with --build.
			specArgs := append([]string{"--build", "--project", filepath.Join(root, "tsconfig.spec.json")}, tt.flags...)
			if stdout, stderr, code := captureBuildOutput(t, specArgs); code != 0 {
				t.Fatalf("solution build exit %d:\n%s\n%s", code, stdout, stderr)
			}

			// Then: the lib is up to date, so its output is untouched.
			info, err := os.Stat(libOutput)
			if err != nil {
				t.Fatal(err)
			}
			if !info.ModTime().Equal(old) {
				t.Errorf("solution build rewrote up-to-date lib output %s", libOutput)
			}
			entries, err := os.ReadDir(filepath.Join(root, "out"))
			if err != nil {
				t.Fatal(err)
			}
			for _, entry := range entries {
				if strings.HasSuffix(entry.Name(), ".lua") {
					t.Errorf("solution build emitted lib output with entry's luau option: %s", entry.Name())
				}
			}
		})
	}
}
