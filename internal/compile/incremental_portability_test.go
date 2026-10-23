package compile

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestBuildProjectIncrementalManifestSurvivesCheckoutMove(t *testing.T) {
	game := `{"name":"portable","tree":{"$className":"DataModel","ReplicatedStorage":{"code":{"$path":"out"},"include":{"$path":"include"}}}}`
	for _, fixture := range []struct {
		name, rojo string
	}{
		{"package", ""},
		{"game", game},
		{"flamework", game},
		{"compiler-paths", ""},
		{"source-root", ""},
		{"rojo-files", strings.Replace(game, `"code":`, `"alpha":{"$path":"extra/A/x.lua"},"beta":{"$path":"extra/AB.lua"},"code":`, 1)},
	} {
		t.Run(fixture.name, func(t *testing.T) {
			var dirs []string
			var manifests [][]byte
			for range 2 {
				dir := writeProject(t, "@scope/portable", fixture.rojo)
				enableIncrementalBuilds(t, dir)
				writeIncrementalFixture(t, dir)
				if fixture.name == "rojo-files" {
					writeFlameworkIncrementalFile(t, filepath.Join(dir, "extra", "A", "x.lua"), "return {}\n")
					writeFlameworkIncrementalFile(t, filepath.Join(dir, "extra", "AB.lua"), "return {}\n")
				}
				if fixture.name == "flamework" {
					writeFlameworkIncrementalFile(t, filepath.Join(dir, "rotor.toml"), "[flamework]\nidGenerationMode = \"full\"\n")
					writeFlameworkIncrementalFile(t, filepath.Join(dir, "flamework.build"), `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"metadata":{"globs":{"paths":{"assets/*.txt":[]},"origins":{"manual":["assets/*.txt"]}}}}`)
					writeFlameworkIncrementalFile(t, filepath.Join(dir, "assets", "test.txt"), "asset\n")
				}
				if fixture.name == "compiler-paths" {
					setPortabilityCompilerOptions(t, dir, map[string]any{
						"rootDirs": []string{"src", "generated"}, "paths": map[string][]string{"@fixture/*": {"./src/*"}},
						"declaration": true, "declarationDir": "types",
					})
				}
				if fixture.name == "source-root" {
					setPortabilityCompilerOptions(t, dir, map[string]any{"sourceMap": true, "sourceRoot": "/sources", "mapRoot": "https://example.com/maps"})
				}
				if err := os.MkdirAll(filepath.Join(dir, "include"), 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(filepath.Join(dir, "include", "RuntimeLib.lua"), []byte("return {}\n"), 0o644); err != nil {
					t.Fatal(err)
				}
				if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
					t.Fatalf("seed build: %v (diags: %v)", err, diags)
				}
				manifest, err := os.ReadFile(filepath.Join(dir, "out", "cache.rbxtsc.tsbuildinfo"))
				if err != nil {
					t.Fatal(err)
				}
				dirs = append(dirs, dir)
				manifests = append(manifests, manifest)
			}
			if !bytes.Equal(manifests[0], manifests[1]) {
				t.Errorf("identical checkouts produced different manifests:\n%s\n%s", manifests[0], manifests[1])
			}
			t.Logf("portable manifest SHA256: %x", sha256.Sum256(manifests[0]))
			t.Logf("portable manifest salt: %s", portabilityManifestSalt(t, manifests[0]))
			if fixture.name == "rojo-files" {
				// Linux golden: A/x.lua sorts before AB.lua with slash
				// separators, but the native Windows order is reversed.
				const wantSalt = "9e1336ec612dbb61d6b398072bdc840c8b624d738102ec98d1bf975acc386d68"
				if got := portabilityManifestSalt(t, manifests[0]); got != wantSalt {
					t.Errorf("Rojo salt = %s, want portable salt %s", got, wantSalt)
				}
			}
			if fixture.name == "source-root" {
				// Windows golden: /sources is URL metadata, not an absolute
				// filesystem path to relativize on Unix.
				const wantSalt = "f1fdfbd71a85dcfba3aaa419fb323b303e59c37fe262a58ecfd68f2f52cd6b0a"
				if got := portabilityManifestSalt(t, manifests[0]); got != wantSalt {
					t.Errorf("sourceRoot salt = %s, want portable salt %s", got, wantSalt)
				}
			}
			manifestPath := filepath.Join(dirs[1], "out", "cache.rbxtsc.tsbuildinfo")
			if err := os.WriteFile(manifestPath, manifests[0], 0o644); err != nil {
				t.Fatal(err)
			}
			timings := NewBuildTimings()
			result, diags, err := BuildProjectWithOptions(dirs[1], ProjectOptions{Timings: timings})
			if err != nil || len(diags) != 0 {
				t.Fatalf("restored build: %v (diags: %v)", err, diags)
			}
			if timings.Counts.SelectedSources != 0 || timings.Counts.ActualWrites != 0 || len(result.EmittedFiles) != 0 {
				t.Fatalf("restored build selected %d sources, wrote %d files, emitted %v; want a no-op", timings.Counts.SelectedSources, timings.Counts.ActualWrites, result.EmittedFiles)
			}
			writeFlameworkIncrementalFile(t, filepath.Join(dirs[1], "src", "util.ts"), "export const VALUE = 2;\n")
			timings = NewBuildTimings()
			if _, diags, err := BuildProjectWithOptions(dirs[1], ProjectOptions{Timings: timings}); err != nil || len(diags) != 0 {
				t.Fatalf("changed build: %v (diags: %v)", err, diags)
			}
			if timings.Counts.SelectedSources == 0 || timings.Counts.ActualWrites == 0 {
				t.Fatal("restored manifest failed to invalidate changed sources")
			}
		})
	}
}

