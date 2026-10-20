package compile

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"

	"rotor/internal/transformer"
)

func writePackageLayoutFixture(t *testing.T) string {
	t.Helper()
	dir := writeProject(t, "@scope/app", "")
	writeProjectFile(t, dir, "src/main.ts", `export { answer } from "@rbxts/lib";`)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/package.json", `{"name":"@rbxts/lib","main":"out/init.luau","types":"out/index.d.ts"}`)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/out/index.d.ts", `export declare const answer: number;`)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/out/init.luau", `return { answer = 42 }`)
	return dir
}

func TestPackageIncrementalLayoutChanges(t *testing.T) {
	dir := writePackageLayoutFixture(t)
	enableIncrementalBuilds(t, dir)
	build := func(wantSelected bool) {
		t.Helper()
		timings := NewBuildTimings()
		if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil {
			t.Fatalf("build: %v (%v)", err, diags)
		}
		if got := timings.Counts.SelectedSources > 0; got != wantSelected {
			t.Fatalf("selected %d sources, want changed=%v", timings.Counts.SelectedSources, wantSelected)
		}
		clean, diags, err := CompileProject(dir)
		if err != nil {
			t.Fatalf("clean compile: %v (%v)", err, diags)
		}
		if got := string(mustReadFile(t, filepath.Join(dir, "out/main.luau"))); got != clean["out/main.luau"] {
			t.Fatalf("incremental output differs from clean:\n%s\nclean:\n%s", got, clean["out/main.luau"])
		}
	}
	build(true)
	build(false)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/.cache/place/clean.project.json", `{"name":"test","tree":{"$path":"../../out"}}`)
	build(false)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/.cache/place/clean.project.json", `{"name":"renamed-test","tree":{"$path":"../../out"}}`)
	build(false)
	if err := os.Remove(filepath.Join(dir, "node_modules/@rbxts/lib/.cache/place/clean.project.json")); err != nil {
		t.Fatal(err)
	}
	build(false)
	config := filepath.Join(dir, "node_modules/@rbxts/lib/default.project.json")
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/default.project.json", `{"name":"lib","tree":{"$path":"out"}}`)
	build(true)
	info, err := os.Stat(config)
	if err != nil {
		t.Fatal(err)
	}
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/default.project.json", `{"name":"renamed","tree":{"$path":"out"}}`)
	if err := os.Chtimes(config, info.ModTime(), info.ModTime()); err != nil {
		t.Fatal(err)
	}
	build(true)
	if err := os.Remove(config); err != nil {
		t.Fatal(err)
	}
	build(true)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/runtime/init.luau", `return { answer = 42 }`)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/package.json", `{"name":"@rbxts/lib","main":"runtime/init.luau","types":"out/index.d.ts"}`)
	build(true)
	build(false)
}

func TestPackageImportsIgnoreDiscoveredNamedProjects(t *testing.T) {
	for _, folder := range []string{".cache/place", "tests/place", "node_modules/nested", "."} {
		t.Run(folder, func(t *testing.T) {
			dir := writePackageLayoutFixture(t)
			mount := "../../out"
			if folder == "." {
				mount = "out"
			}
			writeProjectFile(t, dir, filepath.Join("node_modules/@rbxts/lib", folder, "clean.project.json"),
				`{"name":"lib-testing","tree":{"$className":"DataModel","ReplicatedStorage":{"lib":{"out":{"$path":"`+mount+`"}}}}}`)
			outputs, diags, err := CompileProject(dir)
			if err != nil {
				t.Fatalf("compile: %v (%v)", err, diags)
			}
			want := `TS.import(script, TS.getModule(script, "@rbxts", "lib").out).answer`
			if got := outputs["out/main.luau"]; !strings.Contains(got, want) {
				t.Fatalf("package import should retain its output path:\n%s", got)
			}
		})
	}
}

func TestPackageImportsIgnoreNamedRootRemap(t *testing.T) {
	dir := writePackageLayoutFixture(t)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/testing.project.json", `{"name":"testing","tree":{"$path":"testing-output"}}`)
	outputs, diags, err := CompileProject(dir)
	if err != nil {
		t.Fatalf("compile: %v (%v)", err, diags)
	}
	if got := outputs["out/main.luau"]; !strings.Contains(got, `TS.getModule(script, "@rbxts", "lib").out`) {
		t.Fatalf("named root project remapped the runtime entry:\n%s", got)
	}
}

func TestPackageImportsPnpmProjects(t *testing.T) {
	dir := writePnpmFixture(t)
	writeProjectFile(t, dir, "node_modules/@rbxts/dummy/.cache/place/clean.project.json", `{"name":"testing","tree":{"$path":"../../out"}}`)
	outputs, diags, err := CompileProjectWithOptions(dir, ProjectOptions{Type: transformer.ProjectTypePackage})
	if err != nil {
		t.Fatalf("compile: %v (%v)", err, diags)
	}
	got := outputs["out/main.luau"]
	for _, want := range []string{`TS.getModule(script, "@rbxts", "dummy").out`, `TS.getModule(script, "@rbxts", "linked")`} {
		if !strings.Contains(got, want) {
			t.Fatalf("missing %s in pnpm package output:\n%s", want, got)
		}
	}
}

