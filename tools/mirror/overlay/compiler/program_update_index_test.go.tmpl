package compiler

import (
	"fmt"
	"maps"
	"slices"
	"sync"
	"sync/atomic"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/vfstest"
)

// These tests compare UpdateProgramFiles with the per-replacement linear search it used for
// every batch size. The fixed batch sizes 1, 7, 8 and 12 cover both sides of the current
// indexed-lookup cutoff (8) without depending on it: every size must give the same files,
// map and reuse result as the linear search.

const indexTestFileCount = 12

func batchSizes() []int {
	return []int{1, 7, 8, 12}
}

func indexTestPath(i int) tspath.Path {
	return tspath.Path(fmt.Sprintf("/project/f%d.ts", i))
}

func indexTestSources(changed map[int]string, extra map[string]string) map[string]string {
	sources := make(map[string]string, indexTestFileCount+len(extra))
	for i := range indexTestFileCount {
		sources[string(indexTestPath(i))] = fmt.Sprintf("export const v%d = 1;\n", i)
	}
	for i, text := range changed {
		sources[string(indexTestPath(i))] = text
	}
	maps.Copy(sources, extra)
	return sources
}

// newIndexTestProgram loads f0..f11 as roots. Files in extra exist on disk but are not roots,
// so the Program does not load them until an import needs them.
func newIndexTestProgram(t *testing.T, extra map[string]string) *Program {
	t.Helper()
	roots := make([]string, indexTestFileCount)
	for i := range roots {
		roots[i] = string(indexTestPath(i))
	}
	config := tsoptions.NewParsedCommandLine(&core.CompilerOptions{
		NoLib:            core.TSTrue,
		Types:            []string{},
		Module:           core.ModuleKindESNext,
		ModuleResolution: core.ModuleResolutionKindBundler,
	}, roots, tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true, CurrentDirectory: "/project"})
	host := NewCompilerHost("/project", vfstest.FromMap(indexTestSources(nil, extra), true), "", nil, nil)
	program := NewProgram(ProgramOptions{Host: host, Config: config, CreateCheckerPool: func(*Program) CheckerPool { return nil }})
	if len(program.files) != indexTestFileCount {
		t.Fatalf("program has %d files, want %d", len(program.files), indexTestFileCount)
	}
	return program
}

// recordingHost returns a fresh parse per request and records it in request order, so the
// expected result can replay exactly the files UpdateProgramFiles received. UpdateProgramFiles
// requests the changed files one at a time, so they are the first records; a NewProgram
// fallback can then parse in parallel, hence the lock.
type recordingHost struct {
	CompilerHost
	mu     sync.Mutex
	parsed []*ast.SourceFile
}

func newRecordingHost(changed map[int]string, extra map[string]string) *recordingHost {
	return &recordingHost{CompilerHost: NewCompilerHost("/project", vfstest.FromMap(indexTestSources(changed, extra), true), "", nil, nil)}
}

func (h *recordingHost) GetSourceFile(opts ast.SourceFileParseOptions) *ast.SourceFile {
	file := h.CompilerHost.GetSourceFile(opts)
	h.mu.Lock()
	h.parsed = append(h.parsed, file)
	h.mu.Unlock()
	return file
}

func (h *recordingHost) recorded() []*ast.SourceFile {
	h.mu.Lock()
	defer h.mu.Unlock()
	return slices.Clone(h.parsed)
}

// update runs UpdateProgramFiles with a checker-pool factory that counts Program constructions.
func update(t *testing.T, program *Program, paths []tspath.Path, host *recordingHost) (*Program, bool) {
	t.Helper()
	var pools atomic.Int32
	updated, reused := program.UpdateProgramFiles(paths, host, func(*Program) CheckerPool {
		pools.Add(1)
		return nil
	})
	if got := pools.Load(); got != 1 {
		t.Fatalf("update constructed %d Programs, want 1", got)
	}
	return updated, reused
}

// linearReplacement replays the per-replacement first-match search on copies of the old state.
func linearReplacement(old *Program, paths []tspath.Path, parsed []*ast.SourceFile) ([]*ast.SourceFile, map[tspath.Path]*ast.SourceFile, bool) {
	files := slices.Clone(old.files)
	filesByPath := maps.Clone(old.filesByPath)
	for i, path := range paths {
		index := slices.IndexFunc(files, func(file *ast.SourceFile) bool { return file.Path() == path })
		if index < 0 {
			return nil, nil, false
		}
		files[index] = parsed[i]
		filesByPath[path] = parsed[i]
	}
	return files, filesByPath, true
}

