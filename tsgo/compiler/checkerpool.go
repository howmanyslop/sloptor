package compiler

import (
	"context"
	"slices"
	"sync"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/core"
	"rotor/tsgo/tracing"
)

// CheckerPool is implemented by the project system to provide checkers with
// request-scoped lifetime and reclamation. It returns a checker and a release
// function that must be called when the caller is done with the checker.
// The returned checker must not be accessed concurrently; each acquisition is exclusive.
// If file is non-nil, the pool may use it as an affinity hint to return the same
// checker for the same file across calls.
type CheckerPool interface {
	GetChecker(ctx context.Context, file *ast.SourceFile) (*checker.Checker, func())
}

type checkerPool struct {
	program *Program
	tracing *tracing.Tracing

	// Each slot is built at most once, on first use. Slot count and the
	// file-to-slot mapping stay fixed, so a file always gets the same checker.
	checkerOnces       []sync.Once
	createCheckersOnce sync.Once
	checkers           []*checker.Checker
	locks              []*sync.Mutex

	fileAssociationsOnce sync.Once
	fileAssociations     map[*ast.SourceFile]int
}

var _ CheckerPool = (*checkerPool)(nil)

func newCheckerPool(program *Program) *checkerPool {
	return newCheckerPoolWithTracing(program, nil)
}

func newCheckerPoolWithTracing(program *Program, tr *tracing.Tracing) *checkerPool {
	checkerCount := 4
	if program.SingleThreaded() {
		checkerCount = 1
	} else if c := program.Options().Checkers; c != nil {
		checkerCount = *c
	}

	checkerCount = max(min(checkerCount, len(program.files), 256), 1)

	pool := &checkerPool{
		program:      program,
		checkerOnces: make([]sync.Once, checkerCount),
		checkers:     make([]*checker.Checker, checkerCount),
		locks:        make([]*sync.Mutex, checkerCount),
		tracing:      tr,
	}

	return pool
}

// GetChecker implements CheckerPool. When file is non-nil, returns the checker
// associated with that file; otherwise returns the first checker.
func (p *checkerPool) GetChecker(ctx context.Context, file *ast.SourceFile) (*checker.Checker, func()) {
	if file != nil {
		return p.getCheckerForFileExclusive(ctx, file)
	}
	c := p.getCheckerAt(0)
	p.locks[0].Lock()
	return c, sync.OnceFunc(func() {
		p.locks[0].Unlock()
	})
}

// getCheckerForFileNonExclusive returns the checker for the given file without locking.
// This is only safe when the caller guarantees no concurrent access to the same checker,
// e.g. for read-only operations like obtaining an emit resolver.
func (p *checkerPool) getCheckerForFileNonExclusive(file *ast.SourceFile) (*checker.Checker, func()) {
	idx, ok := p.getFileAssociations()[file]
	if !ok {
		if p.tracing != nil {
			p.createCheckers()
		}
		return nil, noop
	}
	return p.getCheckerAt(idx), noop
}

func (p *checkerPool) getCheckerForFileExclusive(ctx context.Context, file *ast.SourceFile) (*checker.Checker, func()) {
	idx, ok := p.getFileAssociations()[file]
	if !ok {
		// Unknown files have no slot; index -1 panics as before.
		idx = -1
	}
	c := p.getCheckerAt(idx)
	p.locks[idx].Lock()
	return c, sync.OnceFunc(func() {
		p.locks[idx].Unlock()
	})
}

// getCheckerNonExclusive returns the first checker without locking.
func (p *checkerPool) getCheckerNonExclusive() (*checker.Checker, func()) {
	return p.getCheckerAt(0), noop
}

// getCheckerAt returns the checker in slot idx, building it on first use.
// Under tracing every slot is built up front, so trace output is unchanged.
func (p *checkerPool) getCheckerAt(idx int) *checker.Checker {
	if p.tracing != nil {
		p.createCheckers()
	}
	p.createChecker(idx)
	return p.checkers[idx]
}

func (p *checkerPool) createChecker(idx int) {
	p.checkerOnces[idx].Do(func() {
		var tracer *checker.Tracer
		if p.tracing != nil {
			tracer = checker.NewTracer(p.tracing, idx)
		}
		p.checkers[idx], p.locks[idx] = checker.NewChecker(p.program, tracer)
	})
}

// createCheckers builds every slot not yet built and waits for all of them.
func (p *checkerPool) createCheckers() {
	p.createCheckersOnce.Do(func() {
		wg := core.NewWorkGroup(p.program.SingleThreaded())
		for i := range len(p.checkers) {
			wg.Queue(func() {
				p.createChecker(i)
			})
		}
		wg.RunAndWait()
	})
}

func (p *checkerPool) getFileAssociations() map[*ast.SourceFile]int {
	p.fileAssociationsOnce.Do(func() {
		checkerCount := len(p.checkers)
		p.fileAssociations = make(map[*ast.SourceFile]int, len(p.program.files))
		for i, file := range p.program.files {
			p.fileAssociations[file] = i % checkerCount
		}
	})
	return p.fileAssociations
}

// Runs `cb` for each checker in the pool concurrently, locking and unlocking checker mutexes as it goes,
// making it safe to call `forEachCheckerParallel` from many threads simultaneously.
func (p *checkerPool) forEachCheckerParallel(cb func(idx int, c *checker.Checker)) {
	p.createCheckers()
	wg := core.NewWorkGroup(p.program.SingleThreaded())
	for idx, checker := range p.checkers {
		wg.Queue(func() {
			p.locks[idx].Lock()
			defer p.locks[idx].Unlock()
			cb(idx, checker)
		})
	}
	wg.RunAndWait()
}

func (p *checkerPool) GetGlobalDiagnostics() []*ast.Diagnostic {
	p.createCheckers()
	globalDiagnostics := make([][]*ast.Diagnostic, len(p.checkers))
	p.forEachCheckerParallel(func(idx int, checker *checker.Checker) {
		globalDiagnostics[idx] = checker.GetGlobalDiagnostics()
	})
	return SortAndDeduplicateDiagnostics(slices.Concat(globalDiagnostics...))
}

// forEachCheckerGroupDo runs one task per checker in parallel. Each task iterates
// the provided files, processing only those assigned to its checker. Within each
// checker's set, files are visited in their original order.
func (p *checkerPool) forEachCheckerGroupDo(ctx context.Context, files []*ast.SourceFile, singleThreaded bool, cb func(c *checker.Checker, fileIndex int, file *ast.SourceFile)) {
	p.createCheckers()
	fileAssociations := p.getFileAssociations()

	checkerCount := len(p.checkers)
	wg := core.NewWorkGroup(singleThreaded)
	for checkerIdx := range checkerCount {
		wg.Queue(func() {
			p.locks[checkerIdx].Lock()
			defer p.locks[checkerIdx].Unlock()
			for i, file := range files {
				if idx, ok := fileAssociations[file]; ok && idx == checkerIdx {
					cb(p.checkers[checkerIdx], i, file)
				}
			}
		})
	}
	wg.RunAndWait()
}

func noop() {}
