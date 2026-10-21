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
				const wantSalt = "a416de476645ee5ff85d03b4c989c0cebb3145d9cc9f15fadbd86e378ab7c33b"
				if got := portabilityManifestSalt(t, manifests[0]); got != wantSalt {
					t.Errorf("Rojo salt = %s, want portable salt %s", got, wantSalt)
				}
			}
			if fixture.name == "source-root" {
				// Windows golden: /sources is URL metadata, not an absolute
				// filesystem path to relativize on Unix.
				const wantSalt = "7643e41ffcc2ce1a1ddafcc3dabc28ba0420c8b309b0b57b2adb44c896628e3d"
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
	for _, mode := range []string{"realpaths", "preserveSymlinks", "sibling-manifest", "runtime-package", "transformer"} {
		t.Run(mode, func(t *testing.T) {
			var dirs []string
			var manifests [][]byte
			var manifestPaths []string
			for i := range 2 {
				packageName := "portable-types"
				if mode == "runtime-package" {
					packageName = "@rbxts/portable-types"
				}
				dir := writeProject(t, "@scope/portable-linked", "")
				if mode == "sibling-manifest" {
					project := filepath.Join(t.TempDir(), "project")
					if err := os.CopyFS(project, os.DirFS(dir)); err != nil {
						t.Fatal(err)
					}
					dir = project
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
				modules := filepath.Dir(link)
				if err := os.MkdirAll(modules, 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.Symlink(store, link); err != nil {
					if runtime.GOOS != "windows" {
						t.Skipf("cannot create package symlink: %v", err)
					}
					if output, err := exec.Command("cmd", "/c", "mklink", "/J", link, store).CombinedOutput(); err != nil {
						t.Fatalf("create package junction: %v: %s", err, output)
					}
				}
				main := "import type { Value } from '" + packageName + "';\nexport const value: Value = { value: 1 };\n"
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
				t.Logf("%s portable salt: %s", mode, portabilityManifestSalt(t, manifest))
			}
			if !bytes.Equal(manifests[0], manifests[1]) {
				t.Errorf("moving the package store changed the manifest:\n%s\n%s", manifests[0], manifests[1])
			}
			if mode == "runtime-package" {
				// Linux golden includes a wildcard suffix with a mixed-case
				// filename; native lookup canonicalization must not leak in.
				const wantSalt = "069a17089d23bbb2021e6bd99928be5f1ead8b3367eaa5286321778c04c06953"
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
			packageName := "portable-types"
			if mode == "runtime-package" {
				packageName = "@rbxts/portable-types"
			}
			writeFlameworkIncrementalFile(t, filepath.Join(dirs[1], "node_modules", filepath.FromSlash(packageName), "details.d.ts"), `export interface Value { readonly value: string; }`)
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
					const wantSalt = "f4d3d6c99bfcdde8278b8a83c81cdbde58a245a89e62aae8d7b6f386899180d3"
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
