package compile

import (
	"context"
	"errors"
	"sync"

	"rotor/internal/logservice"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
)

// declarationPrefetch emits a project's declaration text during the
// transformer sidecar round trip, which otherwise leaves the build idle while
// Node runs the plugins.
//
// Declarations are emitted off the original program, and nothing else uses it
// during that wait. The emit must finish before the native Flamework transform
// starts: transformSourceFile re-parents the original program's nodes in place
// while it works, and the checker would read them mid-change.
// prefetchingTransformerHost guarantees that by waiting for the emit before the
// round trip returns.
type declarationPrefetch struct {
	program *compiler.Program
	files   []*ast.SourceFile
	timings *BuildTimings

	once    sync.Once
	ran     bool
	emitted []declarationEmitFile
	err     error
}

func newDeclarationPrefetch(program *compiler.Program, selectedPaths map[string]struct{}, timings *BuildTimings) *declarationPrefetch {
	if !program.Options().GetEmitDeclarations() {
		return nil
	}
	files := declarationEmitSourceFiles(program, selectedPaths)
	if len(files) == 0 {
		return nil
	}
	return &declarationPrefetch{program: program, files: files, timings: timings}
}

// run emits the declarations once. Closing abandon stops it between files and
// leaves no result, so the build emits after the transform as usual.
func (prefetch *declarationPrefetch) run(abandon <-chan struct{}) {
	prefetch.once.Do(func() {
		var emitted []declarationEmitFile
		var err error
		stopEmit := prefetch.timings.startStage(declarationEmitStage)
		logservice.BenchmarkIfVerbose("declaration emit", func() {
			emitted, err = emitDeclarationTextsUntil(prefetch.program, prefetch.files, abandon)
		})
		stopEmit()
		if errors.Is(err, errDeclarationEmitAbandoned) {
			return
		}
		prefetch.emitted, prefetch.err, prefetch.ran = emitted, err, true
	})
}

// result returns the prefetched text, and false when no round trip produced
// it.
func (prefetch *declarationPrefetch) result() ([]declarationEmitFile, bool, error) {
	if prefetch == nil || !prefetch.ran {
		return nil, false, nil
	}
	return prefetch.emitted, true, prefetch.err
}

// prefetchingTransformerHost runs its declarationPrefetch alongside the first
// transform round trip and waits for it before returning. A failed round trip
// fails the build, so it abandons the emit rather than wait it out.
type prefetchingTransformerHost struct {
	transformerHost
	prefetch *declarationPrefetch
}

func (host prefetchingTransformerHost) transform(ctx context.Context, call transformerCall) (*sidecarResponse, sidecarCallStats, error) {
	abandon := make(chan struct{})
	done := make(chan struct{})
	go func() {
		defer close(done)
		host.prefetch.run(abandon)
	}()
	response, stats, err := host.transformerHost.transform(ctx, call)
	if err != nil {
		close(abandon)
	}
	<-done
	return response, stats, err
}
