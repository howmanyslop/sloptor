package compile

import (
	"context"
	"errors"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/vfstest"
)

// Most tests below run one update on two fresh Programs loaded from the same
// project: the serial control is single-threaded, so it takes the original
// update path, and the candidate is multi-threaded, so its overlays preparse.

const preparseDeprecatedSource = "/**\n * Old helper.\n * @deprecated Use fresh.\n */\nexport function old(): number {\n\treturn 1;\n}\n\nexport function fresh(): number {\n\treturn 2;\n}\n"

const preparseDeprecatedUse = "import { old } from \"./dep\";\n\n/** Uses the old helper. */\nexport const a: number = \"x\" as unknown as string;\nexport const legacy = old();\n"

const preparseNewImport = "import { extra } from \"../lib/extra\";\nexport const a = extra;\n"

// preparseRebuildFiles lets a changed source rebuild the Program: the lib files
// are outside the include set, and only z.ts imports late.ts.
var preparseRebuildFiles = map[string]string{
	"lib/extra.ts": "export const extra = 1;\n",
	"lib/late.ts":  "export const late = 1;\n",
	"lib/ref.d.ts": "export declare const referenced: number;\n",
	"src/a.ts":     "export const a = 1;\n",
	"src/m.ts":     "export const m = 1;\n",
	"src/z.ts":     "import { late } from \"../lib/late\";\nexport const z = late;\n",
}

func TestPreparseNativeUpdateMatchesSerialControl(t *testing.T) {
	// Given: reverse-ordered native overlays that change text, imports, JSDoc use
	// and diagnostics, next to untouched sources and shared or nil traces.
	dir := writePreparseProject(t, map[string]string{
		"src/a.ts":         "export const a = 1;\n",
		"src/b.ts":         "export const b = 1;\n",
		"src/c.tsx":        "export const c = 1;\n",
		"src/dep.ts":       preparseDeprecatedSource,
		"src/untouched.ts": "export const untouched = 1;\n",
	})
	control, candidate := loadPreparsePrograms(t, dir)
	shared := &sourceTraceMap{fileName: "shared.ts", text: "shared", mappings: []traceMapping{{generatedLine: 0, sourceLine: 2}}}
	authored := &sourceTraceMap{fileName: "a.ts", text: "authored", mappings: []traceMapping{{generatedLine: 0, sourceLine: 7}, {generatedLine: 3, sourceLine: 9}}}
	native := &sourceTraceMap{fileName: "a.ts", text: "native", mappings: []traceMapping{{generatedLine: 0, sourceLine: 0}, {generatedLine: 2, sourceLine: 3}}}
	prefix := &sourceTraceMap{fileName: "prefix.ts", text: "prefix"}
	run := func(program *compiler.Program) (*compiler.Program, []*ast.SourceFile, diagnosticTraces) {
		t.Helper()
		a, b, c := preparseSource(t, program, dir, "src/a.ts"), preparseSource(t, program, dir, "src/b.ts"), preparseSource(t, program, dir, "src/c.tsx")
		traces := diagnosticTraces{normalizeSourceFilePath(a.FileName()): authored, normalizeSourceFilePath(b.FileName()): shared, "prefix": prefix}
		updated, remapped, composed, err := updateNativeFlameworkProgram(nativeProgramUpdate{
			program: program, sourceFiles: projectSourceFiles(program), traces: traces,
			overlays: []nativeSourceOverlay{
				{fileName: c.FileName(), text: "export const c: number = 2;\n", trace: shared},
				{fileName: b.FileName(), text: "export const b = ;\n"},
				{fileName: a.FileName(), text: preparseDeprecatedUse, trace: native},
			},
		})
		if err != nil {
			t.Fatal(err)
		}
		return updated, remapped, composed
	}

	// When: the serial control and the candidate apply the same overlays.
	controlUpdated, controlRemapped, controlComposed := run(control)
	candidateUpdated, candidateRemapped, candidateComposed := run(candidate)

	// Then: the candidate matches the control file for file, in every ordered
	// diagnostic list and lazy JSDoc, in the remapped sources and in the traces.
	controlShape := preparseShape(t, control, controlUpdated)
	requireSameShape(t, controlShape, preparseShape(t, candidate, candidateUpdated))
	requireSameRemap(t, controlUpdated, controlRemapped, candidateUpdated, candidateRemapped)
	requireSameTraces(t, controlComposed, candidateComposed, shared, authored, native, prefix)
	requirePreparseRelease(t, controlUpdated, candidateUpdated)
	for _, want := range []string{"TS1109", "TS2322", "is deprecated", "jsdoc "} {
		if !slices.ContainsFunc(controlShape, func(line string) bool { return strings.Contains(line, want) }) {
			t.Fatalf("fixture shape has no %q; it no longer covers what it names", want)
		}
	}
	for _, name := range []string{"src/dep.ts", "src/untouched.ts", "src/globals.d.ts"} {
		path := filepath.Join(dir, filepath.FromSlash(name))
		if candidateUpdated.GetSourceFile(path) != candidate.GetSourceFile(path) {
			t.Fatalf("untouched %s lost its parse tree", name)
		}
	}
}

