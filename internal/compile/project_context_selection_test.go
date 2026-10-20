package compile

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"rotor/internal/transformer"
)

const projectContextSelectionModelRojo = `{"name":"kind","tree":{"$path":"out","include":{"$path":"include"}}}`
const projectContextSelectionGameRojo = `{"name":"kind","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}}`

func writeProjectContextSelectionFile(tb testing.TB, dir, name, contents string) {
	tb.Helper()
	path := filepath.Join(dir, filepath.FromSlash(name))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		tb.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		tb.Fatal(err)
	}
}

func TestProjectContextSelectionOmitsOnlyTransformImports(t *testing.T) {
	for _, kind := range []transformer.ProjectType{transformer.ProjectTypePackage, transformer.ProjectTypeModel, transformer.ProjectTypeGame} {
		t.Run(string(kind), func(t *testing.T) {
			dir := writePnpmFixture(t)
			writeProjectContextSelectionFile(t, dir, "default.project.json", projectContextSelectionGameRojo)
			absDir, program, diags, err := newProjectProgram(dir, "")
			if err != nil {
				t.Fatalf("program: %v (%v)", err, diags)
			}
			opts := ProjectOptions{Type: kind, projectReferencePaths: map[string]string{"reference.d.ts": "reference.luau"}}
			full, diags, err := newProjectContext(absDir, program, opts)
			if err != nil || len(diags) != 0 {
				t.Fatalf("full context: %v (%v)", err, diags)
			}
			empty, diags, err := newProjectContextWithImportContext(absDir, program, opts, false)
			if err != nil || len(diags) != 0 {
				t.Fatalf("empty-selection context: %v (%v)", err, diags)
			}
			if len(full.rojoContext.PkgRojoResolvers) != len(program.Options().TypeRoots) {
				t.Fatal("full wrapper omitted type-root resolvers")
			}
			if len(full.rojoContext.NodeModulesPathMapping.Exact) == 0 {
				t.Fatal("full wrapper omitted node_modules entry mappings")
			}
			if len(empty.rojoContext.PkgRojoResolvers) != 0 || empty.rojoContext.NodeModulesPathMapping.Exact != nil || len(empty.rojoContext.NodeModulesPathMapping.Patterns) != 0 {
				t.Fatal("empty selection constructed transform import context")
			}
			if empty.dir != full.dir || empty.projectType != full.projectType || empty.env == nil || empty.assets == nil || empty.files == nil || empty.stamps == nil {
				t.Fatal("empty selection lost project identity or macro services")
			}
			got, want := empty.rojoContext, full.rojoContext
			if got.Resolver == nil || !reflect.DeepEqual(got.PathTranslator, want.PathTranslator) ||
				!reflect.DeepEqual(got.RuntimeLibRbxPath, want.RuntimeLibRbxPath) || got.ProjectPath != want.ProjectPath ||
				got.NodeModulesPath != want.NodeModulesPath || !slices.Equal(got.TypeRoots, want.TypeRoots) ||
				got.UseCaseSensitiveFileNames != want.UseCaseSensitiveFileNames || !reflect.DeepEqual(got.ProjectReferencePaths, want.ProjectReferencePaths) {
				t.Fatal("empty selection changed retained resolver/path/reference context")
			}
		})
	}
}

