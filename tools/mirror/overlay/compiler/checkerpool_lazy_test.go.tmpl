package compiler

import (
	"context"
	"fmt"
	"maps"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/core"
	"rotor/tsgo/json"
	"rotor/tsgo/tracing"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/vfstest"
)

const lazyPoolFileCount = 8

// newLazyPoolProgram builds a Program over files that each hold one type error.
func newLazyPoolProgram(t *testing.T, tr *tracing.Tracing) *Program {
	t.Helper()
	files := make(map[string]string, lazyPoolFileCount)
	roots := make([]string, 0, lazyPoolFileCount)
	for i := range lazyPoolFileCount {
		name := fmt.Sprintf("/project/f%d.ts", i)
		files[name] = fmt.Sprintf("export const v%d: string = %d;\n", i, i)
		roots = append(roots, name)
	}
	program := newPoolProgram(t, files, roots, tr)
	if len(program.files) != lazyPoolFileCount {
		t.Fatalf("test program has %d files, want %d", len(program.files), lazyPoolFileCount)
	}
	return program
}

// newPoolProgram builds a 4-checker Program over the given files. noLib keeps
// the test independent of the bundled libs and makes every checker report the
// same missing-global diagnostics.
func newPoolProgram(t *testing.T, files map[string]string, roots []string, tr *tracing.Tracing) *Program {
	t.Helper()
	checkers := 4
	options := &core.CompilerOptions{NoLib: core.TSTrue, Checkers: &checkers}
	config := tsoptions.NewParsedCommandLine(options, roots, tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true, CurrentDirectory: "/project"})
	program := NewProgram(ProgramOptions{
		Host:           NewCompilerHost("/project", vfstest.FromMap(files, true), "", nil, nil),
		Config:         config,
		SingleThreaded: core.TSFalse,
		Tracing:        tr,
	})
	if pool := program.compilerCheckerPool; pool == nil || len(pool.checkers) != 4 {
		t.Fatal("test program does not use a 4-slot compiler pool")
	}
	return program
}

// builtSlots reports which slots hold a checker. Callers read it only after
// every pool call has returned.
func builtSlots(pool *checkerPool) []int {
	var built []int
	for i, c := range pool.checkers {
		if c != nil {
			built = append(built, i)
		}
	}
	return built
}

// diagnosticKeys renders every diagnostic field, including message chains and
// related information, in the given order.
func diagnosticKeys(diags []*ast.Diagnostic) []string {
	keys := make([]string, len(diags))
	for i, d := range diags {
		fileName := ""
		if d.File() != nil {
			fileName = d.File().FileName()
		}
		stringCode, hasStringCode := d.StringCode()
		keys[i] = fmt.Sprintf("{%s [%d,%d) code=%d category=%v key=%v args=%q stringCode=%q/%v text=%q unnecessary=%v deprecated=%v skippedOnNoEmit=%v repopulate=%+v chain=%v related=%v}",
			fileName, d.Pos(), d.End(), d.Code(), d.Category(), d.MessageKey(), d.MessageArgs(), stringCode, hasStringCode, d.String(),
			d.ReportsUnnecessary(), d.ReportsDeprecated(), d.SkippedOnNoEmit(), d.RepopulateInfo(),
			diagnosticKeys(d.MessageChain()), diagnosticKeys(d.RelatedInformation()))
	}
	return keys
}

func TestCheckerPoolBuildsOnlySlotZeroForUnscopedUse(t *testing.T) {
	// Given: a fresh 4-slot pool.
	program := newLazyPoolProgram(t, nil)
	pool := program.compilerCheckerPool
	if built := builtSlots(pool); len(built) != 0 {
		t.Fatalf("slots built before first use: %v", built)
	}

	// When: callers ask for the program checker repeatedly.
	first, done := program.GetTypeChecker(context.Background())
	done()
	again, done := program.GetTypeChecker(context.Background())
	done()
	leased, release := pool.GetChecker(context.Background(), nil)
	release()
	release()

	// Then: only slot 0 exists and every call returns it.
	if first == nil || again != first || leased != first || pool.checkers[0] != first {
		t.Fatal("unscoped calls did not return the one slot-0 checker")
	}
	if built := builtSlots(pool); !slices.Equal(built, []int{0}) {
		t.Fatalf("built slots = %v, want [0]", built)
	}
}

