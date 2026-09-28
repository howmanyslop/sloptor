package compile

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// ----------------------------------------------------------------------------
// pnpm symlink resolution end to end (Phase 3b Task 2, digest §2).
//
// pnpm installs the real package files under
// node_modules/.pnpm/<pkg>@<version>/node_modules/<pkg>/ and links
// node_modules/<pkg> to that directory (a junction on Windows). tsgo realpaths
// node_modules resolutions (createResolvedModuleHandlingSymlink), so the
// module's SourceFile.FileName() is the .pnpm realpath; without
// guessVirtualPath the import's moduleScope computes as ".pnpm" and the
// compile fails with noUnscopedModule. State.GuessVirtualPath walks the
// realpath's ancestors through the program symlink cache's
// directories-by-realpath reverse map and rebases the file onto the
// symlink-side (virtual) path, making the pnpm layout emit byte-identically
// to the flat layout.
//
// Ground truth: rbxtsc 3.0.0 (TS 5.5) compiled this exact layout 2026-06-06
// through testdata/diff/project — fake @rbxts/dummy installed under
// node_modules/.pnpm/@rbxts+dummy@1.0.0/node_modules/@rbxts/dummy with a
// junction at node_modules/@rbxts/dummy, src/_scratch.ts importing it
// (oracle technique; scratch artifacts deleted after). Node's
// fs.realpath.native resolves junctions exactly like tsgo's
// GetFinalPathNameByHandle realpath, so rbxtsc exercised the same
// guessVirtualPath path and emitted the flat-layout import verbatim:
//
//	-- Compiled with sloptor v2.6.0
//	local TS = require(script.Parent.include.RuntimeLib)
//	local dummy = TS.import(script, script.Parent, "node_modules", "@rbxts", "dummy", "out").dummy
//	print(dummy())
//	return nil
//
// The fixture is constructed at test time because the junction cannot live in
// git: writePnpmFixture lays the real files under .pnpm and links the virtual
// path with `mklink /J` (no admin required) on Windows, os.Symlink elsewhere.
// t.TempDir cleanup removes junctions without following them.
// ----------------------------------------------------------------------------

// writePnpmFixture builds a pnpm-shaped model project in a temp dir and
// returns its path.
func writePnpmFixture(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()

	files := map[string]string{
		"package.json":         `{"name":"pnpm-fixture"}`,
		"default.project.json": `{"name":"pnpm","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}`,
		"tsconfig.json": `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`,
		// noLib fundamental globals (see testdata/*/src/globals.d.ts).
		"src/globals.d.ts": "declare function print(...params: Array<unknown>): void;\n" +
			"interface Array<T> {}\ninterface Boolean {}\ninterface CallableFunction {}\n" +
			"interface Function {}\ninterface IArguments {}\ninterface NewableFunction {}\n" +
			"interface Number {}\ninterface Object {}\ninterface RegExp {}\ninterface String {}\n",
		"src/main.ts": `import { dummy } from "@rbxts/dummy";
import { linked } from "@rbxts/linked";
print(dummy(), linked());`,
	}

	// The real package files live under the pnpm store paths. @rbxts/dummy is
	// a plain package; @rbxts/linked additionally ships its own
	// default.project.json (`{"tree":{"$path":"src"}}`, the @rbxts/react
	// shape) so the RojoResolver must discover the nested project file
	// THROUGH the junction — fs.realpathSync resolves junctions in the
	// reference (RojoResolver.ts:285) where filepath.EvalSymlinks fails.
	dummyPkg := "node_modules/.pnpm/@rbxts+dummy@1.0.0/node_modules/@rbxts/dummy"
	files[dummyPkg+"/package.json"] = `{"name":"@rbxts/dummy","version":"1.0.0","main":"out/init.lua","types":"out/index.d.ts"}`
	files[dummyPkg+"/out/index.d.ts"] = "export declare function dummy(): number;"
	files[dummyPkg+"/out/init.lua"] = "return { dummy = function() return 1 end }"
	linkedPkg := "node_modules/.pnpm/@rbxts+linked@1.0.0/node_modules/@rbxts/linked"
	files[linkedPkg+"/package.json"] = `{"name":"@rbxts/linked","version":"1.0.0","main":"src/init.lua","types":"src/index.d.ts"}`
	files[linkedPkg+"/default.project.json"] = `{"name":"linked","tree":{"$path":"src"}}`
	files[linkedPkg+"/src/index.d.ts"] = "export declare function linked(): number;"
	files[linkedPkg+"/src/init.lua"] = "return { linked = function() return 2 end }"

	for name, content := range files {
		path := filepath.Join(dir, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(path), 0o777); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(content), 0o666); err != nil {
			t.Fatal(err)
		}
	}

	// Link the virtual package paths to the pnpm store directories.
	for name, realPkg := range map[string]string{"dummy": dummyPkg, "linked": linkedPkg} {
		link := filepath.Join(dir, "node_modules", "@rbxts", name)
		target := filepath.Join(dir, filepath.FromSlash(realPkg))
		if err := os.MkdirAll(filepath.Dir(link), 0o777); err != nil {
			t.Fatal(err)
		}
		if runtime.GOOS == "windows" {
			// Junctions need no privilege; os.Symlink on Windows requires
			// SeCreateSymbolicLinkPrivilege or Developer Mode.
			if out, err := exec.Command("cmd", "/c", "mklink", "/J", link, target).CombinedOutput(); err != nil {
				t.Fatalf("mklink /J: %v: %s", err, out)
			}
		} else {
			if err := os.Symlink(target, link); err != nil {
				t.Fatal(err)
			}
		}
	}
	return dir
}

