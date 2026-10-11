package compile

import (
	"encoding/json"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// Mirrors upstream tests/compiler/referenceFixture.ts: sibling projects share
// a base config, output mounts, and the root runtime library.
type referenceFixture struct {
	t        testing.TB
	dir      string
	projects []string
}

func newReferenceFixture(t testing.TB) *referenceFixture {
	t.Helper()
	return newReferenceFixtureAt(t, t.TempDir())
}

func newReferenceFixtureAt(t *testing.T, dir string) *referenceFixture {
	t.Helper()
	f := &referenceFixture{t: t, dir: dir}
	f.json("package.json", map[string]any{"name": "reference-fixture"})
	f.json("base.json", map[string]any{"compilerOptions": map[string]any{
		"allowSyntheticDefaultImports": true, "module": "commonjs",
		"moduleResolution": "Node", "moduleDetection": "force", "noLib": true,
		"strict": true, "target": "ESNext", "incremental": true,
		"skipLibCheck": true, "typeRoots": []string{"node_modules/@rbxts"},
		"types": []string{"globals"},
	}})
	f.write("node_modules/@rbxts/globals/index.d.ts", noLibGlobalStubs)
	f.write("include/RuntimeLib.lua", "return {}\n")
	return f
}

func (f *referenceFixture) write(path, text string) {
	f.t.Helper()
	writeSolutionFile(f.t, f.dir, path, text)
}

func (f *referenceFixture) json(path string, value any) {
	f.t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		f.t.Fatal(err)
	}
	f.write(path, string(data))
}

func (f *referenceFixture) project(name string, references []string, overrides map[string]any) {
	f.t.Helper()
	f.projects = append(f.projects, name)
	options := map[string]any{
		"rootDir": "src", "outDir": "out",
		"tsBuildInfoFile": "cache/" + name + ".tsbuildinfo", "composite": name != "game",
	}
	for key, value := range overrides {
		options[key] = value
	}
	refs := []map[string]string{}
	for _, reference := range references {
		refs = append(refs, map[string]string{"path": "../" + reference})
	}
	f.json(name+"/tsconfig.json", map[string]any{
		"extends": "../base.json", "compilerOptions": options,
		"include": []string{"src"}, "references": refs,
	})
	f.write(name+"/src/index.ts", "export const value = 1;\n")
	f.rojo("default.project.json", "", nil)
}

func (f *referenceFixture) rojo(path, prefix string, overrides map[string]any) {
	f.t.Helper()
	mounts := map[string]any{"$className": "ReplicatedStorage", "include": map[string]string{"$path": prefix + "include"}}
	for _, name := range f.projects {
		mounts[name] = map[string]string{"$path": prefix + name + "/out"}
	}
	for name, mount := range overrides {
		mounts[name] = mount
	}
	f.json(path, map[string]any{"name": "references", "tree": map[string]any{
		"$className": "DataModel", "ReplicatedStorage": mounts,
	}})
}

func (f *referenceFixture) options() ProjectOptions {
	return ProjectOptions{RojoConfigPath: filepath.Join(f.dir, "default.project.json"), IncludePath: filepath.Join(f.dir, "include")}
}

func (f *referenceFixture) build() *BuildResult {
	f.t.Helper()
	result, messages, err := BuildSolutionWithOptions(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		f.t.Fatalf("build: %v (%v)", err, messages)
	}
	return result
}

func TestProjectReferenceSeparateDeclarationDir(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types", "declarationMap": true})
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	// Build declarations before the consumer resolves their directory.
	if _, messages, err := BuildSolutionWithOptions(filepath.Join(f.dir, "shared/tsconfig.json"), f.options()); err != nil {
		t.Fatalf("build shared: %v (%v)", err, messages)
	}
	f.build()
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, `TS.import(script, game:GetService("ReplicatedStorage"), "shared")`) {
		t.Fatalf("import did not use the referenced Luau mount:\n%s", output)
	}
	if _, err := os.Stat(filepath.Join(f.dir, "shared/types/index.d.ts.map")); err != nil {
		t.Fatal(err)
	}
}