func TestPreparseTextOverlayUpdateMatchesSerialControl(t *testing.T) {
	// Given: sidecar overlays that change sources and a declaration, repeat one
	// source's text, and name a source deleted from disk after load.
	dir := writePreparseProject(t, map[string]string{
		"src/a.ts":         "export const a = 1;\n",
		"src/b.ts":         "export const b = 1;\n",
		"src/decl.d.ts":    "export declare const decl: number;\n",
		"src/gone.ts":      "export const gone = 1;\n",
		"src/same.ts":      "export const same = 1;\n",
		"src/untouched.ts": "export const untouched = 1;\n",
	})
	control, candidate := loadPreparsePrograms(t, dir)
	gone := filepath.Join(dir, "src", "gone.ts")
	if err := os.Remove(gone); err != nil {
		t.Fatal(err)
	}
	overlays := map[string]string{
		filepath.Join(dir, "src", "a.ts"):      "export const a: number = \"x\" as unknown as string;\n",
		filepath.Join(dir, "src", "b.ts"):      "export const b = ;\n",
		filepath.Join(dir, "src", "decl.d.ts"): "export declare const decl: string;\n",
		filepath.Join(dir, "src", "same.ts"):   "export const same = 1;\n",
		gone:                                   "export const gone = 2;\n",
	}
	run := func(program *compiler.Program) (*compiler.Program, []string) {
		t.Helper()
		updated, reused, err := updateProgramWithTextOverlays(program, overlays)
		if err != nil {
			t.Fatal(err)
		}
		if !reused {
			t.Fatal("text-only overlays rebuilt the Program")
		}
		for _, name := range []string{"src/same.ts", "src/untouched.ts"} {
			path := filepath.Join(dir, filepath.FromSlash(name))
			if updated.GetSourceFile(path) != program.GetSourceFile(path) {
				t.Fatalf("%s lost its parse tree", name)
			}
		}
		return updated, preparseShape(t, program, updated)
	}

	// When: the serial control and the candidate apply the same overlays.
	controlUpdated, controlShape := run(control)
	candidateUpdated, candidateShape := run(candidate)

	// Then: both give the same files, texts, kept parse trees and diagnostics.
	requireSameShape(t, controlShape, candidateShape)
	requirePreparseRelease(t, controlUpdated, candidateUpdated)
}

func TestPreparseNoOpUpdatesKeepTheInputProgram(t *testing.T) {
	// Given: overlays that change nothing.
	dir := writePreparseProject(t, map[string]string{
		"src/a.ts": "export const a = 1;\n",
		"src/b.ts": "export const b = 1;\n",
	})
	control, candidate := loadPreparsePrograms(t, dir)
	same := map[string]string{
		filepath.Join(dir, "src", "a.ts"): "export const a = 1;\n",
		filepath.Join(dir, "src", "b.ts"): "export const b = 1;\n",
	}
	for _, program := range []*compiler.Program{control, candidate} {
		// When/Then: each update returns its input Program, sources and traces.
		for _, overlays := range []map[string]string{nil, same} {
			updated, reused, err := updateProgramWithTextOverlays(program, overlays)
			if err != nil || !reused || updated != program {
				t.Fatalf("no-op text update = (%p, %v, %v), want input %p", updated, reused, err, program)
			}
		}
		sources := projectSourceFiles(program)
		traces := diagnosticTraces{"prefix": {fileName: "prefix.ts", text: "prefix"}}
		updated, remapped, composed, err := updateNativeFlameworkProgram(nativeProgramUpdate{program: program, sourceFiles: sources, traces: traces})
		if err != nil || updated != program || !slices.Equal(remapped, sources) || len(composed) != 1 || composed["prefix"] != traces["prefix"] {
			t.Fatal("no-op native update did not return its input")
		}
	}
}

func TestPreparseRebuildMatchesSerialControl(t *testing.T) {
	const zText = "import { late } from \"../lib/late\";\nexport const z = late + 1;\n"
	for _, scenario := range []struct {
		name     string
		overlays map[string]string
		absent   string
		native   bool
	}{
		{
			name:     "new import first",
			overlays: map[string]string{"src/a.ts": preparseNewImport, "src/m.ts": "export const m = 2;\n", "src/z.ts": zText},
			native:   true,
		},
		{
			name:     "new reference in the middle",
			overlays: map[string]string{"src/a.ts": "export const a = 2;\n", "src/m.ts": "/// <reference path=\"../lib/ref.d.ts\" />\nexport const m = 2;\n", "src/z.ts": zText},
		},
		{
			name:     "later changed file left out of the rebuilt graph",
			overlays: map[string]string{"src/a.ts": preparseNewImport, "lib/late.ts": "export const late = 2;\n", "src/z.ts": "export const z = 2;\n"},
			absent:   "lib/late.ts",
		},
	} {
		t.Run(scenario.name, func(t *testing.T) {
			// Given: overlays that make UpdateProgramFiles rebuild the whole Program.
			dir := writePreparseProject(t, preparseRebuildFiles)
			control, candidate := loadPreparsePrograms(t, dir)
			overlays := make(map[string]string, len(scenario.overlays))
			for name, text := range scenario.overlays {
				overlays[filepath.Join(dir, filepath.FromSlash(name))] = text
			}
			run := func(program *compiler.Program) (*compiler.Program, []string) {
				t.Helper()
				updated, reused, err := updateProgramWithTextOverlays(program, overlays)
				if err != nil {
					t.Fatal(err)
				}
				if reused {
					t.Fatal("changed file shape did not rebuild the Program")
				}
				if scenario.absent != "" && updated.GetSourceFile(filepath.Join(dir, filepath.FromSlash(scenario.absent))) != nil {
					t.Fatalf("rebuilt graph kept %s", scenario.absent)
				}
				return updated, preparseShape(t, program, updated)
			}

			// When: the serial control and the candidate apply the same overlays.
			controlUpdated, controlShape := run(control)
			candidateUpdated, candidateShape := run(candidate)

			// Then: both rebuild to the same Program.
			requireSameShape(t, controlShape, candidateShape)
			requirePreparseRelease(t, controlUpdated, candidateUpdated)
			if !scenario.native {
				return
			}

			// And: on fresh Programs, the native update rebuilds the same way.
			control, candidate = loadPreparsePrograms(t, dir)
			nativeRun := func(program *compiler.Program) (*compiler.Program, []*ast.SourceFile, diagnosticTraces) {
				t.Helper()
				overlays := make([]nativeSourceOverlay, 0, len(scenario.overlays))
				for name, text := range scenario.overlays {
					overlays = append(overlays, nativeSourceOverlay{fileName: preparseSource(t, program, dir, name).FileName(), text: text})
				}
				updated, remapped, composed, err := updateNativeFlameworkProgram(nativeProgramUpdate{program: program, sourceFiles: projectSourceFiles(program), overlays: overlays})
				if err != nil {
					t.Fatal(err)
				}
				return updated, remapped, composed
			}
			controlUpdated, controlRemapped, controlComposed := nativeRun(control)
			candidateUpdated, candidateRemapped, candidateComposed := nativeRun(candidate)
			requireSameShape(t, preparseShape(t, control, controlUpdated), preparseShape(t, candidate, candidateUpdated))
			requireSameRemap(t, controlUpdated, controlRemapped, candidateUpdated, candidateRemapped)
			requireSameTraces(t, controlComposed, candidateComposed)
			requirePreparseRelease(t, controlUpdated, candidateUpdated)
		})
	}
}