func TestCheckerPoolKeepsFixedFileAffinityAfterPartialBuild(t *testing.T) {
	// Given: a pool where only the slot of file 1 exists.
	program := newLazyPoolProgram(t, nil)
	pool := program.compilerCheckerPool
	ctx := context.Background()
	slotOne, done := program.GetTypeCheckerForFileExclusive(ctx, program.files[1])
	done()
	if built := builtSlots(pool); !slices.Equal(built, []int{1}) {
		t.Fatalf("built slots = %v, want [1]", built)
	}

	// When: every file is then asked for, exclusively and not.
	for i, file := range program.files {
		exclusive, done := program.GetTypeCheckerForFileExclusive(ctx, file)
		done()
		shared, _ := program.GetTypeCheckerForFile(ctx, file)

		// Then: file i always maps to slot i%4 and slot 1 keeps its first checker.
		if exclusive == nil || exclusive != shared || exclusive != pool.checkers[i%4] {
			t.Fatalf("file %d did not map to slot %d", i, i%4)
		}
	}
	if pool.checkers[1] != slotOne {
		t.Fatal("slot 1 was replaced")
	}
	if built := builtSlots(pool); !slices.Equal(built, []int{0, 1, 2, 3}) {
		t.Fatalf("built slots = %v, want all", built)
	}
}

func TestCheckerPoolConcurrentFirstUseBuildsOnceAndExcludes(t *testing.T) {
	// Given: a fresh pool and many goroutines aimed at slots 1 and 2.
	program := newLazyPoolProgram(t, nil)
	pool := program.compilerCheckerPool
	ctx := context.Background()
	targets := []*ast.SourceFile{program.files[1], program.files[5], program.files[2], program.files[6]}
	var active [4]atomic.Int32
	var seen [4]sync.Map
	var overlap atomic.Bool
	var start, wg sync.WaitGroup
	start.Add(1)

	// When: they all acquire at once.
	for g := range 32 {
		file := targets[g%len(targets)]
		slot := slices.Index(program.files, file) % 4
		wg.Add(1)
		go func() {
			defer wg.Done()
			start.Wait()
			c, done := program.GetTypeCheckerForFileExclusive(ctx, file)
			defer done()
			if active[slot].Add(1) != 1 {
				overlap.Store(true)
			}
			seen[slot].Store(c, true)
			active[slot].Add(-1)
		}()
	}
	start.Done()
	wg.Wait()

	// Then: each slot was built once, leases never overlapped, and slots 0 and 3 stay unbuilt.
	if overlap.Load() {
		t.Fatal("two goroutines held the same slot at once")
	}
	for _, slot := range []int{1, 2} {
		count := 0
		seen[slot].Range(func(key, _ any) bool {
			count++
			if key.(*checker.Checker) != pool.checkers[slot] {
				t.Errorf("slot %d returned a checker it does not hold", slot)
			}
			return true
		})
		if count != 1 {
			t.Errorf("slot %d returned %d distinct checkers, want 1", slot, count)
		}
	}
	if built := builtSlots(pool); !slices.Equal(built, []int{1, 2}) {
		t.Fatalf("built slots = %v, want [1 2]", built)
	}
}