func TestProjectReferenceNodeModulesSymlink(t *testing.T) {
	for _, test := range []struct {
		name     string
		preserve bool
	}{
		{"resolved symlink", false}, {"preserved symlink", true},
	} {
		t.Run(test.name, func(t *testing.T) {
			testProjectReferenceNodeModulesSymlink(t, newReferenceFixture(t), test.preserve)
		})
	}
}

// An aliased fixture directory (a junction like an 8.3 short temp path, or a
// symlink) spells project paths differently from their real paths.
func TestProjectReferenceAliasedDirectory(t *testing.T) {
	for _, kind := range []string{"junction", "symlink"} {
		t.Run(kind, func(t *testing.T) {
			testProjectReferenceNodeModulesSymlink(t, newAliasedReferenceFixture(t, kind), false)
		})
	}
}

func TestProjectReferenceAliasedDirectoryRojoMounts(t *testing.T) {
	for _, kind := range []string{"junction", "symlink"} {
		t.Run(kind, func(t *testing.T) {
			f := newAliasedReferenceFixture(t, kind)
			f.project("shared", nil, nil)
			f.project("game", []string{"shared"}, nil)
			f.write("game/src/index.ts", `import { value } from "../../shared/src"; export const result = value;`)
			f.json("shared/tsconfig.json", map[string]any{
				"extends": "../base.json", "compilerOptions": map[string]any{
					"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/shared.tsbuildinfo",
				}, "include": []string{"src"}, "rbxts": map[string]string{"type": "game", "rojo": "owner.project.json"},
			})
			f.rojo("shared/owner.project.json", "../", nil)
			f.build()
		})
	}
}

func newAliasedReferenceFixture(t *testing.T, kind string) *referenceFixture {
	t.Helper()
	real := filepath.Join(t.TempDir(), "real")
	alias := filepath.Join(t.TempDir(), "alias")
	if err := os.Mkdir(real, 0o755); err != nil {
		t.Fatal(err)
	}
	if kind == "junction" {
		if runtime.GOOS != "windows" {
			t.Skip("directory junctions are Windows-only")
		}
		if err := exec.Command("cmd", "/c", "mklink", "/J", alias, real).Run(); err != nil {
			t.Skipf("directory junctions unavailable: %v", err)
		}
	} else if err := os.Symlink(real, alias); err != nil {
		t.Skipf("symlinks unavailable: %v", err)
	}
	return newReferenceFixtureAt(t, alias)
}

func testProjectReferenceNodeModulesSymlink(t *testing.T, f *referenceFixture, preserveSymlinks bool) {
	t.Helper()
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, map[string]any{"preserveSymlinks": preserveSymlinks})
	f.json("shared/package.json", map[string]string{"name": "@rbxts/shared", "types": "types/index.d.ts"})
	f.json("shared/tsconfig.json", map[string]any{
		"extends": "../base.json", "compilerOptions": map[string]any{
			"rootDir": "src", "outDir": "out", "declarationDir": "types", "composite": true,
			"tsBuildInfoFile": "cache/shared.tsbuildinfo",
		}, "include": []string{"src"}, "rbxts": map[string]string{"type": "game", "rojo": "../default.project.json"},
	})
	f.write("shared/src/helper.ts", "export const value = 7;\n")
	f.write("game/src/index.ts", `import { value } from "@rbxts/shared/types/helper"; export const result = value;`)
	if err := os.Symlink(filepath.Join(f.dir, "shared"), filepath.Join(f.dir, "node_modules/@rbxts/shared")); err != nil {
		t.Skipf("symlinks unavailable: %v", err)
	}
	if _, messages, err := BuildSolutionWithOptions(filepath.Join(f.dir, "shared/tsconfig.json"), f.options()); err != nil {
		t.Fatalf("build shared: %v (%v)", err, messages)
	}
	f.build()
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, `TS.import(script, game:GetService("ReplicatedStorage"), "shared", "helper")`) {
		t.Fatalf("symlink import did not use the referenced project's mount:\n%s", output)
	}
}