func TestPreparseRebuildLeavesUnrequestedParsesUnused(t *testing.T) {
	// Given: a changed file whose new import rebuilds the Program, ahead of a
	// changed file the rebuilt graph keeps and one it leaves out.
	dir := writePreparseProject(t, preparseRebuildFiles)
	program := loadPreparseProgram(t, dir, false)
	caseSensitive := program.UseCaseSensitiveFileNames()
	a, late, z := preparseSource(t, program, dir, "src/a.ts"), preparseSource(t, program, dir, "lib/late.ts"), preparseSource(t, program, dir, "src/z.ts")
	const zText = "export const z = 2;\n"
	overlays := map[string]string{
		normalizeOverlayPath(a.FileName(), caseSensitive):    preparseNewImport,
		normalizeOverlayPath(late.FileName(), caseSensitive): "export const late = 2;\n",
		normalizeOverlayPath(z.FileName(), caseSensitive):    zText,
	}
	changed := []tspath.Path{a.Path(), late.Path(), z.Path()}
	host := newNativeOverlayHostWithAliases(program.Host(), overlays, parallelOverlayAliases(overlays, caseSensitive))
	host.preparsed = preparseOverlays(program, overlays, changed, caseSensitive)
	if host.preparsed == nil || len(host.preparsed.files) != len(changed) {
		t.Fatal("the changed files were not all preparsed")
	}
	parsed := make(map[tspath.Path]*ast.SourceFile, len(changed))
	for opts, job := range host.preparsed.files {
		parsed[opts.Path] = job.file
	}

	// When: UpdateProgramFiles runs with the preparsed host.
	updated, reused := program.UpdateProgramFiles(changed, host, nil)

	// Then: the rebuild asked for a.ts again and parsed it fresh, took the z.ts
	// parse, and left the late.ts parse unused until release.
	if reused {
		t.Fatal("new import did not rebuild the Program")
	}
	if got := updated.GetSourceFile(a.FileName()); got == nil || got == parsed[a.Path()] || got.Text() != preparseNewImport {
		t.Fatal("rebuild did not parse the requested-again a.ts fresh")
	}
	if updated.GetSourceFile(z.FileName()) != parsed[z.Path()] {
		t.Fatal("rebuild did not take the preparsed z.ts")
	}
	if updated.GetSourceFile(late.FileName()) != nil {
		t.Fatal("rebuilt graph kept late.ts")
	}
	if len(host.preparsed.files) != 1 || host.preparsed.files[late.ParseOptions()] == nil {
		t.Fatalf("unused preparsed files = %d, want late.ts only", len(host.preparsed.files))
	}
	host.preparsed.release()
	if host.preparsed.files != nil {
		t.Fatal("release kept the unused parse")
	}
	if got := updated.Host().GetSourceFile(z.ParseOptions()); got == nil || got == parsed[z.Path()] || got.Text() != zText {
		t.Fatal("retained host did not parse fresh after release")
	}
}

func TestPreparseMissingAndRedirectPositionsMatchSerialControl(t *testing.T) {
	const sharedPackage = `{"name":"shared","version":"1.0.0","types":"index.d.ts"}`
	const sharedText = "export declare class Shared {\n\tprivate brand: number;\n}\n"
	files := map[string]string{
		"node_modules/left/index.d.ts":                        "import { Shared } from \"shared\";\nexport declare const left: Shared;\n",
		"node_modules/left/node_modules/shared/index.d.ts":    sharedText,
		"node_modules/left/node_modules/shared/package.json":  sharedPackage,
		"node_modules/left/package.json":                      `{"name":"left","version":"1.0.0","types":"index.d.ts"}`,
		"node_modules/right/index.d.ts":                       "import { Shared } from \"shared\";\nexport declare const right: Shared;\n",
		"node_modules/right/node_modules/shared/index.d.ts":   sharedText,
		"node_modules/right/node_modules/shared/package.json": sharedPackage,
		"node_modules/right/package.json":                     `{"name":"right","version":"1.0.0","types":"index.d.ts"}`,
		"src/a.ts":                                            "export const a = 1;\n",
		"src/b.ts":                                            "export const b = 1;\n",
		"src/c.ts":                                            "export const c = 1;\n",
		"src/main.ts":                                         "import { left } from \"left\";\nimport { right } from \"right\";\nexport const both = [left, right];\n",
	}
	const missing, target, source = "missing", "redirect target", "redirect source"
	for _, scenario := range []struct {
		changed []string
		jobs    int
	}{
		{[]string{missing, "a", "b", "c"}, 0},
		{[]string{"a", "b", missing, "c"}, 2},
		{[]string{"a", "b", "c", missing}, 3},
		{[]string{target, "a", "b", "c"}, 0},
		{[]string{"a", "b", target, "c"}, 2},
		{[]string{"a", "b", source, "c"}, 2},
		{[]string{"a", "b", "c", source}, 3},
	} {
		t.Run(strings.Join(scenario.changed, ","), func(t *testing.T) {
			// Given: changed paths with a missing path or a package redirect at
			// this position.
			dir := writePreparseProject(t, files)
			control, candidate := loadPreparsePrograms(t, dir)
			run := func(program *compiler.Program, preparse bool) (*compiler.Program, []string) {
				t.Helper()
				caseSensitive := program.UseCaseSensitiveFileNames()
				// Package files resolve to real paths, so find the kept copy in
				// the Program rather than spell its path from dir.
				var targetFile *ast.SourceFile
				for _, file := range program.SourceFiles() {
					if strings.HasSuffix(file.FileName(), "/node_modules/shared/index.d.ts") {
						targetFile = file
					}
				}
				if targetFile == nil || len(program.GetRedirectTargets(targetFile.Path())) != 1 {
					t.Fatal("fixture did not form a package redirect")
				}
				sourceName := program.GetRedirectTargets(targetFile.Path())[0]
				sourcePath := tspath.ToPath(sourceName, program.GetCurrentDirectory(), caseSensitive)
				if sourcePath == targetFile.Path() || program.GetSourceFileByPath(sourcePath) != targetFile {
					t.Fatal("fixture redirect source does not map to its target")
				}
				const changedShared = "export declare class Shared {\n\tprivate brand: string;\n}\n"
				overlays := map[string]string{
					normalizeOverlayPath(targetFile.FileName(), caseSensitive): changedShared,
					normalizeOverlayPath(sourceName, caseSensitive):            changedShared,
				}
				missingPath := tspath.ToPath(filepath.ToSlash(filepath.Join(dir, "src", "missing.ts")), program.GetCurrentDirectory(), caseSensitive)
				paths := map[string]tspath.Path{missing: missingPath, target: targetFile.Path(), source: sourcePath}
				for _, name := range []string{"a", "b", "c"} {
					file := preparseSource(t, program, dir, "src/"+name+".ts")
					paths[name] = file.Path()
					overlays[normalizeOverlayPath(file.FileName(), caseSensitive)] = fmt.Sprintf("export const %s = 2;\n", name)
				}
				changed := make([]tspath.Path, 0, len(scenario.changed))
				for _, name := range scenario.changed {
					changed = append(changed, paths[name])
				}
				if preparse {
					jobs := 0
					preparsed := preparseOverlays(program, overlays, changed, caseSensitive)
					if preparsed != nil {
						jobs = len(preparsed.files)
					}
					preparsed.release()
					if jobs != scenario.jobs {
						t.Fatalf("preparse jobs = %d, want %d (the paths before the first missing or redirect path)", jobs, scenario.jobs)
					}
				}
				updated, reused := updateProgramFilesWithOverlays(program, overlays, changed)
				if reused {
					t.Fatal("missing or redirect path did not rebuild the Program")
				}
				return updated, preparseShape(t, program, updated)
			}

			// When: the serial control and the candidate apply the same update.
			_, controlShape := run(control, false)
			_, candidateShape := run(candidate, true)

			// Then: both rebuild to the same Program.
			requireSameShape(t, controlShape, candidateShape)
		})
	}
}