// An empty pass must not leave import context missing on the next selected
// build. Both synthetic package resolvers and types-to-runtime mappings are
// observable through these pnpm/nested-Rojo imports.
func TestProjectContextSelectionNoChangeThenEditMatchesFresh(t *testing.T) {
	const imports = "import { dummy } from \"@rbxts/dummy\";\nimport { linked } from \"@rbxts/linked\";\n"
	const edited = imports + "export const result = dummy() + linked() + 1;\nprint(result);\n"
	prepare := func(source string) string {
		t.Helper()
		dir := writePnpmFixture(t)
		enableIncrementalBuilds(t, dir)
		setProjectContextSelectionOptions(t, dir, map[string]any{"declaration": true, "declarationMap": true, "sourceMap": true})
		writeProjectContextSelectionFile(t, dir, "src/main.ts", source)
		writeProjectContextSelectionFile(t, dir, "src/data.json", "{\"copied\":true}\n")
		return dir
	}
	build := func(dir string) (*BuildResult, *BuildTimings) {
		t.Helper()
		timings := NewBuildTimings()
		result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Type: transformer.ProjectTypePackage, Timings: timings})
		if err != nil || len(diags) != 0 {
			t.Fatalf("build: %v (%v)", err, diags)
		}
		return result, timings
	}
	dir := prepare(imports + "export const result = dummy() + linked();\nprint(result);\n")
	seed, _ := build(dir)
	before := readProjectContextSelectionOutputs(t, dir, seed.OwnedOutputs)
	old := time.Unix(100, 0)
	for _, name := range seed.OwnedOutputs {
		path := filepath.Join(dir, filepath.FromSlash(name))
		if err := os.Chtimes(path, old, old); err != nil {
			t.Fatal(err)
		}
	}
	nc, counts := build(dir)
	if counts.Counts.SelectedSources != 0 || counts.Counts.ActualWrites != 0 || len(nc.Outputs) != 0 || len(nc.EmittedFiles) != 0 {
		t.Fatalf("no-change pass selected/emitted/wrote: %+v, outputs %v, emitted %v", counts.Counts, nc.Outputs, nc.EmittedFiles)
	}
	if !slices.Equal(nc.OwnedOutputs, seed.OwnedOutputs) || !reflect.DeepEqual(readProjectContextSelectionOutputs(t, dir, nc.OwnedOutputs), before) {
		t.Fatal("no-change pass lost owned outputs or changed their bytes")
	}
	for _, name := range nc.OwnedOutputs {
		info, err := os.Stat(filepath.Join(dir, filepath.FromSlash(name)))
		if err != nil {
			t.Fatal(err)
		}
		if !info.ModTime().Equal(old) {
			t.Fatalf("no-change pass rewrote %s", name)
		}
	}

	writeProjectContextSelectionFile(t, dir, "src/main.ts", edited)
	changed, counts := build(dir)
	if counts.Counts.SelectedSources != 1 {
		t.Fatalf("edited pass selected %d sources, want 1", counts.Counts.SelectedSources)
	}
	freshDir := prepare(edited)
	fresh, _ := build(freshDir)
	if !slices.Equal(changed.OwnedOutputs, fresh.OwnedOutputs) ||
		!reflect.DeepEqual(readProjectContextSelectionOutputs(t, dir, changed.OwnedOutputs), readProjectContextSelectionOutputs(t, freshDir, fresh.OwnedOutputs)) {
		t.Fatal("build after empty selection differs from fresh Luau/declaration/map/copied output")
	}
	for _, want := range []string{`TS.getModule(script, "@rbxts", "dummy").out`, `TS.getModule(script, "@rbxts", "linked")`} {
		if !strings.Contains(changed.Outputs["out/main.luau"], want) {
			t.Fatalf("edited output lost import %s:\n%s", want, changed.Outputs["out/main.luau"])
		}
	}
}