func TestProjectReferenceRojoMounts(t *testing.T) {
	for _, test := range []struct{ name, path, mount string }{
		{"matching", "", ""},
		{"module", "shared/out/init.luau", "ReplicatedStorage.shared"},
		{"unimported module", "shared/out/tests/check.luau", "ReplicatedStorage.shared.tests.check"},
		{"runtime", "include/RuntimeLib.lua", "ReplicatedStorage.include.RuntimeLib"},
		{"transitive module", "common/out/init.luau", "ReplicatedStorage.common"},
	} {
		t.Run(test.name, func(t *testing.T) {
			f := newReferenceFixture(t)
			f.project("common", nil, nil)
			f.project("shared", []string{"common"}, nil)
			f.project("game", []string{"shared"}, nil)
			f.write("shared/src/tests/check.ts", "export const check = 1;\n")
			f.write("game/src/index.ts", `import { value } from "../../shared/src"; export const result = value;`)
			f.json("shared/tsconfig.json", map[string]any{
				"extends": "../base.json", "compilerOptions": map[string]any{
					"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/shared.tsbuildinfo",
				}, "include": []string{"src"}, "references": []map[string]string{{"path": "../common"}},
				"rbxts": map[string]string{"type": "game", "rojo": "owner.project.json"},
			})
			f.rojo("shared/owner.project.json", "../", nil)
			if test.path != "" {
				f.rojo("default.project.json", "", map[string]any{
					"different": map[string]string{"$path": test.path},
				})
				_, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
				want := `Project "` + filepath.Join(f.dir, "game/tsconfig.json") + `" must mount "` + filepath.Join(f.dir, test.path) +
					`" at the same Roblox path as "` + filepath.Join(f.dir, "shared/owner.project.json") + `" (` + test.mount +
					`). Referenced projects with their own rbxts.rojo require consistent module and runtime mounts.`
				if err == nil || !strings.Contains(err.Error(), want) {
					t.Fatalf("error = %v, want %s", err, want)
				}
			} else {
				f.build()
			}
		})
	}
}

func TestProjectReferenceValidation(t *testing.T) {
	for _, test := range []struct {
		name    string
		options map[string]any
		message string
	}{
		{"composite", map[string]any{"composite": false}, `must have setting "composite": true`},
		{"noEmit", map[string]any{"noEmit": true}, "may not disable emit"},
		{"declaration", map[string]any{"declaration": false}, "may not disable emit"},
		{"shared build info", map[string]any{"tsBuildInfoFile": "../game/cache/game.tsbuildinfo"}, "unique tsBuildInfoFile"},
		{"build info in another output", map[string]any{"tsBuildInfoFile": "../game/out/shared.tsbuildinfo"}, "another project's output"},
		{"output containing source", map[string]any{"outDir": "../game"}, "contains source directory"},
		{"overlapping outputs", map[string]any{"outDir": "../game/out/shared"}, "overlapping output directories"},
		{"overlapping declarations", map[string]any{"declarationDir": "../game/out/types"}, "overlapping output directories"},
	} {
		t.Run(test.name, func(t *testing.T) {
			f := newReferenceFixture(t)
			f.project("shared", nil, test.options)
			f.project("game", []string{"shared"}, nil)
			f.write("game/out/keep.luau", "return 7\n")
			result, _, err := BuildSolutionWithOptions(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
			if err == nil || !strings.Contains(err.Error(), test.message) {
				t.Fatalf("error = %v, want %s", err, test.message)
			}
			if code := map[string]string{"composite": "TS6306", "noEmit": "TS6310", "declaration": "TS6310"}[test.name]; code != "" {
				if result == nil || len(result.Diagnostics) != 1 || result.Diagnostics[0].Code != code {
					t.Fatalf("result = %+v, want diagnostic %s", result, code)
				}
			}
			if got := string(mustReadFile(t, filepath.Join(f.dir, "game/out/keep.luau"))); got != "return 7\n" {
				t.Fatalf("existing output changed: %q", got)
			}
		})
	}
}

func TestProjectReferenceIncrementalImportPaths(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	f.write("shared/src/left.ts", "export const left = 1;\n")
	f.write("shared/src/right.ts", "export const right = 1;\n")
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	f.rojo("default.project.json", "", map[string]any{"moved": map[string]string{"$path": "shared/next"}})
	f.build()
	for range 5 {
		if result := f.build(); len(result.Outputs) != 0 {
			t.Fatalf("unchanged reference paths caused a rebuild: %v", keys(result.Outputs))
		}
	}
	declaration := string(mustReadFile(t, filepath.Join(f.dir, "shared/types/index.d.ts")))
	configPath := filepath.Join(f.dir, "shared/tsconfig.json")
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(t, configPath), &config); err != nil {
		t.Fatal(err)
	}
	config["compilerOptions"].(map[string]any)["outDir"] = "next"
	f.json("shared/tsconfig.json", config)
	f.build()
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, `TS.import(script, game:GetService("ReplicatedStorage"), "moved")`) {
		t.Fatalf("cached import did not follow the referenced output directory:\n%s", output)
	}
	if got := string(mustReadFile(t, filepath.Join(f.dir, "shared/types/index.d.ts"))); got != declaration {
		t.Fatalf("declaration unexpectedly changed:\n%s", got)
	}
}