func TestPackageWatchRetainsMissingExplicitProject(t *testing.T) {
	dir := writePackageLayoutFixture(t)
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/default.project.json", `{"name":"lib","tree":{"Mounted":{"$path":"layout.project.json"}}}`)
	named := filepath.Join(dir, "node_modules/@rbxts/lib/layout.project.json")
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/layout.project.json", `{"name":"ignored","tree":{"$path":"out"}}`)
	config := filepath.Join(dir, "tsconfig.json")
	coordinator, err := NewSolutionCoordinator(config, ProjectOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if _, diags, err := coordinator.Drain(); err != nil {
		t.Fatalf("initial drain: %v (%v)", err, diags)
	}
	sets := coordinator.WatchSets()
	config = sets[0].ProjectPath
	if err := os.Remove(named); err != nil {
		t.Fatal(err)
	}
	if !coordinator.RefreshPackageWatchInputs(config) {
		t.Fatal("removing explicit composition did not change the layout")
	}
	inputs := coordinator.WatchSets()[0].PackageInputs
	if inputs == nil || !slices.Contains(canonicalWatchPaths(inputs.Files), canonicalWatchDirectory(named)) {
		t.Fatalf("missing explicit project lost its watch: %+v", inputs)
	}
	writeProjectFile(t, dir, "node_modules/@rbxts/lib/layout.project.json", `{"name":"ignored","tree":{"$path":"out"}}`)
	if !coordinator.RefreshPackageWatchInputs(config) {
		t.Fatal("recreating explicit composition did not change the layout")
	}
}

func TestPackageWatchExplicitJunctionRetarget(t *testing.T) {
	for _, suffix := range []string{"", "/shared.project.json", "/sub/deeper/shared.project.json"} {
		t.Run(suffix, func(t *testing.T) {
			dir := writePackageLayoutFixture(t)
			writeProjectFile(t, dir, "node_modules/@rbxts/lib/default.project.json", `{"name":"lib","tree":{"Mounted":{"$path":"../../../links/runtime`+suffix+`"}}}`)
			for _, name := range []string{"One", "Two"} {
				body := `{"name":"ignored","tree":{"` + name + `":{"$path":"../../node_modules/@rbxts/lib/out"}}}`
				writeProjectFile(t, dir, "targets/"+name+"/default.project.json", body)
				writeProjectFile(t, dir, "targets/"+name+"/shared.project.json", body)
				writeProjectFile(t, dir, "targets/"+name+"/sub/deeper/shared.project.json", strings.Replace(body, "../../node_modules", "../../../../node_modules", 1))
			}
			alias := filepath.Join(dir, "links/runtime")
			if err := os.MkdirAll(filepath.Dir(alias), 0o755); err != nil {
				t.Fatal(err)
			}
			link := func(name string) {
				t.Helper()
				target := filepath.Join(dir, "targets", name)
				if runtime.GOOS == "windows" {
					if out, err := exec.Command("cmd", "/c", "mklink", "/J", alias, target).CombinedOutput(); err != nil {
						t.Fatalf("mklink: %v: %s", err, out)
					}
				} else if err := os.Symlink(target, alias); err != nil {
					t.Fatal(err)
				}
			}
			link("One")
			coordinator, err := NewSolutionCoordinator(filepath.Join(dir, "tsconfig.json"), ProjectOptions{})
			if err != nil {
				t.Fatal(err)
			}
			if _, diags, err := coordinator.Drain(); err != nil {
				t.Fatalf("initial drain: %v (%v)", err, diags)
			}
			set := coordinator.WatchSets()[0]
			if set.PackageInputs == nil || !slices.Contains(canonicalWatchPaths(set.PackageInputs.Directories), canonicalWatchDirectory(filepath.Dir(alias))) {
				t.Fatalf("junction parent is not watched: %+v", set.PackageInputs)
			}
			if err := os.Remove(alias); err != nil {
				t.Fatal(err)
			}
			link("Two")
			if !coordinator.RefreshPackageWatchInputs(set.ProjectPath) {
				t.Fatal("junction retarget did not change the layout")
			}
			coordinator.Invalidate(set.ProjectPath)
			if _, diags, err := coordinator.Drain(); err != nil {
				t.Fatalf("retargeted drain: %v (%v)", err, diags)
			}
			if got := string(mustReadFile(t, filepath.Join(dir, "out/main.luau"))); !strings.Contains(got, ".Mounted.Two") {
				t.Fatalf("retargeted layout is stale:\n%s", got)
			}
		})
	}
}
