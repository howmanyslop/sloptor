package compile

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// A workspace package can ship test-only modules behind a custom condition:
// declarations go to one directory (declarationDir) and Luau to another. The
// import must target the Luau the export branch names, not a sibling of the
// resolved declaration.
func TestCompileProjectExportConditionBranchRuntime(t *testing.T) {
	workspace := t.TempDir()
	game := filepath.Join(workspace, "game")
	dependency := filepath.Join(workspace, "packages", "core")

	gameFiles := map[string]string{
		"package.json":         `{"name":"game"}`,
		"default.project.json": `{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"},"rbxts_include":{"$className":"Folder","node_modules":{"$className":"Folder","@flux":{"$path":"node_modules/@flux"}}}}}}`,
		"tsconfig.json": `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "Preserve",
		"moduleResolution": "Bundler",
		"customConditions": ["testing"],
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@flux", "node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`,
		"src/globals.d.ts": noLibGlobalStubs,
		"src/main.ts": `import { helper } from "@flux/core/testing";
import { styles } from "@flux/core/test/helpers/styles";
print(helper, styles);`,
	}
	dependencyFiles := map[string]string{
		"package.json": `{"name":"@flux/core","version":"1.0.0","exports":{
			"./testing":{"testing":{"source":"./test/helpers/index.ts","types":"./out-tsc/spec/test/helpers/index.d.ts","default":"./out-test/test/helpers/index.luau"}},
			"./test/*":{"testing":{"types":"./out-tsc/spec/test/*.d.ts","default":"./out-test/test/*.luau"}}
		}}`,
		"out-tsc/spec/test/helpers/index.d.ts":  "export declare const helper: number;\n",
		"out-tsc/spec/test/helpers/styles.d.ts": "export declare const styles: number;\n",
		"out-test/test/helpers/index.luau":      "return { helper = 1 }\n",
		"out-test/test/helpers/styles.luau":     "return { styles = 2 }\n",
	}
	for root, files := range map[string]map[string]string{game: gameFiles, dependency: dependencyFiles} {
		for name, content := range files {
			path := filepath.Join(root, filepath.FromSlash(name))
			if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
				t.Fatal(err)
			}
		}
	}

	link := filepath.Join(game, "node_modules", "@flux", "core")
	if err := os.MkdirAll(filepath.Dir(link), 0o755); err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS == "windows" {
		if output, err := exec.Command("cmd", "/c", "mklink", "/J", link, dependency).CombinedOutput(); err != nil {
			t.Fatalf("mklink /J: %v: %s", err, output)
		}
	} else if err := os.Symlink(dependency, link); err != nil {
		t.Fatal(err)
	}

	files, diags, err := CompileProject(game)
	if err != nil {
		t.Fatalf("CompileProject: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}
	main := files["out/main.luau"]
	for _, want := range []string{
		`"@flux", "core", "out-test", "test", "helpers", "index").helper`,
		`"@flux", "core", "out-test", "test", "helpers", "styles").styles`,
	} {
		if !strings.Contains(main, want) {
			t.Errorf("out/main.luau missing %s:\n%s", want, main)
		}
	}
}