func TestProjectReferenceRojoMountsRefresh(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/src"; export const result = value;`)
	configPath := filepath.Join(f.dir, "shared/tsconfig.json")
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(t, configPath), &config); err != nil {
		t.Fatal(err)
	}
	config["rbxts"] = map[string]string{"rojo": "owner.project.json", "type": "game"}
	f.json("shared/tsconfig.json", config)
	f.rojo("shared/owner.project.json", "../", nil)
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		t.Fatal(err)
	}
	if _, messages, err := coordinator.Drain(); err != nil {
		t.Fatalf("first build: %v (%v)", err, messages)
	}
	outputPath := filepath.Join(f.dir, "game/out/init.luau")
	before := string(mustReadFile(t, outputPath))
	f.rojo("shared/owner.project.json", "../", map[string]any{"changed": map[string]string{"$path": "../shared/out/init.luau"}})
	coordinator.Invalidate(configPath)
	if _, messages, err := coordinator.Drain(); err == nil || !strings.Contains(strings.Join(messages, "\n"), "must mount") {
		t.Fatalf("changed reference mount: %v (%v)", err, messages)
	}
	if got := string(mustReadFile(t, outputPath)); got != before {
		t.Fatalf("output changed after inconsistent mounts:\n%s", got)
	}
}

func TestProjectReferenceTransitiveImports(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("common", nil, map[string]any{"declarationDir": "types"})
	f.project("left", []string{"common"}, nil)
	f.project("right", []string{"common"}, nil)
	f.project("game", []string{"left", "right"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../common/types"; export const result = value;`)
	f.build()
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, `TS.import(script, game:GetService("ReplicatedStorage"), "common")`) {
		t.Fatalf("transitive declaration import did not use the common project's mount:\n%s", output)
	}
}

func TestProjectReferenceAddedSource(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/types"; export const result = value;`)
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		t.Fatal(err)
	}
	if _, messages, err := coordinator.Drain(); err != nil {
		t.Fatalf("first build: %v (%v)", err, messages)
	}
	f.write("shared/src/added.ts", "export const added = 8;\n")
	f.write("game/src/index.ts", `import { added } from "../../shared/types/added"; export const result = added;`)
	coordinator.Invalidate(filepath.Join(f.dir, "shared/tsconfig.json"))
	if _, messages, err := coordinator.Drain(); err != nil {
		t.Fatalf("added reference source: %v (%v)", err, messages)
	}
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, `TS.import(script, game:GetService("ReplicatedStorage"), "shared", "added")`) {
		t.Fatalf("added declaration import did not use the referenced project's mount:\n%s", output)
	}
}