func TestCheckerPoolWholePoolCallsBuildEverySlot(t *testing.T) {
	ctx := context.Background()
	calls := map[string]func(*testing.T, *Program){
		"ForEachCheckerParallel": func(t *testing.T, p *Program) {
			var mu sync.Mutex
			var indexes []int
			p.ForEachCheckerParallel(func(idx int, c *checker.Checker) {
				if c == nil {
					t.Errorf("callback %d got a nil checker", idx)
				}
				mu.Lock()
				indexes = append(indexes, idx)
				mu.Unlock()
			})
			slices.Sort(indexes)
			if !slices.Equal(indexes, []int{0, 1, 2, 3}) {
				t.Errorf("callback indexes = %v, want every slot", indexes)
			}
		},
		"GetGlobalDiagnostics": func(t *testing.T, p *Program) {
			if len(p.GetGlobalDiagnostics(ctx)) == 0 {
				t.Error("no global diagnostics without a lib")
			}
		},
		"GetSemanticDiagnostics": func(t *testing.T, p *Program) {
			if len(p.GetSemanticDiagnostics(ctx, nil)) == 0 {
				t.Error("no semantic diagnostics for files with type errors")
			}
		},
		"TypeCount": func(_ *testing.T, p *Program) {
			p.TypeCount()
		},
	}
	for name, call := range calls {
		t.Run(name, func(t *testing.T) {
			// Given: a pool where only slot 0 exists.
			program := newLazyPoolProgram(t, nil)
			pool := program.compilerCheckerPool
			slotZero, _ := program.GetTypeChecker(ctx)

			// When: a whole-pool call runs.
			call(t, program)

			// Then: every slot exists and slot 0 is unchanged.
			if built := builtSlots(pool); !slices.Equal(built, []int{0, 1, 2, 3}) {
				t.Fatalf("built slots = %v, want all", built)
			}
			if pool.checkers[0] != slotZero {
				t.Fatal("slot 0 was replaced")
			}
		})
	}
}

func TestCheckerPoolPartialThenFullMatchesEagerResults(t *testing.T) {
	ctx := context.Background()

	// Given: one program used for full checking at once, and one first used only through slot 0.
	eager := newLazyPoolProgram(t, nil)
	eager.compilerCheckerPool.createCheckers()
	partial := newLazyPoolProgram(t, nil)
	c, done := partial.GetTypeChecker(ctx)
	if len(c.GetDiagnostics(ctx, partial.files[0])) == 0 {
		t.Fatal("slot 0 found no error in file 0")
	}
	done()

	// When: both run the full semantic and global checks.
	wantSemantic := diagnosticKeys(eager.GetSemanticDiagnostics(ctx, nil))
	gotSemantic := diagnosticKeys(partial.GetSemanticDiagnostics(ctx, nil))
	wantGlobal := diagnosticKeys(eager.GetGlobalDiagnostics(ctx))
	gotGlobal := diagnosticKeys(partial.GetGlobalDiagnostics(ctx))

	// Then: the results and their order are the same.
	if !slices.Equal(gotSemantic, wantSemantic) {
		t.Fatalf("semantic diagnostics differ:\n got %v\nwant %v", gotSemantic, wantSemantic)
	}
	if !slices.Equal(gotGlobal, wantGlobal) {
		t.Fatalf("global diagnostics differ:\n got %v\nwant %v", gotGlobal, wantGlobal)
	}
}

func TestCheckerPoolCancelledGlobalDiagnosticsLeavesLateSlotsUsable(t *testing.T) {
	// Given: a pool where only slot 0 exists, and a cancelled context.
	program := newLazyPoolProgram(t, nil)
	program.GetTypeChecker(context.Background())
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	// When: global diagnostics run under the cancelled context.
	program.GetGlobalDiagnostics(ctx)

	// Then: the slots built late are not cancelled and still check files.
	program.ForEachCheckerParallel(func(idx int, c *checker.Checker) {
		if c.WasCanceled() {
			t.Errorf("slot %d is marked cancelled", idx)
		}
	})
	live := context.Background()
	c, done := program.GetTypeCheckerForFileExclusive(live, program.files[3])
	defer done()
	if len(c.GetDiagnostics(live, program.files[3])) == 0 {
		t.Fatal("slot 3 found no error in file 3")
	}
}