func TestPreparseDuplicateAndRetainedRequestsMatchSerialControl(t *testing.T) {
	// Given: a changed path that is listed twice.
	dir := writePreparseProject(t, map[string]string{
		"src/a.ts": "export const a = 1;\n",
		"src/b.ts": "export const b = 1;\n",
		"src/c.ts": "export const c = 1;\n",
	})
	control, candidate := loadPreparsePrograms(t, dir)
	const aText = "export const a = 2;\n"
	run := func(program *compiler.Program) (*compiler.Program, []string) {
		t.Helper()
		caseSensitive := program.UseCaseSensitiveFileNames()
		a, b := preparseSource(t, program, dir, "src/a.ts"), preparseSource(t, program, dir, "src/b.ts")
		overlays := map[string]string{
			normalizeOverlayPath(a.FileName(), caseSensitive): aText,
			normalizeOverlayPath(b.FileName(), caseSensitive): "export const b = 2;\n",
		}

		// When: the update runs, then the host that the Program keeps gets two
		// more requests.
		updated, reused := updateProgramFilesWithOverlays(program, overlays, []tspath.Path{a.Path(), b.Path(), a.Path()})
		if !reused {
			t.Fatal("duplicate text-only update rebuilt the Program")
		}
		first, second := updated.Host().GetSourceFile(a.ParseOptions()), updated.Host().GetSourceFile(a.ParseOptions())

		// Then: each of these requests parses fresh from the overlay.
		current := updated.GetSourceFile(a.FileName())
		if first == nil || second == nil || first == second || first == current || second == current || first.Text() != aText || first.ParseOptions() != a.ParseOptions() {
			t.Fatal("retained host request did not parse fresh from the overlay")
		}
		return updated, preparseShape(t, program, updated)
	}
	controlUpdated, controlShape := run(control)
	candidateUpdated, candidateShape := run(candidate)
	requireSameShape(t, controlShape, candidateShape)
	requirePreparseRelease(t, controlUpdated, candidateUpdated)
}

func TestPreparseOverlaysParsesEachRequestableChangedFileOnce(t *testing.T) {
	// Given: overlays for three Program files, and a changed file without one.
	dir := writePreparseProject(t, map[string]string{
		"src/a.ts": "export const a = 1;\n",
		"src/b.ts": "export const b = 1;\n",
		"src/c.ts": "export const c = 1;\n",
		"src/d.ts": "export const d = 1;\n",
	})
	program := loadPreparseProgram(t, dir, false)
	caseSensitive := program.UseCaseSensitiveFileNames()
	files := make(map[string]*ast.SourceFile)
	overlays := make(map[string]string)
	for _, name := range []string{"a", "b", "c"} {
		file := preparseSource(t, program, dir, "src/"+name+".ts")
		files[name] = file
		overlays[normalizeOverlayPath(file.FileName(), caseSensitive)] = fmt.Sprintf("export const %s = 2;\n", name)
	}
	a, b, c, d := files["a"].Path(), files["b"].Path(), files["c"].Path(), preparseSource(t, program, dir, "src/d.ts").Path()
	missing := tspath.ToPath(filepath.ToSlash(filepath.Join(dir, "src", "missing.ts")), program.GetCurrentDirectory(), caseSensitive)
	for _, scenario := range []struct {
		name    string
		changed []tspath.Path
		want    []string
	}{
		{"duplicate and no overlay before a missing path", []tspath.Path{b, a, b, d, missing, c}, []string{"b", "a"}},
		{"all", []tspath.Path{a, b, c}, []string{"a", "b", "c"}},
		{"one job", []tspath.Path{a}, nil},
		{"one job before a missing path", []tspath.Path{a, missing, b}, nil},
		{"missing first", []tspath.Path{missing, a, b}, nil},
	} {
		t.Run(scenario.name, func(t *testing.T) {
			// When: the overlays preparse for the changed paths.
			preparsed := preparseOverlays(program, overlays, scenario.changed, caseSensitive)
			defer preparsed.release()

			// Then: each listed file parses once with its Program options and
			// overlay text, nothing else parses, and one job gets no work group.
			if scenario.want == nil {
				if preparsed != nil {
					t.Fatalf("preparsed %d files, want none", len(preparsed.files))
				}
				return
			}
			if preparsed == nil || len(preparsed.files) != len(scenario.want) {
				t.Fatalf("preparsed files do not match %v", scenario.want)
			}
			for _, name := range scenario.want {
				old := files[name]
				text := overlays[normalizeOverlayPath(old.FileName(), caseSensitive)]
				job := preparsed.files[old.ParseOptions()]
				if job == nil || job.panicked || job.text != text || job.file == nil || job.file == old {
					t.Fatalf("%s.ts was not preparsed from its overlay", name)
				}
				if job.file.Text() != text || job.file.ParseOptions() != old.ParseOptions() || job.file.ScriptKind != old.ScriptKind {
					t.Fatalf("%s.ts preparse lost its text, parse options or script kind", name)
				}
			}
		})
	}
}