type programSnapshot struct {
	files           []*ast.SourceFile
	filesByPath     map[tspath.Path]*ast.SourceFile
	resolvedModules map[tspath.Path]int
}

func snapshotProgram(p *Program) programSnapshot {
	resolved := make(map[tspath.Path]int, len(p.resolvedModules))
	for path, cache := range p.resolvedModules {
		resolved[path] = len(cache)
	}
	return programSnapshot{files: slices.Clone(p.files), filesByPath: maps.Clone(p.filesByPath), resolvedModules: resolved}
}

func assertUnchanged(t *testing.T, p *Program, before programSnapshot) {
	t.Helper()
	after := snapshotProgram(p)
	if !slices.Equal(before.files, after.files) || !maps.Equal(before.filesByPath, after.filesByPath) || !maps.Equal(before.resolvedModules, after.resolvedModules) {
		t.Fatal("UpdateProgramFiles mutated the old Program")
	}
}

func assertLinearResult(t *testing.T, old *Program, paths []tspath.Path, host *recordingHost, updated *Program, reused bool) {
	t.Helper()
	wantFiles, wantByPath, wantReused := linearReplacement(old, paths, host.recorded())
	if reused != wantReused {
		t.Fatalf("reused = %v, want %v", reused, wantReused)
	}
	if !wantReused {
		if updated == old {
			t.Fatal("fallback returned the old Program")
		}
		return
	}
	if !slices.Equal(updated.files, wantFiles) {
		t.Fatal("updated files differ from the per-replacement linear search")
	}
	if !maps.Equal(updated.filesByPath, wantByPath) {
		t.Fatal("updated filesByPath differs from the per-replacement linear search")
	}
}

// assertRebuilt checks that a fallback Program loaded the requested text for each path.
func assertRebuilt(t *testing.T, updated *Program, reused bool, want map[tspath.Path]string) {
	t.Helper()
	if reused {
		t.Fatal("update reused the file set, want one rebuild")
	}
	for path, text := range want {
		if file := updated.filesByPath[path]; file == nil || file.Text() != text {
			t.Fatalf("rebuilt Program did not load the requested %s", path)
		}
	}
}

func TestUpdateProgramFiles_matchesLinearSearch_forCompatibleTextUpdates(t *testing.T) {
	for _, size := range batchSizes() {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: a real Program and text-only edits to the last size files, in reverse order.
			program := newIndexTestProgram(t, nil)
			before := snapshotProgram(program)
			changed := map[int]string{}
			var paths []tspath.Path
			for i := indexTestFileCount - 1; i >= indexTestFileCount-size; i-- {
				changed[i] = fmt.Sprintf("export const v%d = 2;\n", i)
				paths = append(paths, indexTestPath(i))
			}
			host := newRecordingHost(changed, nil)

			// When: one batched update applies every edit.
			updated, reused := update(t, program, paths, host)

			// Then: every edit replaces its first position and the old Program is untouched.
			if !reused {
				t.Fatal("compatible text updates rebuilt the Program")
			}
			assertLinearResult(t, program, paths, host, updated, reused)
			assertUnchanged(t, program, before)
		})
	}
}

func TestUpdateProgramFiles_matchesLinearSearch_forRepeatedPaths(t *testing.T) {
	for _, size := range batchSizes() {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: a batch that names f0 at its start, middle and end.
			program := newIndexTestProgram(t, nil)
			before := snapshotProgram(program)
			paths := []tspath.Path{indexTestPath(0)}
			for i := 1; len(paths) < size; i++ {
				if len(paths) == size/2 || len(paths) == size-1 {
					paths = append(paths, indexTestPath(0))
					continue
				}
				paths = append(paths, indexTestPath(i))
			}
			host := newRecordingHost(map[int]string{0: "export const v0 = 3;\n"}, nil)

			// When: the batch applies.
			updated, reused := update(t, program, paths, host)

			// Then: the last f0 parse wins at f0's first position.
			assertLinearResult(t, program, paths, host, updated, reused)
			if got, want := updated.filesByPath[indexTestPath(0)], host.recorded()[len(paths)-1]; reused && got != want {
				t.Fatal("repeated path did not keep its last replacement")
			}
			assertUnchanged(t, program, before)
		})
	}
}