func TestProjectReferenceRojoMountsAfterDependencyCopy(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/src"; export const result = value;`)
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(t, filepath.Join(f.dir, "shared/tsconfig.json")), &config); err != nil {
		t.Fatal(err)
	}
	config["rbxts"] = map[string]string{"rojo": "owner.project.json", "type": "game"}
	f.json("shared/tsconfig.json", config)
	f.rojo("shared/owner.project.json", "../", nil)
	f.json("shared/src/default.project.json", map[string]any{"name": "nested", "tree": map[string]any{"current": map[string]string{"$path": "init.luau"}}})
	f.rojo("default.project.json", "", map[string]any{"shared": map[string]string{"$path": "shared/out/init.luau"}})
	_, messages, err := BuildSolutionWithOptions(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err == nil || !strings.Contains(err.Error()+strings.Join(messages, "\n"), "must mount") {
		t.Fatalf("inconsistent copied reference mount: %v (%v)", err, messages)
	}
	if _, err := os.Stat(filepath.Join(f.dir, "game/out/init.luau")); !os.IsNotExist(err) {
		t.Fatalf("consumer emitted with inconsistent mounts: %v", err)
	}
}

func (f *referenceFixture) symlinkPackage(name string) {
	f.t.Helper()
	if err := os.Symlink(filepath.Join(f.dir, name), filepath.Join(f.dir, "node_modules/@rbxts", name)); err != nil {
		f.t.Skipf("symlinks unavailable: %v", err)
	}
}

// Package output stays relocatable: a package reaches a referenced package
// through getModule, and the reference map only supplies the Luau path.
func TestProjectReferencePackageImport(t *testing.T) {
	for _, test := range []struct {
		name     string
		options  map[string]any
		include  string
		manifest map[string]string
		files    map[string]string
	}{
		{
			name:     "luau next to declarations",
			include:  "src/**/*.d.ts",
			manifest: map[string]string{"types": "src/helper.d.ts", "main": "out/helper.luau"},
			files: map[string]string{
				"lib/src/helper.d.ts": "export declare const value: number;\n",
				"lib/src/helper.luau": "return { value = 7 }\n",
			},
		},
		{
			name:     "declaration directory",
			options:  map[string]any{"declarationDir": "types"},
			include:  "src",
			manifest: map[string]string{"types": "types/helper.d.ts"},
			files:    map[string]string{"lib/src/helper.ts": "export const value = 7;\n"},
		},
	} {
		t.Run(test.name, func(t *testing.T) {
			f := newReferenceFixture(t)
			options := map[string]any{"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/lib.tsbuildinfo"}
			for key, value := range test.options {
				options[key] = value
			}
			f.json("lib/tsconfig.json", map[string]any{
				"extends": "../base.json", "compilerOptions": options, "include": []string{test.include},
				"rbxts": map[string]string{"type": "package"},
			})
			test.manifest["name"] = "@rbxts/lib"
			f.json("lib/package.json", test.manifest)
			for path, text := range test.files {
				f.write(path, text)
			}
			f.json("pkg/tsconfig.json", map[string]any{
				"extends": "../base.json", "compilerOptions": map[string]any{
					"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/pkg.tsbuildinfo",
				}, "include": []string{"src"}, "references": []map[string]string{{"path": "../lib"}},
				"rbxts": map[string]string{"type": "package"},
			})
			f.write("pkg/src/index.ts", `import { value } from "@rbxts/lib"; export const result = value;`)
			f.symlinkPackage("lib")
			result, messages, err := BuildSolutionWithOptions(filepath.Join(f.dir, "pkg/tsconfig.json"), ProjectOptions{IncludePath: filepath.Join(f.dir, "include"), Type: "package"})
			if err != nil {
				t.Fatalf("build: %v (%v)", err, messages)
			}
			if len(result.Diagnostics) != 0 {
				t.Fatalf("diagnostics: %+v", result.Diagnostics)
			}
			output := string(mustReadFile(t, filepath.Join(f.dir, "pkg/out/init.luau")))
			if !strings.Contains(output, `TS.getModule(script, "@rbxts", "lib").out.helper`) {
				t.Fatalf("package import did not use getModule with the referenced Luau path:\n%s", output)
			}
		})
	}
}