func TestProjectContextSelectionMainWarningsAndInvalidMount(t *testing.T) {
	for _, kind := range []transformer.ProjectType{transformer.ProjectTypePackage, transformer.ProjectTypeModel, transformer.ProjectTypeGame} {
		for _, emptySource := range []bool{false, true} {
			name := string(kind) + "/no-change"
			if emptySource {
				name = string(kind) + "/empty-source"
			}
			t.Run(name, func(t *testing.T) {
				dir := writeProject(t, "context-selection-fixture", projectContextSelectionModelRojo)
				if emptySource {
					if err := os.Remove(filepath.Join(dir, "src", "main.ts")); err != nil {
						t.Fatal(err)
					}
				}
				enableIncrementalBuilds(t, dir)
				writeProjectContextSelectionFile(t, dir, "node_modules/@rbxts/broken/default.project.json", "{")
				const mainRojo = `{"name":"kind","tree":{"$path":"out","include":{"$path":"include"},"Broken":{"$path":"node_modules/@rbxts/broken"},"Missing":{"$path":"missing.project.json"}}}`
				writeProjectContextSelectionFile(t, dir, "default.project.json", mainRojo)
				log := captureCompilerLog(t)
				build := func() (*BuildTimings, []string, error) {
					t.Helper()
					log.Reset()
					timings := NewBuildTimings()
					_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Type: kind, Timings: timings})
					lines := strings.Split(strings.TrimSuffix(log.String(), "\n"), "\n")
					if len(lines) != 2 || !strings.HasPrefix(lines[0], "Compiler Warning: RojoResolver: Invalid configuration!") ||
						lines[1] != `Compiler Warning: RojoResolver: Path does not exist "`+filepath.Join(dir, "missing.project.json")+`"` {
						t.Fatalf("main warnings/order = %q", log.String())
					}
					return timings, diags, err
				}
				if _, diags, err := build(); err != nil || len(diags) != 0 {
					t.Fatalf("seed: %v (%v)", err, diags)
				}
				counts, diags, err := build()
				if err != nil || len(diags) != 0 || counts.Counts.SelectedSources != 0 || counts.Counts.ActualWrites != 0 {
					t.Fatalf("empty selection: %v (%v), counts %+v", err, diags, counts.Counts)
				}
				// A source mount is invalid even in explicit package mode. The
				// declaration-only input case keeps selection empty after this edit.
				writeProjectContextSelectionFile(t, dir, "default.project.json", strings.Replace(mainRojo, `"$path":"out"`, `"$path":"src"`, 1))
				counts, diags, err = build()
				if err == nil || !strings.Contains(strings.Join(diags, "\n"), "Rojo") {
					t.Fatalf("invalid main mount accepted: %v (%v)", err, diags)
				}
				if emptySource && counts.Counts.SelectedSources != 0 {
					t.Fatalf("empty-source validation selected %d files", counts.Counts.SelectedSources)
				}
				writeProjectContextSelectionFile(t, dir, "default.project.json", mainRojo)
				counts, diags, err = build()
				if err != nil || len(diags) != 0 || counts.Counts.SelectedSources != 0 || counts.Counts.ActualWrites != 0 {
					t.Fatalf("recovery: %v (%v), counts %+v", err, diags, counts.Counts)
				}
			})
		}
	}
}

func TestProjectContextSelectionEmptyDiagnosticsAndCancellation(t *testing.T) {
	for _, gate := range []string{"program", "global", "cancellation"} {
		t.Run(gate, func(t *testing.T) {
			dir := writeProject(t, "@scope/context-selection-diagnostics", "")
			if err := os.Remove(filepath.Join(dir, "src", "main.ts")); err != nil {
				t.Fatal(err)
			}
			opts := ProjectOptions{Timings: NewBuildTimings()}
			want := "declarationMap"
			switch gate {
			case "program":
				setProjectContextSelectionOptions(t, dir, map[string]any{"declarationMap": true})
			case "global":
				writeProjectContextSelectionFile(t, dir, "src/globals.d.ts", "// No fundamental global types.\n")
				want = "Cannot find global type"
			case "cancellation":
				ctx, cancel := context.WithCancel(context.Background())
				cancel()
				opts.Context = ctx
			}
			result, diags, err := BuildProjectWithOptions(dir, opts)
			if gate == "cancellation" {
				if !errors.Is(err, context.Canceled) {
					t.Fatalf("empty selection error = %v, want cancellation", err)
				}
			} else if err == nil || result == nil || len(result.Diagnostics) == 0 || !strings.Contains(strings.Join(diags, "\n"), want) {
				t.Fatalf("empty selection lost %s diagnostics: %v (%v), result %+v", gate, err, diags, result)
			}
			if opts.Timings.Counts.SelectedSources != 0 || opts.Timings.Counts.ActualWrites != 0 {
				t.Fatalf("empty selection selected/wrote output: %+v", opts.Timings.Counts)
			}
		})
	}
}

func TestProjectContextSelectionEmptyRuntimeMountValidation(t *testing.T) {
	for _, kind := range []transformer.ProjectType{transformer.ProjectTypeModel, transformer.ProjectTypeGame} {
		for _, tc := range []struct {
			name string
			rojo string
			want string
		}{
			{"missing-rojo", "", "Non-package projects must have a Rojo project file!"},
			{"missing-include", `{"name":"x","tree":{"$path":"out"}}`, "Rojo project contained no data for include folder!"},
			{"network", `{"name":"x","tree":{"$className":"DataModel","ServerScriptService":{"out":{"$path":"out"},"include":{"$path":"include"}}}}`, "Runtime library cannot be in a server-only or client-only container!"},
			{"isolated", `{"name":"x","tree":{"$className":"DataModel","PluginDebugService":{"include":{"$path":"include"}},"ReplicatedStorage":{"out":{"$path":"out"}}}}`, "Runtime library cannot be in an isolated container!"},
		} {
			t.Run(string(kind)+"/"+tc.name, func(t *testing.T) {
				dir := writeProject(t, "@scope/override-package", tc.rojo)
				if err := os.Remove(filepath.Join(dir, "src", "main.ts")); err != nil {
					t.Fatal(err)
				}
				timings := NewBuildTimings()
				_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Type: kind, Timings: timings})
				if err == nil || !slices.Equal(diags, []string{tc.want}) || timings.Counts.SelectedSources != 0 {
					t.Fatalf("empty selection lost mount validation: %v (%v), counts %+v", err, diags, timings.Counts)
				}
			})
		}
	}
}