func TestUpdateProgramFiles_matchesLinearSearch_forDuplicateFilePositions(t *testing.T) {
	for _, size := range batchSizes() {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: fabricated state where f1 appears twice in the file list.
			program := newIndexTestProgram(t, nil)
			duplicate := program.filesByPath[indexTestPath(1)]
			program.files = append(slices.Clone(program.files), duplicate)
			before := snapshotProgram(program)
			paths := make([]tspath.Path, size)
			for i := range paths {
				paths[i] = indexTestPath((i + 1) % indexTestFileCount)
			}
			host := newRecordingHost(map[int]string{1: "export const v1 = 4;\n"}, nil)

			// When: the batch updates f1.
			updated, reused := update(t, program, paths, host)

			// Then: only the first f1 position changes; the later copy stays.
			assertLinearResult(t, program, paths, host, updated, reused)
			if updated.files[len(updated.files)-1] != duplicate {
				t.Fatal("update replaced the later duplicate position")
			}
			assertUnchanged(t, program, before)
		})
	}
}

func TestUpdateProgramFiles_fallsBack_whenFileListMissesMappedPath(t *testing.T) {
	for _, size := range batchSizes() {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: fabricated state where the last batch path is in filesByPath but not in files.
			program := newIndexTestProgram(t, nil)
			missing := indexTestPath(size - 1)
			program.files = slices.DeleteFunc(slices.Clone(program.files), func(file *ast.SourceFile) bool { return file.Path() == missing })
			before := snapshotProgram(program)
			paths := make([]tspath.Path, size)
			for i := range paths {
				paths[i] = indexTestPath(i)
			}
			want := "export const missing = 6;\n"
			host := newRecordingHost(map[int]string{size - 1: want}, nil)

			// When: the batch reaches the missing path.
			updated, reused := update(t, program, paths, host)

			// Then: the update rebuilds once with the requested text, as the linear search did.
			assertLinearResult(t, program, paths, host, updated, reused)
			assertRebuilt(t, updated, reused, map[tspath.Path]string{missing: want})
			assertUnchanged(t, program, before)
		})
	}
}

// aliasProgram fabricates filesByPath[alias] = the file of target, so an update of alias
// parses a replacement whose path is target, not alias. Real loading does not produce this
// outside redirects (which take the full rebuild); it is the only way to reach the
// path-changing branch through UpdateProgramFiles.
func aliasProgram(t *testing.T, alias, target int) *Program {
	t.Helper()
	program := newIndexTestProgram(t, nil)
	program.filesByPath = maps.Clone(program.filesByPath)
	program.filesByPath[indexTestPath(alias)] = program.filesByPath[indexTestPath(target)]
	return program
}

func paddedPaths(head []tspath.Path, size int, skip ...int) []tspath.Path {
	paths := slices.Clone(head)
	for i := 0; len(paths) < size; i++ {
		if !slices.Contains(skip, i) {
			paths = append(paths, indexTestPath(i))
		}
	}
	return paths
}

func TestUpdateProgramFiles_fallsBack_whenPathChangedThenRequestedAgain(t *testing.T) {
	for _, size := range batchSizes()[1:] {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: A's replacement has path B, and A is requested again later.
			const a, b = 1, 4
			program := aliasProgram(t, a, b)
			before := snapshotProgram(program)
			paths := paddedPaths([]tspath.Path{indexTestPath(a), indexTestPath(a)}, size, a, b)
			want := map[int]string{a: "export const v1 = 7;\n", b: "export const v4 = 7;\n"}
			host := newRecordingHost(want, nil)

			// When: the batch applies.
			updated, reused := update(t, program, paths, host)

			// Then: the second A finds no A position and the update rebuilds once with both texts.
			assertLinearResult(t, program, paths, host, updated, reused)
			assertRebuilt(t, updated, reused, map[tspath.Path]string{indexTestPath(a): want[a], indexTestPath(b): want[b]})
			assertUnchanged(t, program, before)
		})
	}
}