func TestCheckerPoolBuildsEverySlotAtFirstUseUnderTracing(t *testing.T) {
	// Given: a traced program on a virtual file system.
	fs := vfstest.FromMap(map[string]string{}, true)
	tr, err := tracing.StartTracing(fs, "/trace", "", true)
	if err != nil {
		t.Fatal(err)
	}
	stopped := false
	t.Cleanup(func() {
		if !stopped {
			_ = tr.StopTracing()
		}
	})
	program := newLazyPoolProgram(t, tr)
	pool := program.compilerCheckerPool

	// When: only the slot-0 checker is asked for, then tracing stops.
	program.GetTypeChecker(context.Background())
	built := builtSlots(pool)
	stopped = true
	if err := tr.StopTracing(); err != nil {
		t.Fatalf("StopTracing: %v", err)
	}

	// Then: every slot was built, as before lazy slots, and the trace has a
	// legend entry and a types file for each one.
	if !slices.Equal(built, []int{0, 1, 2, 3}) {
		t.Fatalf("built slots = %v, want all", built)
	}
	legendText, ok := fs.ReadFile("/trace/legend.json")
	if !ok {
		t.Fatal("legend.json missing")
	}
	var legend []tracing.TraceRecord
	if err := json.Unmarshal([]byte(legendText), &legend); err != nil {
		t.Fatalf("legend.json: %v", err)
	}
	var ids []int
	for _, record := range legend {
		ids = append(ids, record.CheckerID)
		if want := fmt.Sprintf("/trace/types_%d.json", record.CheckerID); record.TypesPath != want {
			t.Errorf("checker %d types path = %q, want %q", record.CheckerID, record.TypesPath, want)
		}
		types, ok := fs.ReadFile(record.TypesPath)
		if !ok || !strings.HasPrefix(types, "[") {
			t.Errorf("checker %d types file missing or not a JSON array", record.CheckerID)
		}
	}
	if !slices.Equal(ids, []int{0, 1, 2, 3}) {
		t.Fatalf("legend checker ids = %v, want [0 1 2 3]", ids)
	}
}

func TestCheckerPoolUnknownFileKeepsBehavior(t *testing.T) {
	// Given: a file that belongs to another program.
	program := newLazyPoolProgram(t, nil)
	foreign := newLazyPoolProgram(t, nil).files[0]
	pool := program.compilerCheckerPool

	// When/Then: the shared lookup returns nil and the exclusive lookup panics.
	if c, _ := pool.getCheckerForFileNonExclusive(foreign); c != nil {
		t.Fatal("unknown file got a checker")
	}
	defer func() {
		if recover() == nil {
			t.Fatal("exclusive lookup of an unknown file did not panic")
		}
	}()
	pool.getCheckerForFileExclusive(context.Background(), foreign)
}

// mergedProgramFiles declares one global interface in two files, adds to it
// through a global augmentation, and adds to a module interface through a
// module augmentation. use.ts reads all of it and holds two errors.
var mergedProgramFiles = map[string]string{
	"/project/globals.d.ts":  "interface Shared { a: string; }\n",
	"/project/globals2.d.ts": "interface Shared { b: number; }\ndeclare var sharedValue: Shared;\n",
	"/project/mod.ts":        "export interface Box { value: string; }\nexport declare const box: Box;\n",
	"/project/augment.ts":    "export {};\ndeclare module \"./mod\" { interface Box { extra: number; } }\ndeclare global { interface Shared { c: boolean; } }\n",
	"/project/use.ts": `import { box } from "./mod";
export const merged = sharedValue;
export const extra = box.extra;
export const flag = merged.c;
export const plain = merged.b;
export const wrong: { a: number } = merged;
export const missing = merged.nope;
`,
}

var mergedProgramRoots = []string{"/project/globals.d.ts", "/project/globals2.d.ts", "/project/mod.ts", "/project/augment.ts", "/project/use.ts"}