func TestPreparseTakeNeedsExactOptionsAndText(t *testing.T) {
	for _, caseSensitive := range []bool{true, false} {
		t.Run(fmt.Sprintf("caseSensitive=%v", caseSensitive), func(t *testing.T) {
			// Given: a host with one preparsed overlay and one physical alias.
			const fileName = "/project/src/Source.ts"
			const aliasName = "/physical/src/Source.ts"
			const text = "/** Source. */\nexport const source = 1;\n"
			opts := ast.SourceFileParseOptions{FileName: fileName, Path: tspath.Path(fileName), ExternalModuleIndicatorOptions: ast.ExternalModuleIndicatorOptions{Force: true}}
			base := compiler.NewCompilerHost("/project", vfstest.FromMap(map[string]string{}, caseSensitive), "", nil, nil)
			host := newNativeOverlayHostWithAliases(base,
				map[string]string{normalizeOverlayPath(fileName, caseSensitive): text},
				map[string]string{normalizeOverlayPath(aliasName, caseSensitive): text})
			preparse := func() *ast.SourceFile {
				job := &overlayParse{opts: opts, text: text}
				job.parse()
				host.preparsed = &preparsedOverlays{files: map[ast.SourceFileParseOptions]*overlayParse{opts: job}}
				return job.file
			}
			requireFresh := func(request ast.SourceFileParseOptions, preparsed *ast.SourceFile) {
				t.Helper()
				got := host.GetSourceFile(request)
				if got == nil || got == preparsed || got.Text() != text || got.ParseOptions() != request {
					t.Fatalf("request %+v did not parse fresh from the overlay", request)
				}
			}

			// When/Then: only the first exact request takes the parse; the next
			// one parses fresh.
			preparsed := preparse()
			if host.GetSourceFile(opts) != preparsed {
				t.Fatal("exact request did not take the preparsed file")
			}
			requireFresh(opts, preparsed)

			// When/Then: a request with another path, other module flags, another
			// case or the alias spelling parses fresh and leaves the parse alone.
			preparsed = preparse()
			variants := []ast.SourceFileParseOptions{
				{FileName: fileName, Path: "/canonical/src/source.ts", ExternalModuleIndicatorOptions: opts.ExternalModuleIndicatorOptions},
				{FileName: fileName, Path: opts.Path},
				{FileName: fileName, Path: opts.Path, ExternalModuleIndicatorOptions: ast.ExternalModuleIndicatorOptions{JSX: true, Force: true}},
				{FileName: aliasName, Path: tspath.Path(aliasName), ExternalModuleIndicatorOptions: opts.ExternalModuleIndicatorOptions},
			}
			lower := ast.SourceFileParseOptions{FileName: "/project/src/source.ts", Path: "/project/src/source.ts", ExternalModuleIndicatorOptions: opts.ExternalModuleIndicatorOptions}
			if caseSensitive {
				if host.GetSourceFile(lower) != nil {
					t.Fatal("case-sensitive host read the overlay through another case")
				}
			} else {
				variants = append(variants, lower)
			}
			for _, variant := range variants {
				requireFresh(variant, preparsed)
			}
			if host.GetSourceFile(opts) != preparsed {
				t.Fatal("variant requests used the exact request's parse")
			}

			// When/Then: a text mismatch drops the parse, and so does release.
			preparsed = preparse()
			if host.preparsed.take(opts, "export const other = 1;\n") != nil {
				t.Fatal("text mismatch took the preparsed file")
			}
			requireFresh(opts, preparsed)
			preparsed = preparse()
			host.preparsed.release()
			requireFresh(opts, preparsed)
			var none *preparsedOverlays
			none.release()
			if none.take(opts, text) != nil {
				t.Fatal("nil preparse gave a file")
			}
		})
	}
}

func TestPreparseTakeAndReleaseAreSafeUnderConcurrentRequests(t *testing.T) {
	// Given: preparsed overlays for several files.
	newPreparsed := func() (*preparsedOverlays, []*overlayParse) {
		jobs := make([]*overlayParse, 0, 4)
		files := make(map[ast.SourceFileParseOptions]*overlayParse, 4)
		for index := range 4 {
			fileName := fmt.Sprintf("/project/src/file%d.ts", index)
			job := &overlayParse{opts: ast.SourceFileParseOptions{FileName: fileName, Path: tspath.Path(fileName)}, text: fmt.Sprintf("export const value%d = %d;\n", index, index)}
			job.parse()
			jobs = append(jobs, job)
			files[job.opts] = job
		}
		return &preparsedOverlays{files: files}, jobs
	}
	request := func(preparsed *preparsedOverlays, jobs []*overlayParse, release bool) []int32 {
		taken := make([]atomic.Int32, len(jobs))
		var wg sync.WaitGroup
		for range 8 {
			for index, job := range jobs {
				wg.Add(1)
				go func() {
					defer wg.Done()
					if file := preparsed.take(job.opts, job.text); file != nil {
						if file != job.file {
							t.Errorf("request for %s took another file", job.opts.FileName)
						}
						taken[index].Add(1)
					}
				}()
			}
		}
		if release {
			wg.Add(1)
			go func() {
				defer wg.Done()
				preparsed.release()
			}()
		}
		wg.Wait()
		counts := make([]int32, len(jobs))
		for index := range taken {
			counts[index] = taken[index].Load()
		}
		return counts
	}

	// When/Then: concurrent requests take each parse exactly once.
	preparsed, jobs := newPreparsed()
	for index, count := range request(preparsed, jobs, false) {
		if count != 1 {
			t.Fatalf("file%d taken %d times, want once", index, count)
		}
	}

	// When/Then: requests that race with release take each parse at most once,
	// and no request after release takes one.
	preparsed, jobs = newPreparsed()
	for index, count := range request(preparsed, jobs, true) {
		if count > 1 {
			t.Fatalf("file%d taken %d times, want at most once", index, count)
		}
	}
	for _, job := range jobs {
		if preparsed.take(job.opts, job.text) != nil {
			t.Fatal("request after release took a file")
		}
	}
}