func TestUpdateProgramFiles_matchesLinearSearch_whenPathChangeCreatesEarlierPosition(t *testing.T) {
	for _, size := range batchSizes()[1:] {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: A (earlier) gets a replacement with path B, then B is updated.
			const a, b = 1, 4
			program := aliasProgram(t, a, b)
			before := snapshotProgram(program)
			paths := paddedPaths([]tspath.Path{indexTestPath(a), indexTestPath(b)}, size, a, b)
			host := newRecordingHost(nil, nil)

			// When: the batch applies.
			updated, reused := update(t, program, paths, host)

			// Then: B's update lands at A's old position, and the original B stays in place.
			assertLinearResult(t, program, paths, host, updated, reused)
			if !reused {
				t.Fatal("path-changing batch rebuilt the Program")
			}
			aIndex := slices.IndexFunc(program.files, func(file *ast.SourceFile) bool { return file.Path() == indexTestPath(a) })
			bIndex := slices.IndexFunc(program.files, func(file *ast.SourceFile) bool { return file.Path() == indexTestPath(b) })
			if aIndex < 0 || aIndex > bIndex || updated.files[aIndex] != host.recorded()[1] || updated.files[bIndex] != program.files[bIndex] {
				t.Fatal("B update did not take the earlier B position")
			}
			assertUnchanged(t, program, before)
		})
	}
}

func TestUpdateProgramFiles_matchesLinearSearch_forAlreadyLoadedImport(t *testing.T) {
	for _, size := range batchSizes() {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: f0 gains an import of loaded f1, and other files change only text.
			program := newIndexTestProgram(t, nil)
			before := snapshotProgram(program)
			changed := map[int]string{0: "import { v1 } from \"./f1\";\nexport const v0 = v1;\n"}
			paths := []tspath.Path{indexTestPath(0)}
			for i := 1; len(paths) < size; i++ {
				changed[i] = fmt.Sprintf("export const v%d = 5;\n", i)
				paths = append(paths, indexTestPath(i))
			}
			host := newRecordingHost(changed, nil)

			// When: the batch applies.
			updated, reused := update(t, program, paths, host)

			// Then: the file set is reused and f0 records the resolution to f1.
			if !reused {
				t.Fatal("already-loaded import rebuilt the Program")
			}
			assertLinearResult(t, program, paths, host, updated, reused)
			resolvesF1 := false
			for key, resolved := range updated.resolvedModules[indexTestPath(0)] {
				resolvesF1 = resolvesF1 || key.Name == "./f1" && resolved.IsResolved() && updated.toPath(resolved.ResolvedFileName) == indexTestPath(1)
			}
			if !resolvesF1 {
				t.Fatal("f0 import of f1 did not resolve in the updated Program")
			}
			assertUnchanged(t, program, before)
		})
	}
}

func TestUpdateProgramFiles_fallsBack_whenImportNeedsNewModule(t *testing.T) {
	for _, size := range batchSizes() {
		t.Run(fmt.Sprintf("batch=%d", size), func(t *testing.T) {
			// Given: extra.ts is on disk for both hosts but not loaded, and the last batch file imports it.
			const extraPath = "/project/extra.ts"
			program := newIndexTestProgram(t, map[string]string{extraPath: "export const extra = 1;\n"})
			if program.filesByPath[extraPath] != nil {
				t.Fatal("extra.ts was already part of the Program")
			}
			before := snapshotProgram(program)
			last := size - 1
			importer := "import { extra } from \"./extra\";\nexport const v = extra;\n"
			paths := make([]tspath.Path, size)
			for i := range paths {
				paths[i] = indexTestPath(i)
			}
			extraText := "export const extra = 2;\n"
			host := newRecordingHost(map[int]string{last: importer}, map[string]string{extraPath: extraText})

			// When: the batch applies.
			updated, reused := update(t, program, paths, host)

			// Then: the update rebuilds once and loads the importer and extra.ts from the new host.
			assertRebuilt(t, updated, reused, map[tspath.Path]string{indexTestPath(last): importer, extraPath: extraText})
			for i := range indexTestFileCount {
				if updated.filesByPath[indexTestPath(i)] == nil {
					t.Fatalf("rebuilt Program lost f%d", i)
				}
			}
			assertUnchanged(t, program, before)
		})
	}
}