func TestBuildProjectIncrementalManifestSurvivesPackageStoreMove(t *testing.T) {
	for _, name := range []string{"realpaths", "preserveSymlinks", "sibling-manifest", "runtime-package", "transformer", "virtual-store", "transitive-store", "workspace-store", "hoisted-store", "workspace-hoisted-store", "aliased-transitive-store", "aliased-workspace-store", "aliased-hoisted-store", "aliased-workspace-hoisted-store"} {
		t.Run(name, func(t *testing.T) {
			mode, aliased := strings.CutPrefix(name, "aliased-")
			workspaceStore := mode == "workspace-store" || mode == "workspace-hoisted-store"
			hoistedStore := mode == "hoisted-store" || mode == "workspace-hoisted-store"
			transitiveStore := mode == "transitive-store" || workspaceStore || hoistedStore
			var dirs []string
			var manifests [][]byte
			var manifestPaths []string
			var declarationPaths []string
			for i := range 2 {
				packageName := "portable-types"
				if mode == "runtime-package" {
					packageName = "@rbxts/portable-types"
				}
				if mode == "workspace-hoisted-store" {
					packageName = "@scope/portable-types"
				}
				dir := writeProject(t, "@scope/portable-linked", "")
				if mode == "sibling-manifest" || workspaceStore {
					projectPath := "project"
					if workspaceStore {
						projectPath = filepath.Join("packages", "app")
					}
					project := filepath.Join(t.TempDir(), projectPath)
					if err := os.CopyFS(project, os.DirFS(dir)); err != nil {
						t.Fatal(err)
					}
					dir = project
				}
				if aliased {
					root, projectPath := dir, "."
					if workspaceStore {
						root = filepath.Clean(filepath.Join(dir, "..", ".."))
						projectPath = filepath.Join("packages", "app")
					}
					alias := filepath.Join(t.TempDir(), "checkout")
					linkPortabilityDirectory(t, root, alias)
					dir = filepath.Join(alias, projectPath)
				}
				enableIncrementalBuilds(t, dir)
				manifestPath := filepath.Join(dir, "out", "cache.rbxtsc.tsbuildinfo")
				if mode == "sibling-manifest" {
					setPortabilityCompilerOptions(t, dir, map[string]any{"tsBuildInfoFile": "../cache/build.tsbuildinfo"})
					manifestPath = filepath.Join(dir, "..", "cache", "build.rbxtsc.tsbuildinfo")
				}
				if mode == "preserveSymlinks" {
					setPortabilityCompilerOptions(t, dir, map[string]any{"preserveSymlinks": true})
				}
				store := t.TempDir()
				packageRoot := dir
				if workspaceStore {
					packageRoot = filepath.Clean(filepath.Join(dir, "..", ".."))
				}
				if mode == "virtual-store" || hoistedStore {
					// pnpm truncates virtual-store names at different lengths
					// on Windows and Unix. Logical package links stay the same.
					storeNames := []string{"portable-types@1.0.0_peer-package@1.0.0_another-peer-package@1.0.0", "portable-types@1.0.0_abcd1234"}
					store = filepath.Join(packageRoot, "node_modules", ".pnpm", storeNames[i], "node_modules", packageName)
					if err := os.MkdirAll(store, 0o755); err != nil {
						t.Fatal(err)
					}
				}
				if transitiveStore && !hoistedStore {
					store = filepath.Join(packageRoot, "node_modules", ".pnpm", "portable-types@1.0.0", "node_modules", packageName)
					if err := os.MkdirAll(store, 0o755); err != nil {
						t.Fatal(err)
					}
				}
				if mode == "sibling-manifest" {
					storeName := []string{"store-one", "store-two"}[i]
					store = filepath.Join(dir, "..", storeName)
					if err := os.MkdirAll(store, 0o755); err != nil {
						t.Fatal(err)
					}
				}
				for name, contents := range map[string]string{
					"package.json": `{"name":"` + packageName + `","types":"index.d.ts","main":"init.luau"}`,
					"index.d.ts":   `export { Value } from "./details"; export declare const answer: number;`,
					"details.d.ts": `export interface Value { readonly value: number; }`,
					"init.luau":    `return { answer = 1 }`,
				} {
					if err := os.WriteFile(filepath.Join(store, name), []byte(contents), 0o644); err != nil {
						t.Fatal(err)
					}
				}
				if mode == "runtime-package" {
					writeProjectFile(t, store, "package.json", `{"name":"@rbxts/portable-types","types":"index.d.ts","main":"out/init.luau","exports":{".":{"types":"./index.d.ts","default":"./out/init.luau"},"./*":{"types":"./types/*/Index.d.ts","default":"./out/*/init.luau"}}}`)
					writeProjectFile(t, store, "out/init.luau", `return { answer = 1 }`)
					writeProjectFile(t, store, "default.project.json", `{"name":"portable-types","tree":{"$path":"out"}}`)
				}
				link := filepath.Join(dir, "node_modules", filepath.FromSlash(packageName))
				main := "import type { Value } from '" + packageName + "';\nexport const value: Value = { value: 1 };\n"
				if transitiveStore {
					parentNames := []string{"parent-types@1.0.0_peer-package@1.0.0_another-peer-package@1.0.0", "parent-types@1.0.0_abcd1234"}
					parentName := parentNames[i]
					if hoistedStore {
						parentName = "parent-types@1.0.0"
					}
					parent := filepath.Join(packageRoot, "node_modules", ".pnpm", parentName, "node_modules", "parent-types")
					writeProjectFile(t, parent, "package.json", `{"name":"parent-types","types":"index.d.ts"}`)
					writeProjectFile(t, parent, "index.d.ts", `export { Value } from "`+packageName+`";`)
					linkPortabilityDirectory(t, parent, filepath.Join(packageRoot, "node_modules", "parent-types"))
					link = filepath.Join(filepath.Dir(parent), packageName)
					if hoistedStore {
						link = filepath.Join(packageRoot, "node_modules", ".pnpm", "node_modules", packageName)
					}
					main = "import type { Value } from 'parent-types';\nexport const value: Value = { value: 1 };\n"
				}
				linkPortabilityDirectory(t, store, link)
				if mode == "runtime-package" {
					main = "import { answer } from '" + packageName + "';\nimport type { Value } from '" + packageName + "';\nexport const value: Value = { value: answer };\n"
				}
				if mode == "transformer" {
					setRepoSidecarPath(t)
					closeSidecarSessions()
					t.Cleanup(closeSidecarSessions)
					symlinkFixtureTypeScript(t, dir)
					writeProjectFile(t, dir, "node_modules/introduced-types/package.json", `{"name":"introduced-types","types":"index.d.ts"}`)
					writeProjectFile(t, dir, "node_modules/introduced-types/index.d.ts", `export interface Added { readonly value: number; }`)
					// Replacing the linked import with a previously unloaded
					// declaration forces a fresh transformed program. Its
					// symlink cache no longer knows the original package.
					writeProjectFile(t, dir, "plugins/replace-import.js", `const ts = require("typescript");
module.exports = () => () => sourceFile => {
	return ts.createSourceFile(sourceFile.fileName,
		'import type { Added } from "introduced-types"; export const value: Added = { value: 1 };',
		ts.ScriptTarget.Latest, true);
};`)
					writePluginTsconfig(t, dir, map[string]any{"transform": "./plugins/replace-import.js"})
				}
				if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte(main), 0o644); err != nil {
					t.Fatal(err)
				}
				if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
					t.Fatalf("seed build: %v (diags: %v)", err, diags)
				}
				manifest, err := os.ReadFile(manifestPath)
				if err != nil {
					t.Fatal(err)
				}
				if !strings.Contains(string(manifest), "node_modules/"+packageName+"/details.d.ts") {
					t.Errorf("manifest does not use the package link path:\n%s", manifest)
				}
				dirs = append(dirs, dir)
				manifests = append(manifests, manifest)
				manifestPaths = append(manifestPaths, manifestPath)
				declarationPaths = append(declarationPaths, filepath.Join(store, "details.d.ts"))
				t.Logf("%s portable salt: %s", mode, portabilityManifestSalt(t, manifest))
			}
			if !bytes.Equal(manifests[0], manifests[1]) {
				t.Errorf("moving the package store changed the manifest:\n%s\n%s", manifests[0], manifests[1])
			}
			if mode == "runtime-package" {
				// Linux golden includes a wildcard suffix with a mixed-case
				// filename; native lookup canonicalization must not leak in.
				const wantSalt = "9641a34feda1634d4e968fda1c9d64316d90940198b902b43976047ad3d97081"
				if got := portabilityManifestSalt(t, manifests[0]); got != wantSalt {
					t.Errorf("package salt = %s, want portable salt %s", got, wantSalt)
				}
			}
			if err := os.WriteFile(manifestPaths[1], manifests[0], 0o644); err != nil {
				t.Fatal(err)
			}
			timings := NewBuildTimings()
			if _, diags, err := BuildProjectWithOptions(dirs[1], ProjectOptions{Timings: timings}); err != nil || len(diags) != 0 {
				t.Fatalf("restored build: %v (diags: %v)", err, diags)
			}
			if timings.Counts.SelectedSources != 0 || timings.Counts.ActualWrites != 0 {
				t.Fatalf("restored build selected %d sources and wrote %d files; want a no-op", timings.Counts.SelectedSources, timings.Counts.ActualWrites)
			}
			// Changing a declaration in the new store must still reach its
			// importer through the graph restored from the old checkout.
			writeFlameworkIncrementalFile(t, declarationPaths[1], `export interface Value { readonly value: string; }`)
			timings = NewBuildTimings()
			_, _, err := BuildProjectWithOptions(dirs[1], ProjectOptions{Timings: timings})
			if mode == "transformer" {
				if err != nil || timings.Counts.SelectedSources == 0 {
					t.Fatalf("changed original dependency selected %d sources: %v", timings.Counts.SelectedSources, err)
				}
			} else if err == nil {
				t.Fatal("restored manifest hid a dependency type error")
			}
		})
	}
}