// A game reaches a referenced game project imported by package name through
// the project's Rojo mount, even when that mount is outside node_modules.
func TestProjectReferenceGamePackageNameImport(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	f.json("shared/package.json", map[string]string{"name": "@rbxts/shared"})
	f.write("shared/src/test/helpers.ts", "export const value = 7;\n")
	f.write("game/src/index.ts", `import { value } from "@rbxts/shared/src/test/helpers"; export const result = value;`)
	f.json("default.project.json", map[string]any{"name": "references", "tree": map[string]any{
		"$className": "DataModel", "ReplicatedStorage": map[string]any{
			"$className": "ReplicatedStorage", "include": map[string]string{"$path": "include"},
			"game": map[string]string{"$path": "game/out"}, "shared-tests": map[string]string{"$path": "shared/out"},
		},
	}})
	f.symlinkPackage("shared")
	result := f.build()
	if len(result.Diagnostics) != 0 {
		t.Fatalf("diagnostics: %+v", result.Diagnostics)
	}
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	if !strings.Contains(output, `TS.import(script, game:GetService("ReplicatedStorage"), "shared-tests", "test", "helpers")`) {
		t.Fatalf("game import did not use the referenced project's mount:\n%s", output)
	}
}

// solutionLibFixture builds a lib whose tsconfig.json is a solution config over
// a package lib and a game spec with its own rbxts.rojo.
func solutionLibFixture(t *testing.T, pluginReference string) *referenceFixture {
	t.Helper()
	f := newReferenceFixture(t)
	f.json("lib/tsconfig.json", map[string]any{
		"compilerOptions": map[string]any{"composite": true}, "files": []string{}, "include": []string{},
		"references": []map[string]string{{"path": "./tsconfig.lib.json"}, {"path": "./tsconfig.spec.json"}},
	})
	f.json("lib/tsconfig.lib.json", map[string]any{
		"extends": "../base.json", "compilerOptions": map[string]any{
			"rootDir": "src", "outDir": "out", "declarationDir": "types", "composite": true,
			"tsBuildInfoFile": "cache/lib.tsbuildinfo",
		}, "include": []string{"src"}, "rbxts": map[string]string{"type": "package"},
	})
	f.json("lib/tsconfig.spec.json", map[string]any{
		"extends": "../base.json", "compilerOptions": map[string]any{
			"rootDir": "spec", "outDir": "out-test", "composite": true,
			"tsBuildInfoFile": "cache/spec.tsbuildinfo",
		}, "include": []string{"spec"}, "rbxts": map[string]string{"type": "game", "rojo": "./test.project.json"},
	})
	f.json("lib/package.json", map[string]string{"name": "@rbxts/lib", "types": "types/helper.d.ts"})
	f.write("lib/src/helper.ts", "export const value = 7;\n")
	f.write("lib/spec/helper.test.ts", "export const checked = true;\n")
	f.json("lib/test.project.json", map[string]any{"name": "lib-tests", "tree": map[string]any{
		"$className": "DataModel", "ReplicatedStorage": map[string]any{
			"$className": "ReplicatedStorage", "include": map[string]string{"$path": "../include"},
			"lib-tests": map[string]string{"$path": "out-test"},
		},
	}})
	f.json("plugin/tsconfig.json", map[string]any{
		"extends": "../base.json", "compilerOptions": map[string]any{
			"rootDir": "src", "outDir": "out", "composite": true, "tsBuildInfoFile": "cache/plugin.tsbuildinfo",
		}, "include": []string{"src"}, "references": []map[string]string{{"path": pluginReference}},
		"rbxts": map[string]string{"type": "package"},
	})
	f.write("plugin/src/index.ts", `import { value } from "@rbxts/lib"; export const result = value;`)
	f.symlinkPackage("lib")
	return f
}