func TestCompileProjectPnpmSymlinks(t *testing.T) {
	dir := writePnpmFixture(t)
	files, diags, err := CompileProject(dir)
	if err != nil {
		t.Fatalf("CompileProject: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}

	// rbxtsc 3.0.0 junction-fixture output verbatim (see header). The linked
	// import has NO trailing "src": the package's own default.project.json
	// (`{"tree":{"$path":"src"}}`) collapses the src directory onto the
	// package root, and rbxtsc discovers that nested project file through
	// the junction (oracle 2026-06-06, same scratch procedure).
	want := "-- Compiled with sloptor v2.6.0\n" +
		"local TS = require(script.Parent.include.RuntimeLib)\n" +
		"local dummy = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n" +
		"local linked = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"linked\").linked\n" +
		"print(dummy(), linked())\n" +
		"return nil\n"
	if got := files["out/main.luau"]; got != want {
		t.Errorf("out/main.luau:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if len(files) != 1 {
		t.Errorf("produced %d files, want 1 (%v)", len(files), keys(files))
	}
}

func TestCompileProjectPnpmSymlinksWithNodeModulesRojoPath(t *testing.T) {
	dir := writePnpmFixture(t)
	if resolvedDir, err := filepath.EvalSymlinks(dir); err == nil {
		dir = resolvedDir
	}
	config := `{"name":"pnpm","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$path":"node_modules"}}}`
	if err := os.WriteFile(filepath.Join(dir, "default.project.json"), []byte(config), 0o666); err != nil {
		t.Fatal(err)
	}
	nodeModulesConfig := `{"name":"node_modules","tree":{"@rbxts":{"dummy":{"$path":".pnpm/@rbxts+dummy@1.0.0/node_modules/@rbxts/dummy"},"linked":{"$path":".pnpm/@rbxts+linked@1.0.0/node_modules/@rbxts/linked"}}}}`
	if err := os.WriteFile(filepath.Join(dir, "node_modules", "default.project.json"), []byte(nodeModulesConfig), 0o666); err != nil {
		t.Fatal(err)
	}

	files, diags, err := CompileProject(dir)
	if err != nil {
		t.Fatalf("CompileProject: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}

	want := "-- Compiled with sloptor v2.6.0\n" +
		"local TS = require(script.Parent.include.RuntimeLib)\n" +
		"local dummy = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n" +
		"local linked = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"linked\").linked\n" +
		"print(dummy(), linked())\n" +
		"return nil\n"
	if got := files["out/main.luau"]; got != want {
		t.Errorf("out/main.luau:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestCompileProjectExcludesWorkspaceDependencySource(t *testing.T) {
	workspace := t.TempDir()
	game := filepath.Join(workspace, "e2e", "default")
	dependency := filepath.Join(workspace, "packages", "core")

	gameFiles := map[string]string{
		"package.json":         `{"name":"flux-e2e"}`,
		"default.project.json": `{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"},"rbxts_include":{"$className":"Folder","node_modules":{"$className":"Folder","@flux":{"$path":"node_modules/@flux"}}}}}}`,
		"tsconfig.json": `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "Preserve",
		"moduleResolution": "Bundler",
		"customConditions": ["source"],
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
		"src/main.ts": `import { value } from "@flux/core";
import { first } from "./first";
import { second } from "./second";
import { third } from "./third";
print(value, first, second, third);`,
		"src/first.ts":  "export const first = 1;\n",
		"src/second.ts": "export const second = 2;\n",
		"src/third.ts":  "export const third = 3;\n",
	}
	dependencyFiles := map[string]string{
		"package.json":         `{"name":"@flux/core","version":"1.0.0","main":"dist/init.luau","types":"dist/index.d.ts","exports":{".":{"source":"./src/index.ts","import":"./dist/init.luau","types":"./dist/index.d.ts"}}}`,
		"default.project.json": `{"name":"core","tree":{"$path":"dist"}}`,
		"src/index.ts":         "export const value = 42;\n",
		"dist/index.d.ts":      "export declare const value: number;\n",
		"dist/init.luau":       "return { value = 42 }\n",
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
	if len(files) != 4 {
		t.Fatalf("produced %d files, want the 4 game files only (%v)", len(files), keys(files))
	}
	main := files["out/main.luau"]
	if !strings.Contains(main, `"rbxts_include", "node_modules", "@flux", "core"`) {
		t.Fatalf("workspace import did not target the package runtime:\n%s", main)
	}
	for _, name := range []string{"first", "second", "third"} {
		if _, ok := files["out/"+name+".luau"]; !ok {
			t.Errorf("out/%s.luau missing (%v)", name, keys(files))
		}
	}
}