func TestProjectContextSelectionNoChangeCancellation(t *testing.T) {
	dir := writeProject(t, "@scope/context-selection-cancel", "")
	enableIncrementalBuilds(t, dir)
	seed, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil || len(diags) != 0 {
		t.Fatalf("seed: %v (%v)", err, diags)
	}
	want := readProjectContextSelectionOutputs(t, dir, seed.OwnedOutputs)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	timings := NewBuildTimings()
	_, diags, err = BuildProjectWithOptions(dir, ProjectOptions{Context: ctx, Timings: timings})
	if !errors.Is(err, context.Canceled) || timings.Counts.SelectedSources != 0 || timings.Counts.ActualWrites != 0 {
		t.Fatalf("no-change cancellation: %v (%v), counts %+v", err, diags, timings.Counts)
	}
	if !reflect.DeepEqual(readProjectContextSelectionOutputs(t, dir, seed.OwnedOutputs), want) {
		t.Fatal("cancelled no-change pass changed output bytes")
	}
}

func TestProjectContextSelectionMissingOutputRestoresImportContext(t *testing.T) {
	for _, missing := range []string{"out/main.luau", "out/main.luau.map", "out/main.d.ts", "out/main.d.ts.map"} {
		t.Run(missing, func(t *testing.T) {
			dir := writePnpmFixture(t)
			enableIncrementalBuilds(t, dir)
			setProjectContextSelectionOptions(t, dir, map[string]any{"declaration": true, "declarationMap": true, "sourceMap": true})
			opts := ProjectOptions{Type: transformer.ProjectTypePackage}
			seed, diags, err := BuildProjectWithOptions(dir, opts)
			if err != nil || len(diags) != 0 {
				t.Fatalf("seed: %v (%v)", err, diags)
			}
			want := readProjectContextSelectionOutputs(t, dir, seed.OwnedOutputs)
			opts.Timings = NewBuildTimings()
			if _, diags, err := BuildProjectWithOptions(dir, opts); err != nil || len(diags) != 0 || opts.Timings.Counts.SelectedSources != 0 {
				t.Fatalf("no-change: %v (%v), counts %+v", err, diags, opts.Timings.Counts)
			}
			if err := os.Remove(filepath.Join(dir, filepath.FromSlash(missing))); err != nil {
				t.Fatal(err)
			}
			opts.Timings = NewBuildTimings()
			recovered, diags, err := BuildProjectWithOptions(dir, opts)
			if err != nil || len(diags) != 0 {
				t.Fatalf("recovery: %v (%v)", err, diags)
			}
			if opts.Timings.Counts.SelectedSources != 1 || opts.Timings.Counts.ActualWrites != 1 {
				t.Fatalf("recovery did not select/recreate missing output: %+v, emitted %v", opts.Timings.Counts, recovered.EmittedFiles)
			}
			if !slices.Equal(recovered.OwnedOutputs, seed.OwnedOutputs) || !reflect.DeepEqual(readProjectContextSelectionOutputs(t, dir, recovered.OwnedOutputs), want) {
				t.Fatal("missing-output recovery changed output ownership or bytes")
			}
		})
	}
}

func setProjectContextSelectionOptions(t *testing.T, dir string, overrides map[string]any) {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal(data, &config); err != nil {
		t.Fatal(err)
	}
	options := config["compilerOptions"].(map[string]any)
	for key, value := range overrides {
		options[key] = value
	}
	data, err = json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	writeProjectContextSelectionFile(t, dir, "tsconfig.json", string(data))
}

func readProjectContextSelectionOutputs(t *testing.T, dir string, paths []string) map[string]string {
	t.Helper()
	outputs := make(map[string]string, len(paths))
	for _, name := range paths {
		data, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(name)))
		if err != nil {
			t.Fatal(err)
		}
		outputs[name] = string(data)
	}
	return outputs
}