// Upstream gives a solution config no path translator and checks Rojo mounts
// only for direct references, so a game spec reached through a solution
// reference is not checked.
func TestProjectReferenceSolutionRojoMounts(t *testing.T) {
	f := solutionLibFixture(t, "../lib")
	result, messages, err := BuildSolutionWithOptions(filepath.Join(f.dir, "plugin/tsconfig.json"), ProjectOptions{IncludePath: filepath.Join(f.dir, "include"), Type: "package"})
	if err != nil {
		t.Fatalf("build: %v (%v)", err, messages)
	}
	if len(result.Diagnostics) != 0 {
		t.Fatalf("diagnostics: %+v", result.Diagnostics)
	}
	output := string(mustReadFile(t, filepath.Join(f.dir, "plugin/out/init.luau")))
	if !strings.Contains(output, `TS.getModule(script, "@rbxts", "lib").out.helper`) {
		t.Fatalf("import through the solution reference did not resolve:\n%s", output)
	}
}

func TestProjectReferenceDirectSpecRojoMounts(t *testing.T) {
	f := solutionLibFixture(t, "../lib/tsconfig.spec.json")
	_, err := NewSolutionCoordinator(filepath.Join(f.dir, "plugin/tsconfig.json"), ProjectOptions{IncludePath: filepath.Join(f.dir, "include"), Type: "package"})
	want := `Project "` + filepath.Join(f.dir, "plugin/tsconfig.json") + `" must mount "` + filepath.Join(f.dir, "lib/out-test/helper.test.luau") +
		`" at the same Roblox path as "` + filepath.Join(f.dir, "lib/test.project.json") +
		`" (ReplicatedStorage.lib-tests.helper.test). Referenced projects with their own rbxts.rojo require consistent module and runtime mounts.`
	if err == nil || !strings.Contains(err.Error(), want) {
		t.Fatalf("error = %v, want %s", err, want)
	}
}

// Hand-written declarations and JSON outside rootDir describe files mapped
// directly by Rojo; index declarations reach the init module beside them.
func TestProjectImportOutsideRootDir(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("game", nil, map[string]any{"resolveJsonModule": true})
	f.write("lib/index.d.ts", "export declare const value: number;\n")
	f.write("lib/init.luau", "return { value = 1 }\n")
	f.write("lib/pure.d.ts", "export declare const pure: number;\n")
	f.write("lib/pure.luau", "return { pure = 2 }\n")
	f.json("lib/data.json", map[string]int{"count": 3})
	f.rojo("default.project.json", "", map[string]any{"lib": map[string]string{"$path": "lib"}})
	f.write("game/src/index.ts", `import { value } from "../../lib";
import { pure } from "../../lib/pure";
import data from "../../lib/data.json";
export const result = value + pure + data.count;`)
	result := f.build()
	if len(result.Diagnostics) != 0 {
		t.Fatalf("diagnostics: %+v", result.Diagnostics)
	}
	output := string(mustReadFile(t, filepath.Join(f.dir, "game/out/init.luau")))
	for _, want := range []string{
		`TS.import(script, game:GetService("ReplicatedStorage"), "lib")`,
		`TS.import(script, game:GetService("ReplicatedStorage"), "lib", "pure")`,
		`TS.import(script, game:GetService("ReplicatedStorage"), "lib", "data")`,
	} {
		if !strings.Contains(output, want) {
			t.Fatalf("missing %s:\n%s", want, output)
		}
	}
}

// TypeScript reports a source file outside rootDir, but a census still
// transforms the importer; the import must not fail the transform.
func TestProjectImportSourceOutsideRootDirCensus(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("game", nil, nil)
	f.write("lib/helper.ts", "export const value = 1;\n")
	f.write("game/src/index.ts", `import { value } from "../../lib/helper"; export const result = value;`)
	solution, err := CompileSolutionDiagnostics(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		t.Fatal(err)
	}
	for _, project := range solution.Projects {
		for _, file := range project.Files {
			if file.InternalError != nil {
				t.Fatalf("%s: %v", file.FileName, file.InternalError)
			}
		}
	}
}