func linkPortabilityDirectory(t *testing.T, target, link string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(link), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, link); err != nil {
		if runtime.GOOS != "windows" {
			t.Skipf("cannot create package symlink: %v", err)
		}
		if output, err := exec.Command("cmd", "/c", "mklink", "/J", link, target).CombinedOutput(); err != nil {
			t.Fatalf("create fixture junction: %v: %s", err, output)
		}
	}
}

func setPortabilityCompilerOptions(t *testing.T, dir string, overrides map[string]any) {
	t.Helper()
	configPath := filepath.Join(dir, "tsconfig.json")
	data, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal(data, &config); err != nil {
		t.Fatal(err)
	}
	options, ok := config["compilerOptions"].(map[string]any)
	if !ok {
		t.Fatal("fixture has no compilerOptions object")
	}
	maps.Copy(options, overrides)
	data, err = json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(configPath, data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestBuildSolutionIncrementalManifestsSurviveCheckoutMove(t *testing.T) {
	var fixtures []*referenceFixture
	manifests := map[string][]byte{}
	for i := range 2 {
		f := newReferenceFixture(t)
		f.project("shared", nil, nil)
		f.project("game", []string{"shared"}, nil)
		f.write("shared/src/SomeModule.ts", "export const value = 1;\n")
		f.write("game/src/index.ts", `import { value } from "../../shared/src/SomeModule"; export const result = value;`)
		f.build()
		for _, project := range []string{"shared", "game"} {
			path := filepath.Join(f.dir, project, "cache", project+".rbxtsc.tsbuildinfo")
			manifest, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			if i == 0 {
				manifests[project] = manifest
				t.Logf("%s portable salt: %s", project, portabilityManifestSalt(t, manifest))
				if project == "game" {
					// Linux golden retains the SomeModule.ts spelling.
					const wantSalt = "5905463aa1a43803b2844578a584cfbcc31549f41b15afc2b87ddf20971708ca"
					if got := portabilityManifestSalt(t, manifest); got != wantSalt {
						t.Errorf("reference salt = %s, want portable salt %s", got, wantSalt)
					}
				}
			} else {
				if !bytes.Equal(manifests[project], manifest) {
					t.Errorf("%s manifest changed between checkouts:\n%s\n%s", project, manifests[project], manifest)
				}
				if err := os.WriteFile(path, manifests[project], 0o644); err != nil {
					t.Fatal(err)
				}
			}
		}
		fixtures = append(fixtures, f)
	}
	f := fixtures[1]
	for _, project := range []string{"shared", "game"} {
		opts := f.options()
		opts.TsConfigPath = filepath.Join(f.dir, project, "tsconfig.json")
		opts.Timings = NewBuildTimings()
		if _, diags, err := BuildProjectWithOptions(filepath.Join(f.dir, project), opts); err != nil || len(diags) != 0 {
			t.Fatalf("restored %s build: %v (diags: %v)", project, err, diags)
		}
		if opts.Timings.Counts.SelectedSources != 0 || opts.Timings.Counts.ActualWrites != 0 {
			t.Errorf("restored %s selected %d sources and wrote %d files", project, opts.Timings.Counts.SelectedSources, opts.Timings.Counts.ActualWrites)
		}
	}
}

func portabilityManifestSalt(t *testing.T, manifest []byte) string {
	t.Helper()
	var stored struct {
		Salt string `json:"salt"`
	}
	if err := json.Unmarshal(manifest, &stored); err != nil {
		t.Fatal(err)
	}
	return stored.Salt
}

func TestIncrementalPathTemplatesPreserveWildcardSubstitution(t *testing.T) {
	dir := writeProject(t, "@scope/wildcard-paths", "")
	enableIncrementalBuilds(t, dir)
	writeProjectFile(t, dir, "src/main.ts", `import { value } from "@/a/b"; export const result = value;`)
	writeProjectFile(t, dir, "src/a/value.ts", "export const value = 1;\n")
	writeProjectFile(t, dir, "src/value.ts", "export const value = 2;\n")
	setPortabilityCompilerOptions(t, dir, map[string]any{"paths": map[string][]string{"@/*": {"./src/*/../value"}}})
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
		t.Fatalf("seed build: %v (diags: %v)", err, diags)
	}
	setPortabilityCompilerOptions(t, dir, map[string]any{"paths": map[string][]string{"@/*": {"./src/value"}}})
	timings := NewBuildTimings()
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil || len(diags) != 0 {
		t.Fatalf("changed build: %v (diags: %v)", err, diags)
	}
	if timings.Counts.SelectedSources == 0 {
		t.Fatal("wildcard substitution change selected no sources")
	}
	clean, diags, err := CompileProject(dir)
	if err != nil || len(diags) != 0 {
		t.Fatalf("clean compile: %v (diags: %v)", err, diags)
	}
	got, err := os.ReadFile(filepath.Join(dir, "out", "main.luau"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != clean["out/main.luau"] {
		t.Fatalf("wildcard substitution left stale output:\n%s\nwant:\n%s", got, clean["out/main.luau"])
	}
}

func TestIncrementalInheritedPathsPreserveTheirBaseDirectory(t *testing.T) {
	dir := writeProject(t, "@scope/inherited-paths", "")
	enableIncrementalBuilds(t, dir)
	writeProjectFile(t, dir, "src/main.ts", `import { value } from "alias"; export const result = value;`)
	writeProjectFile(t, dir, "src/value.ts", "export const value = 1;\n")
	writeProjectFile(t, dir, "config/src/value.ts", "export const value = 2;\n")
	setPortabilityCompilerOptions(t, dir, map[string]any{"rootDir": "."})
	configPath := filepath.Join(dir, "tsconfig.json")
	data, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal(data, &config); err != nil {
		t.Fatal(err)
	}
	config["extends"] = "./config/base.json"
	config["include"] = []string{"src", "config/src"}
	data, err = json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(configPath, data, 0o644); err != nil {
		t.Fatal(err)
	}
	writeBase := func(target string) {
		t.Helper()
		data, err := json.Marshal(map[string]any{"compilerOptions": map[string]any{"paths": map[string][]string{"alias": {target}}}})
		if err != nil {
			t.Fatal(err)
		}
		writeProjectFile(t, dir, "config/base.json", string(data))
	}
	writeBase(filepath.ToSlash(filepath.Join(dir, "src", "value")))
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
		t.Fatalf("seed build: %v (diags: %v)", err, diags)
	}
	writeBase("./src/value")
	timings := NewBuildTimings()
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil || len(diags) != 0 {
		t.Fatalf("changed build: %v (diags: %v)", err, diags)
	}
	if timings.Counts.SelectedSources == 0 {
		t.Fatal("inherited paths target change selected no sources")
	}
	clean, diags, err := CompileProject(dir)
	if err != nil || len(diags) != 0 {
		t.Fatalf("clean compile: %v (diags: %v)", err, diags)
	}
	got, err := os.ReadFile(filepath.Join(dir, "out", "src", "main.luau"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != clean["out/src/main.luau"] {
		t.Fatalf("inherited paths left stale output:\n%s\nwant:\n%s", got, clean["out/src/main.luau"])
	}
}

func TestIncrementalManifestOnAnotherVolumeSurvivesCheckoutMove(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("Windows drive-relative path semantics")
	}
	cacheBase, err := os.UserCacheDir()
	if err != nil {
		t.Fatal(err)
	}
	if strings.EqualFold(filepath.VolumeName(cacheBase), filepath.VolumeName(t.TempDir())) {
		t.Skip("requires a cache directory on another volume")
	}
	cacheDir, err := os.MkdirTemp(cacheBase, "sloptor-portable-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.RemoveAll(cacheDir); err != nil {
			t.Error(err)
		}
	})
	var dirs, manifestPaths []string
	var manifests [][]byte
	for i := range 3 {
		dir := writeProject(t, "@scope/cross-volume", "")
		if i > 0 {
			project := filepath.Join(cacheDir, "project")
			if i == 2 {
				project = cacheDir
			}
			if err := os.CopyFS(project, os.DirFS(dir)); err != nil {
				t.Fatal(err)
			}
			dir = project
		}
		enableIncrementalBuilds(t, dir)
		writeIncrementalFixture(t, dir)
		cache := filepath.Join(cacheDir, "cache", "cache.tsbuildinfo")
		setPortabilityCompilerOptions(t, dir, map[string]any{"tsBuildInfoFile": filepath.ToSlash(cache)})
		if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
			t.Fatalf("seed build: %v (diags: %v)", err, diags)
		}
		manifestPath := strings.TrimSuffix(cache, ".tsbuildinfo") + ".rbxtsc.tsbuildinfo"
		manifest, err := os.ReadFile(manifestPath)
		if err != nil {
			t.Fatal(err)
		}
		dirs = append(dirs, dir)
		manifestPaths = append(manifestPaths, manifestPath)
		manifests = append(manifests, manifest)
	}
	for i := 1; i < len(manifests); i++ {
		if !bytes.Equal(manifests[0], manifests[i]) {
			t.Errorf("cross-volume manifest %d differs:\n%s\n%s", i, manifests[0], manifests[i])
		}
	}
	last := len(dirs) - 1
	if err := os.WriteFile(manifestPaths[last], manifests[0], 0o644); err != nil {
		t.Fatal(err)
	}
	timings := NewBuildTimings()
	if _, diags, err := BuildProjectWithOptions(dirs[last], ProjectOptions{Timings: timings}); err != nil || len(diags) != 0 {
		t.Fatalf("restored build: %v (diags: %v)", err, diags)
	}
	if timings.Counts.SelectedSources != 0 || timings.Counts.ActualWrites != 0 {
		t.Fatalf("restored cross-volume build selected %d and wrote %d", timings.Counts.SelectedSources, timings.Counts.ActualWrites)
	}
}