// exportedConstTypes returns the type text of each top-level const in file.
func exportedConstTypes(c *checker.Checker, file *ast.SourceFile) map[string]string {
	types := make(map[string]string)
	for _, statement := range file.Statements.Nodes {
		if !ast.IsVariableStatement(statement) {
			continue
		}
		for _, declaration := range statement.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes {
			name := declaration.Name()
			types[name.Text()] = c.TypeToString(c.GetTypeAtLocation(name))
		}
	}
	return types
}

func TestCheckerPoolLateSlotsBuildWhileSlotZeroChecks(t *testing.T) {
	ctx := context.Background()

	// Given: the expected results from a program whose slots were all built first.
	eager := newPoolProgram(t, mergedProgramFiles, mergedProgramRoots, nil)
	eager.compilerCheckerPool.createCheckers()
	eagerUse := eager.GetSourceFile("/project/use.ts")
	eagerChecker, done := eager.GetTypeCheckerForFileExclusive(ctx, eagerUse)
	wantFile := diagnosticKeys(eagerChecker.GetDiagnostics(ctx, eagerUse))
	wantTypes := exportedConstTypes(eagerChecker, eagerUse)
	done()
	wantSemantic := diagnosticKeys(eager.GetSemanticDiagnostics(ctx, nil))
	wantGlobal := diagnosticKeys(eager.GetGlobalDiagnostics(ctx))

	// Given: a second program with slot 0 leased for use.ts.
	program := newPoolProgram(t, mergedProgramFiles, mergedProgramRoots, nil)
	pool := program.compilerCheckerPool
	use := program.GetSourceFile("/project/use.ts")
	if slot := slices.Index(program.files, use) % 4; slot != 0 {
		t.Fatalf("use.ts maps to slot %d, want 0", slot)
	}
	slotZero, release := program.GetTypeCheckerForFileExclusive(ctx, use)

	// When: slot 0 checks use.ts while another goroutine leases slots 1-3,
	// which builds them under lazy slots.
	var gotFile []string
	var gotTypes map[string]string
	var codes []int32
	start := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		<-start
		diags := slotZero.GetDiagnostics(ctx, use)
		for _, d := range diags {
			codes = append(codes, d.Code())
		}
		gotFile = diagnosticKeys(diags)
		gotTypes = exportedConstTypes(slotZero, use)
	}()
	go func() {
		defer wg.Done()
		<-start
		for _, file := range program.files[1:4] {
			_, done := program.GetTypeCheckerForFileExclusive(ctx, file)
			done()
		}
	}()
	close(start)
	wg.Wait()
	release()

	// Then: every slot exists and slot 0 saw the merged declarations.
	if built := builtSlots(pool); !slices.Equal(built, []int{0, 1, 2, 3}) {
		t.Fatalf("built slots = %v, want all", built)
	}
	for name, want := range map[string]string{"merged": "Shared", "extra": "number", "flag": "boolean", "plain": "number"} {
		if gotTypes[name] != want {
			t.Errorf("type of %s = %q, want %q", name, gotTypes[name], want)
		}
	}
	if !maps.Equal(gotTypes, wantTypes) {
		t.Errorf("types differ from eager:\n got %v\nwant %v", gotTypes, wantTypes)
	}
	if !slices.Contains(codes, 2322) || !slices.Contains(codes, 2339) {
		t.Errorf("use.ts error codes = %v, want 2322 and 2339", codes)
	}

	// Then: file, whole-program and global diagnostics match the eager program in full and in order.
	if !slices.Equal(gotFile, wantFile) {
		t.Fatalf("use.ts diagnostics differ:\n got %v\nwant %v", gotFile, wantFile)
	}
	if got := diagnosticKeys(program.GetSemanticDiagnostics(ctx, nil)); !slices.Equal(got, wantSemantic) {
		t.Fatalf("semantic diagnostics differ:\n got %v\nwant %v", got, wantSemantic)
	}
	if got := diagnosticKeys(program.GetGlobalDiagnostics(ctx)); !slices.Equal(got, wantGlobal) {
		t.Fatalf("global diagnostics differ:\n got %v\nwant %v", got, wantGlobal)
	}
}