// A file named "..x" sits inside rootDir, so its import uses the outDir
// path. Rojo itself treats such a name as outside its mount.
func TestProjectImportDotDotNameInsideRootDir(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("game", nil, nil)
	f.write("game/src/..types.d.ts", "export declare const value: number;\n")
	f.write("game/src/index.ts", `import { value } from "./..types"; export const result = value;`)
	_, messages, _ := BuildSolutionWithOptions(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	want := filepath.Join("out", "..types.luau")
	if !strings.Contains(strings.Join(messages, "\n"), want) {
		t.Fatalf("messages = %v, want path %s", messages, want)
	}
}

// A direct build maps a reference with its own luau option, like a solution
// build, so both agree on the incremental salt.
func TestProjectReferencePathsUseReferenceLuau(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"declarationDir": "types"})
	f.project("game", []string{"shared"}, nil)
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(t, filepath.Join(f.dir, "shared/tsconfig.json")), &config); err != nil {
		t.Fatal(err)
	}
	config["rbxts"] = map[string]any{"luau": false}
	f.json("shared/tsconfig.json", config)
	configPath := filepath.Join(f.dir, "game/tsconfig.json")
	graph, err := BuildSolutionGraph(configPath, f.options())
	if err != nil {
		t.Fatal(err)
	}
	populateCrossProjectMetadata(graph)
	var solution map[string]string
	for _, project := range graph.Projects {
		if project.ConfigPath == configPath {
			solution = project.Options.projectReferencePaths
		}
	}
	_, program, _, err := newProjectProgram(filepath.Dir(configPath), configPath)
	if err != nil {
		t.Fatal(err)
	}
	direct := createCrossProjectImportPathMap(program, f.options())
	if len(solution) == 0 || !maps.Equal(direct, solution) {
		t.Fatalf("direct = %v, solution = %v", direct, solution)
	}
}

// A census reports every project even when solution validation fails.
func TestProjectReferenceValidationCensus(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, map[string]any{"composite": false})
	f.project("game", []string{"shared"}, nil)
	solution, err := CompileSolutionDiagnostics(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err == nil || !strings.Contains(err.Error(), `must have setting "composite": true`) {
		t.Fatalf("error = %v", err)
	}
	if len(solution.Projects) != 2 {
		t.Fatalf("projects = %d, want 2", len(solution.Projects))
	}
}

// A project builds when only an unrelated project's reference mounts break.
func TestProjectReferenceRojoMountsUnrelatedProject(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	f.project("other", nil, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/src"; export const result = value;`)
	var config map[string]any
	if err := json.Unmarshal(mustReadFile(t, filepath.Join(f.dir, "shared/tsconfig.json")), &config); err != nil {
		t.Fatal(err)
	}
	config["rbxts"] = map[string]string{"rojo": "owner.project.json", "type": "game"}
	f.json("shared/tsconfig.json", config)
	f.rojo("shared/owner.project.json", "../", nil)
	f.json("shared/src/default.project.json", map[string]any{"name": "nested", "tree": map[string]any{"current": map[string]string{"$path": "init.luau"}}})
	f.rojo("default.project.json", "", map[string]any{"shared": map[string]string{"$path": "shared/out/init.luau"}})
	options := f.options()
	singleThreaded := true
	options.SingleThreaded = &singleThreaded
	coordinator, err := NewSolutionCoordinatorForRoots([]string{filepath.Join(f.dir, "game/tsconfig.json"), filepath.Join(f.dir, "other/tsconfig.json")}, options)
	if err != nil {
		t.Fatal(err)
	}
	coordinator.Drain()
	if _, err := os.Stat(filepath.Join(f.dir, "other/out/init.luau")); err != nil {
		t.Fatalf("unrelated project did not build: %v", err)
	}
	if _, err := os.Stat(filepath.Join(f.dir, "game/out/init.luau")); !os.IsNotExist(err) {
		t.Fatalf("consumer emitted with inconsistent mounts: %v", err)
	}
}