func TestIncrementalWorkspaceRojoPathsRemainDistinct(t *testing.T) {
	game := `{"name":"workspace","tree":{"$className":"DataModel","ReplicatedStorage":{"code":{"$path":"packages/shared/out"},"include":{"$path":"include"}}}}`
	dir := writeProject(t, "@scope/workspace", game)
	alias := filepath.Join(t.TempDir(), "checkout")
	linkPortabilityDirectory(t, dir, alias)
	dir = alias
	enableIncrementalBuilds(t, dir)
	setPortabilityCompilerOptions(t, dir, map[string]any{"outDir": "packages/shared/out"})
	writeProjectFile(t, dir, "packages/shared/package.json", `{"name":"workspace-link","types":"index.d.ts"}`)
	writeProjectFile(t, dir, "packages/shared/index.d.ts", `export interface Value { readonly value: number; }`)
	writeProjectFile(t, dir, "src/main.ts", `import type { Value } from "workspace-link"; import { VALUE } from "./util"; export const value: Value = { value: VALUE };`)
	writeProjectFile(t, dir, "src/util.ts", `export const VALUE = 1;`)
	writeProjectFile(t, dir, "include/RuntimeLib.lua", "return {}\n")
	if err := os.MkdirAll(filepath.Join(dir, "node_modules"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(dir, "packages", "shared"), filepath.Join(dir, "node_modules", "workspace-link")); err != nil {
		t.Fatal(err)
	}
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
		t.Fatalf("seed build: %v (diags: %v)", err, diags)
	}
	manifest := mustReadFile(t, filepath.Join(dir, "out", "cache.rbxtsc.tsbuildinfo"))
	if !bytes.Contains(manifest, []byte("project://packages/shared/index.d.ts")) {
		t.Fatalf("workspace input lost its project-relative identity:\n%s", manifest)
	}
	writeProjectFile(t, dir, "default.project.json", strings.Replace(game, "packages/shared/out", "node_modules/workspace-link/out", 1))
	_, cleanDiags, cleanErr := CompileProject(dir)
	if cleanErr == nil || !strings.Contains(strings.Join(cleanDiags, "\n"), "Could not find Rojo data") {
		t.Fatalf("clean compile accepted a mount that does not cover the physical output: %v %v", cleanErr, cleanDiags)
	}
	timings := NewBuildTimings()
	_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings})
	if timings.Counts.SelectedSources == 0 || err == nil || !strings.Contains(strings.Join(diags, "\n"), "Could not find Rojo data") {
		t.Fatalf("changed Rojo mount selected %d sources: %v (diags: %v); want the clean compile failure", timings.Counts.SelectedSources, err, diags)
	}
}