func TestPreparsePanicRaisesOnlyOnRequest(t *testing.T) {
	// Given: an overlay parse that panics, as the parser does for a file name
	// that is not absolute.
	opts := ast.SourceFileParseOptions{FileName: "src/relative.ts", Path: "src/relative.ts"}
	const text = "export const relative = 1;\n"
	want := preparseRecover(func() { parser.ParseSourceFile(opts, text, core.GetScriptKindFromFileName(opts.FileName)) })
	if want == nil {
		t.Fatal("parser accepted a relative file name; the fixture needs a parse panic")
	}
	job := &overlayParse{opts: opts, text: text}

	// When: the work group runs the parse.
	job.parse()

	// Then: the job keeps the panic, and only a request for it raises the same
	// value on the requesting goroutine.
	if !job.panicked || job.recovered != want || job.file != nil {
		t.Fatalf("job = (panicked %v, recovered %v), want the parser panic %v", job.panicked, job.recovered, want)
	}
	base := compiler.NewCompilerHost("/project", vfstest.FromMap(map[string]string{}, true), "", nil, nil)
	host := newNativeOverlayHostWithAliases(base, map[string]string{normalizeOverlayPath(opts.FileName, true): text}, nil)
	host.preparsed = &preparsedOverlays{files: map[ast.SourceFileParseOptions]*overlayParse{opts: job}}
	other := ast.SourceFileParseOptions{FileName: "/project/src/other.ts", Path: "/project/src/other.ts"}
	if got := preparseRecover(func() { host.preparsed.take(other, text) }); got != nil {
		t.Fatalf("request for another file raised %v", got)
	}
	if got := preparseRecover(func() { host.GetSourceFile(opts) }); got != want {
		t.Fatalf("request raised %v, want %v", got, want)
	}
	unused := &preparsedOverlays{files: map[ast.SourceFileParseOptions]*overlayParse{opts: job}}
	if got := preparseRecover(unused.release); got != nil {
		t.Fatalf("release of an unused panicked parse raised %v", got)
	}
}

func TestPreparseComposeTracesMatchesSerialComposition(t *testing.T) {
	// Given: traces that share pointers, nil traces, and a repeated file.
	dir := writePreparseProject(t, map[string]string{"src/a.ts": "export const a = 1;\n"})
	control, candidate := loadPreparsePrograms(t, dir)
	inner := &sourceTraceMap{fileName: "inner.ts", text: "inner", mappings: []traceMapping{{generatedLine: 0, sourceLine: 3}, {generatedLine: 1, sourceLine: 5}}}
	outer := &sourceTraceMap{fileName: "outer.ts", text: "outer", mappings: []traceMapping{{generatedLine: 0, sourceLine: 0}, {generatedLine: 4, sourceLine: 1}}}
	last := &sourceTraceMap{fileName: "last.ts", text: "last", mappings: []traceMapping{{generatedLine: 2, sourceLine: 1}}}
	prefix := &sourceTraceMap{fileName: "prefix.ts", text: "prefix"}
	keyA, keyB, keyC := normalizeSourceFilePath("/project/a.ts"), normalizeSourceFilePath("/project/b.ts"), normalizeSourceFilePath("/project/c.ts")
	traces := diagnosticTraces{keyA: inner, keyB: inner, "prefix": prefix}
	overlays := []nativeSourceOverlay{
		{fileName: "/project/a.ts", trace: outer},
		{fileName: "/project/b.ts"},
		{fileName: "/project/c.ts", trace: outer},
		{fileName: "/project/a.ts", trace: last},
	}
	clone := func(trace *sourceTraceMap) sourceTraceMap {
		cloned := *trace
		cloned.mappings = slices.Clone(trace.mappings)
		return cloned
	}
	before := []sourceTraceMap{clone(inner), clone(outer), clone(last), clone(prefix)}
	// The loop the native update ran before: copy, then compose in order.
	want := make(diagnosticTraces, len(traces)+len(overlays))
	for key, trace := range traces {
		want[key] = trace
	}
	for _, overlay := range overlays {
		key := normalizeSourceFilePath(overlay.fileName)
		want[key] = composeSourceTraceMaps(overlay.trace, traces[key])
	}

	for _, program := range []*compiler.Program{control, candidate} {
		// When: the overlays compose in the Program's work group.
		got := composeNativeOverlayTraces(program, traces, overlays)

		// Then: the values match the serial loop, pass-through traces keep their
		// pointers, the last overlay for a file wins, and the inputs stay as they were.
		if !reflect.DeepEqual(got, want) {
			t.Fatalf("composed traces differ from the serial loop (single-threaded %v)", program.SingleThreaded())
		}
		if got[keyB] != inner || got[keyC] != outer || got["prefix"] != prefix || got[keyA] == inner || got[keyA] == outer || got[keyA] == last {
			t.Fatal("composed traces lost pass-through identity or did not compose")
		}
		if !reflect.DeepEqual(got[keyA], composeSourceTraceMaps(last, inner)) {
			t.Fatal("repeated file did not keep its last overlay's trace")
		}
		after := []sourceTraceMap{clone(inner), clone(outer), clone(last), clone(prefix)}
		if len(traces) != 3 || traces[keyA] != inner || traces[keyB] != inner || traces["prefix"] != prefix || !reflect.DeepEqual(after, before) {
			t.Fatal("composition changed its inputs")
		}
	}
}

