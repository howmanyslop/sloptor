package compiler

import (
	"context"
	"slices"
	"sync"
	"sync/atomic"

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

	createCheckersOnce sync.Once
	checkers           []*checker.Checker
	locks              []*sync.Mutex
	fileAssociations   map[*ast.SourceFile]*atomic.Pointer[checker.Checker]
}

var _ CheckerPool = (*checkerPool)(nil)

func newCheckerPool(program *Program) *checkerPool {
	return newCheckerPoolWithTracing(program, nil)
}

func newCheckerPoolWithTracing(program *Program, tr *tracing.Tracing) *checkerPool {
	checkerCount := 4
	if program.SingleThreaded() {
		checkerCount = 1
	} else if program.opts.CheckerCount > 0 {
		checkerCount = program.opts.CheckerCount
	} else if c := program.Options().Checkers; c != nil {
		checkerCount = *c
	}

	checkerCount = max(min(checkerCount, len(program.files), 256), 1)

	pool := &checkerPool{
		program:  program,
		checkers: make([]*checker.Checker, checkerCount),
		locks:    make([]*sync.Mutex, checkerCount),
		tracing:  tr,
	}

	return pool
}

// GetChecker implements CheckerPool. When file is non-nil, returns the checker
// associated with that file; otherwise returns the first checker.
func (p *checkerPool) GetChecker(ctx context.Context, file *ast.SourceFile) (*checker.Checker, func()) {
	if file != nil {
		return p.getCheckerForFileExclusive(ctx, file)
	}
	p.createCheckers()
	c := p.checkers[0]
	p.locks[0].Lock()
	return c, sync.OnceFunc(func() {
		p.locks[0].Unlock()
	})
}

// getCheckerForFileNonExclusive returns the checker for the given file without locking.
// This is only safe when the caller guarantees no concurrent access to the same checker,
// e.g. for read-only operations like obtaining an emit resolver.
func (p *checkerPool) getCheckerForFileNonExclusive(file *ast.SourceFile) (*checker.Checker, func()) {
	p.createCheckers()
	return p.associatedChecker(file), noop
}

// associatedChecker is the checker tasked with checking file, or nil for a
// file outside the program.
func (p *checkerPool) associatedChecker(file *ast.SourceFile) *checker.Checker {
	if association := p.fileAssociations[file]; association != nil {
		return association.Load()
	}
	return nil
}

func (p *checkerPool) getCheckerForFileExclusive(ctx context.Context, file *ast.SourceFile) (*checker.Checker, func()) {
	p.createCheckers()
	c := p.associatedChecker(file)
	idx := slices.Index(p.checkers, c)
	p.locks[idx].Lock()
	return c, sync.OnceFunc(func() {
		p.locks[idx].Unlock()
	})
}

// getCheckerNonExclusive returns the first checker without locking.
func (p *checkerPool) getCheckerNonExclusive() (*checker.Checker, func()) {
	p.createCheckers()
	return p.checkers[0], noop
}

func (p *checkerPool) createCheckers() {
	p.createCheckersOnce.Do(func() {
		checkerCount := len(p.checkers)
		wg := core.NewWorkGroup(p.program.SingleThreaded())
		for i := range checkerCount {
			wg.Queue(func() {
				var tracer *checker.Tracer
				if p.tracing != nil {
					tracer = checker.NewTracer(p.tracing, i)
				}
				p.checkers[i], p.locks[i] = checker.NewChecker(p.program, tracer)
			})
		}

		wg.RunAndWait()

		p.fileAssociations = make(map[*ast.SourceFile]*atomic.Pointer[checker.Checker], len(p.program.files))
		for i, file := range p.program.files {
			association := &atomic.Pointer[checker.Checker]{}
			association.Store(p.checkers[i%checkerCount])
			p.fileAssociations[file] = association
		}
	})
}

// forEachFileOnFreeChecker runs cb for every file, largest first, on whichever
// checker finishes its previous file first, and leaves each file associated
// with the checker that took it, so later requests for the file reuse the work
// that checker did. cb runs without the checker's lock, which it may take.
func (p *checkerPool) forEachFileOnFreeChecker(ctx context.Context, files []*ast.SourceFile, cb func(file *ast.SourceFile)) {
	p.createCheckers()
	queue := slices.Clone(files)
	slices.SortStableFunc(queue, func(a, b *ast.SourceFile) int {
		return len(b.Text()) - len(a.Text())
	})
	var next atomic.Int64
	wg := core.NewWorkGroup(p.program.SingleThreaded())
	for _, c := range p.checkers {
		wg.Queue(func() {
			for ctx.Err() == nil {
				position := int(next.Add(1)) - 1
				if position >= len(queue) {
					return
				}
				file := queue[position]
				if association := p.fileAssociations[file]; association != nil {
					association.Store(c)
				}
				cb(file)
			}
		})
	}
	wg.RunAndWait()
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

	checkerCount := len(p.checkers)
	wg := core.NewWorkGroup(singleThreaded)
	for checkerIdx := range checkerCount {
		wg.Queue(func() {
			p.locks[checkerIdx].Lock()
			defer p.locks[checkerIdx].Unlock()
			for i, file := range files {
				if checker := p.checkers[checkerIdx]; checker == p.associatedChecker(file) {
					cb(checker, i, file)
				}
			}
		})
	}
	wg.RunAndWait()
}

func noop() {}