func TestIncrementalSaltUsesCachedProjectReferencePaths(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	f.write("shared/src/SomeModule.ts", "export const value = 1;\n")
	sharedConfig := filepath.Join(f.dir, "shared", "tsconfig.json")
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(t, sharedConfig), &config); err != nil {
		t.Fatal(err)
	}
	config["rbxts"] = map[string]any{"luau": false}
	f.json("shared/tsconfig.json", config)
	f.build()
	configPath := filepath.Join(f.dir, "game", "tsconfig.json")
	graph, err := BuildSolutionGraph(configPath, f.options())
	if err != nil {
		t.Fatal(err)
	}
	populateCrossProjectMetadata(graph)
	var cachedOptions ProjectOptions
	for _, project := range graph.Projects {
		if project.ConfigPath == configPath {
			cachedOptions = project.Options
		}
	}
	dir, program, diags, err := newProjectProgram(filepath.Dir(configPath), configPath)
	if err != nil || len(diags) != 0 {
		t.Fatalf("program: %v (diags: %v)", err, diags)
	}
	direct, err := incrementalSaltWithFlamework(dir, program, f.options(), "", nil)
	if err != nil {
		t.Fatal(err)
	}
	cached, err := incrementalSaltWithFlamework(dir, program, cachedOptions, "", nil)
	if err != nil || cached != direct {
		t.Fatalf("cached salt = %s, direct = %s: %v", cached, direct, err)
	}
	// Metadata and Program belong to the same snapshot. A salt calculation
	// must not re-read each reference's options behind that snapshot.
	config["rbxts"] = map[string]any{"luau": true}
	f.json("shared/tsconfig.json", config)
	after, err := incrementalSaltWithFlamework(dir, program, cachedOptions, "", nil)
	if err != nil || after != cached {
		t.Fatalf("salt re-read reference options: before = %s, after = %s: %v", cached, after, err)
	}
}