func TestPreparseAliasesMatchSerialAliases(t *testing.T) {
	// Given: overlays for existing, missing and relative paths, and for a path
	// through a symlinked parent when the platform permits one.
	dir := t.TempDir()
	for _, name := range []string{"one.ts", "two.ts", filepath.Join("sub", "three.ts"), "five.ts"} {
		path := filepath.Join(dir, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte("export {};\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	overlays := map[string]string{
		filepath.Join(dir, "one.ts"):          "one",
		filepath.Join(dir, "two.ts"):          "two",
		filepath.Join(dir, "sub", "three.ts"): "three",
		filepath.Join(dir, "missing.ts"):      "missing",
		filepath.Join("relative", "four.ts"):  "four",
	}
	linked := filepath.Join(t.TempDir(), "linked")
	if err := os.Symlink(dir, linked); err == nil {
		overlays[filepath.Join(linked, "five.ts")] = "five"
	} else {
		t.Logf("no symlinked parent: %v", err)
	}
	compare := func() map[string]string {
		t.Helper()
		var serial map[string]string
		for _, caseSensitive := range []bool{true, false} {
			normalized := make(map[string]string, len(overlays))
			for path, text := range overlays {
				normalized[normalizeOverlayPath(path, caseSensitive)] = text
			}
			got, want := parallelOverlayAliases(normalized, caseSensitive), overlayAliases(normalized, "", caseSensitive)
			if !maps.Equal(got, want) {
				t.Fatalf("parallel aliases differ from serial aliases (case-sensitive %v):\nparallel %v\nserial   %v", caseSensitive, got, want)
			}
			if caseSensitive {
				serial = want
			}
		}
		return serial
	}

	// When/Then: the parallel aliases equal the serial ones in both case modes.
	compare()

	// When/Then: changing the working directory preserves the serial result.
	// The Windows realpath wrapper makes "." absolute; on other platforms it
	// stays relative, so the serial code cannot make a relative alias from it.
	t.Chdir(dir)
	aliases := compare()
	text, hasRelative := aliases[normalizeOverlayPath("one.ts", true)]
	if hasRelative != (runtime.GOOS == "windows") || (hasRelative && text != "one") {
		t.Fatalf("working-directory relative alias = %q, present %v", text, hasRelative)
	}

	// And: zero or one overlay gives the serial result.
	for _, small := range []map[string]string{nil, {filepath.Join(dir, "one.ts"): "one"}} {
		if got, want := parallelOverlayAliases(small, true), overlayAliases(small, "", true); !maps.Equal(got, want) {
			t.Fatal("small overlay sets differ from serial aliases")
		}
	}
}

func TestPreparseAliasMergeKeepsEntryOrder(t *testing.T) {
	// Given: two spellings of one physical file with different texts.
	link := normalizeOverlayPath("/project/link/a.ts", true)
	physical := normalizeOverlayPath("/project/real/a.ts", true)
	relative := normalizeOverlayPath(filepath.Join("real", "a.ts"), true)
	first := overlayAliasEntry{path: link, text: "first", physical: physical, resolved: true}
	second := overlayAliasEntry{path: physical, text: "second", physical: physical, resolved: true}
	noConfig := errors.New("config directory does not resolve")
	canonical := filepath.FromSlash("/project")
	for _, scenario := range []struct {
		name    string
		entries []overlayAliasEntry
		config  error
		want    map[string]string
	}{
		{"first then second", []overlayAliasEntry{first, second}, noConfig, map[string]string{link: "first", physical: "second"}},
		{"second then first", []overlayAliasEntry{second, first}, noConfig, map[string]string{link: "first", physical: "first"}},
		{"first then second under the config directory", []overlayAliasEntry{first, second}, nil, map[string]string{link: "first", physical: "second", relative: "second"}},
		{"second then first under the config directory", []overlayAliasEntry{second, first}, nil, map[string]string{link: "first", physical: "first", relative: "first"}},
	} {
		// When: the entries merge in this snapshot order.
		got := mergeOverlayAliases(scenario.entries, ".", canonical, scenario.config, true)

		// Then: the later entry wins each alias that both write, as the serial
		// loop does when it visits the overlays in this order.
		if !maps.Equal(got, scenario.want) {
			t.Fatalf("%s: aliases = %v, want %v", scenario.name, got, scenario.want)
		}
	}

	// When/Then: a stored lookup panic raises at its entry.
	failed := overlayAliasEntry{path: normalizeOverlayPath("/project/failed.ts", true), text: "failed", panicked: true, recovered: "lookup failed"}
	if got := preparseRecover(func() { mergeOverlayAliases([]overlayAliasEntry{first, failed, second}, ".", canonical, nil, true) }); got != "lookup failed" {
		t.Fatalf("merge raised %v, want the stored lookup panic", got)
	}
}

func TestPreparseMissingSourceErrorsMatchSerialControl(t *testing.T) {
	// Given: overlays that name a source the Program does not hold.
	dir := writePreparseProject(t, map[string]string{
		"src/a.ts": "export const a = 1;\n",
		"src/b.ts": "export const b = 1;\n",
	})
	control, candidate := loadPreparsePrograms(t, dir)
	missing := filepath.Join(dir, "src", "missing.ts")
	for _, program := range []*compiler.Program{control, candidate} {
		a := preparseSource(t, program, dir, "src/a.ts")

		// When/Then: both updates fail with the same error.
		_, _, _, err := updateNativeFlameworkProgram(nativeProgramUpdate{
			program: program, sourceFiles: projectSourceFiles(program),
			overlays: []nativeSourceOverlay{{fileName: a.FileName(), text: "export const a = 2;\n"}, {fileName: missing, text: "export {};\n"}},
		})
		if err == nil || err.Error() != "compile: native Flamework source missing from Program: "+missing {
			t.Fatalf("native update error = %v", err)
		}
		_, _, err = updateProgramWithTextOverlays(program, map[string]string{filepath.Join(dir, "src", "a.ts"): "export const a = 2;\n", missing: "export {};\n"})
		if err == nil || err.Error() != "compile: overlay source missing from Program: "+missing {
			t.Fatalf("text update error = %v", err)
		}
	}
}

// writePreparseProject writes a project whose src holds the globals stub and
// the given files; names are relative to the project root.
func writePreparseProject(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := writeProject(t, "@scope/preparse-update", "")
	if err := os.Remove(filepath.Join(dir, "src", "main.ts")); err != nil {
		t.Fatal(err)
	}
	for name, text := range files {
		path := filepath.Join(dir, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

// loadPreparsePrograms loads the serial control and the candidate as fresh
// Programs from dir.
func loadPreparsePrograms(t *testing.T, dir string) (control, candidate *compiler.Program) {
	t.Helper()
	return loadPreparseProgram(t, dir, true), loadPreparseProgram(t, dir, false)
}

// loadPreparseProgram loads a fresh Program with its own host and one checker,
// so the two Programs of a test differ only in threading.
func loadPreparseProgram(t *testing.T, dir string, singleThreaded bool) *compiler.Program {
	t.Helper()
	checkers := 1
	_, program, diagnostics, err := newProjectProgramWithOptions(dir, "", ProjectOptions{Checkers: &checkers, SingleThreaded: &singleThreaded})
	if err != nil {
		t.Fatalf("newProjectProgramWithOptions: %v (%v)", err, diagnostics)
	}
	if program.SingleThreaded() != singleThreaded {
		t.Fatalf("Program.SingleThreaded() = %v, want %v", program.SingleThreaded(), singleThreaded)
	}
	return program
}

func preparseSource(t *testing.T, program *compiler.Program, dir, name string) *ast.SourceFile {
	t.Helper()
	source := program.GetSourceFile(filepath.Join(dir, filepath.FromSlash(name)))
	if source == nil {
		t.Fatalf("fixture source missing from Program: %s", name)
	}
	return source
}

// preparseShape describes an updated Program against the Program it came
// from. It queries the diagnostics first, in a fixed order, then lists each
// file in Program order with its parse options, script kind, text, whether the
// old parse tree stayed, its parse diagnostics and the lazy JSDoc of its
// statements.
func preparseShape(t *testing.T, old, updated *compiler.Program) []string {
	t.Helper()
	ctx := context.Background()
	shape := []string{fmt.Sprintf("files %d", len(updated.SourceFiles()))}
	shape = append(shape, preparseDiagnostics("semantic", updated.GetSemanticDiagnostics(ctx, nil))...)
	shape = append(shape, preparseDiagnostics("suggestion", updated.GetSuggestionDiagnostics(ctx, nil))...)
	shape = append(shape, preparseDiagnostics("syntactic", updated.GetSyntacticDiagnostics(ctx, nil))...)
	shape = append(shape, preparseDiagnostics("global", updated.GetGlobalDiagnostics(ctx))...)
	shape = append(shape, preparseDiagnostics("program", updated.GetProgramDiagnostics())...)
	for _, file := range updated.SourceFiles() {
		kept := old.GetSourceFileByPath(file.Path()) == file
		shape = append(shape, fmt.Sprintf("file %s opts=%+v kind=%v declaration=%v kept=%v text=%q", file.FileName(), file.ParseOptions(), file.ScriptKind, file.IsDeclarationFile, kept, file.Text()))
		shape = append(shape, preparseDiagnostics("parse", file.Diagnostics())...)
		for index, statement := range file.Statements.Nodes {
			for _, jsdoc := range statement.JSDoc(file) {
				shape = append(shape, fmt.Sprintf("jsdoc %s statement=%d kind=%v range=%d-%d", file.FileName(), index, jsdoc.Kind, jsdoc.Pos(), jsdoc.End()))
			}
		}
	}
	return shape
}

func preparseDiagnostics(label string, diagnostics []*ast.Diagnostic) []string {
	lines := make([]string, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		fileName := "<none>"
		if diagnostic.File() != nil {
			fileName = diagnostic.File().FileName()
		}
		lines = append(lines, fmt.Sprintf("%s %s %d+%d %v TS%d %s", label, fileName, diagnostic.Pos(), diagnostic.Len(), diagnostic.Category(), diagnostic.Code(), diagnostic.String()))
	}
	return lines
}

func requireSameShape(t *testing.T, control, candidate []string) {
	t.Helper()
	if !slices.Equal(control, candidate) {
		t.Fatalf("candidate differs from the serial control\ncontrol:\n%s\ncandidate:\n%s", strings.Join(control, "\n"), strings.Join(candidate, "\n"))
	}
}

func requireSameRemap(t *testing.T, controlProgram *compiler.Program, control []*ast.SourceFile, candidateProgram *compiler.Program, candidate []*ast.SourceFile) {
	t.Helper()
	if len(control) != len(candidate) {
		t.Fatalf("remapped sources: control %d, candidate %d", len(control), len(candidate))
	}
	for index := range control {
		if control[index].FileName() != candidate[index].FileName() {
			t.Fatalf("remapped source[%d]: control %s, candidate %s", index, control[index].FileName(), candidate[index].FileName())
		}
		if control[index] != controlProgram.GetSourceFile(control[index].FileName()) || candidate[index] != candidateProgram.GetSourceFile(candidate[index].FileName()) {
			t.Fatalf("remapped source[%d] is not the updated Program's file", index)
		}
	}
}

// requireSameTraces compares composed traces by value, and requires a trace
// that passes through from the inputs to stay the same pointer.
func requireSameTraces(t *testing.T, control, candidate diagnosticTraces, inputs ...*sourceTraceMap) {
	t.Helper()
	if !reflect.DeepEqual(control, candidate) {
		t.Fatal("candidate composed traces differ from the serial control")
	}
	for key, trace := range control {
		if trace != nil && slices.Contains(inputs, trace) != (candidate[key] == trace) {
			t.Fatalf("trace %s: pass-through identity differs from the serial control", key)
		}
	}
}

// requirePreparseRelease checks the overlay host that each updated Program
// keeps: the serial control has no preparse, and the candidate's is released.
func requirePreparseRelease(t *testing.T, control, candidate *compiler.Program) {
	t.Helper()
	if host, ok := control.Host().(nativeOverlayHost); !ok || host.preparsed != nil {
		t.Fatalf("serial control host %T has a preparse or is not the overlay host", control.Host())
	}
	if host, ok := candidate.Host().(nativeOverlayHost); !ok || host.preparsed == nil || host.preparsed.files != nil {
		t.Fatalf("candidate host %T has no released preparse", candidate.Host())
	}
}

func preparseRecover(run func()) (recovered any) {
	defer func() {
		recovered = recover()
	}()
	run()
	return nil
}
