package flamework

import (
	"fmt"
	"os"
	"slices"
	"sync"
	"sync/atomic"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/printer"
)

// parallelTransformMinFiles keeps small projects on the serial loop, where a
// worker's checker would spend more re-deriving library types than its share
// of the files saves.
const parallelTransformMinFiles = 256

// parallelTransformEnv set to "0" forces the serial per-file loop; "1" takes
// the parallel loop whenever it is otherwise eligible, however few the files.
const parallelTransformEnv = "ROTOR_FLAMEWORK_PARALLEL"

type recordedOperationKind int

const (
	recordedIdentifier recordedOperationKind = iota
	recordedStringHash
	recordedGlob
)

// recordedOperation is one build-info write a file's transform asked for.
type recordedOperation struct {
	kind  recordedOperationKind
	key   string
	value string
}

// sharedHashes hands every worker the same value for a string hash first
// requested during the parallel phase, as one serial transform would.
type sharedHashes struct {
	mu     sync.Mutex
	values map[string]string
}

// transformRecording is a parallel worker's half of the build info: reads see
// the build info as it stood before the loop plus this worker's own requests,
// and every write is recorded for replay in input-file order, so
// flamework.build is written in the order a serial transform would write it.
type transformRecording struct {
	hashes      *sharedHashes
	identifiers map[string]string
	operations  []recordedOperation
}

// identifier is Project.Identifier for a worker. It is only reached in full ID
// mode, where an identifier is a function of its declaration alone, so two
// workers that both meet a new declaration agree on its identifier and the
// replay keeps the first file's request.
func (r *transformRecording) identifier(p *Project, declaration DeclarationIdentity) (string, error) {
	if existing, ok := p.buildInfo.Identifier(declaration.InternalID); ok {
		return existing, nil
	}
	if existing, ok := r.identifiers[declaration.InternalID]; ok {
		return existing, nil
	}
	id, err := GenerateIdentifier(IdentifierRequest{
		Mode:            p.idMode,
		Salt:            p.config.Salt,
		HashPrefix:      p.hashPrefix,
		PackageName:     p.packageName,
		InternalID:      declaration.InternalID,
		DeclarationName: declaration.DeclarationName,
		LuaFileName:     declaration.LuaFileName,
		PackagePrefix:   declaration.PackagePrefix,
		NextID:          uint64(p.buildInfo.LatestID()),
		IsGame:          p.isGame,
		IsPackage:       declaration.IsPackage,
	})
	if err != nil {
		return "", fmt.Errorf("generate identifier %q: %w", declaration.InternalID, err)
	}
	if declaration.IsPackage {
		return id, nil
	}
	r.identifiers[declaration.InternalID] = id
	r.operations = append(r.operations, recordedOperation{kind: recordedIdentifier, key: declaration.InternalID, value: id})
	return id, nil
}

func (r *transformRecording) hashString(p *Project, text, context string) (string, error) {
	key := context + ":" + text
	if value, ok := p.buildInfo.StringHash(key); ok {
		return value, nil
	}
	r.hashes.mu.Lock()
	value, ok := r.hashes.values[key]
	if !ok {
		var err error
		value, err = NewUUIDv4()
		if err != nil {
			r.hashes.mu.Unlock()
			return "", fmt.Errorf("flamework: hash string: %w", err)
		}
		r.hashes.values[key] = value
	}
	r.hashes.mu.Unlock()
	r.operations = append(r.operations, recordedOperation{kind: recordedStringHash, key: key, value: value})
	return value, nil
}

func (r *transformRecording) addGlob(pattern, origin string) {
	r.operations = append(r.operations, recordedOperation{kind: recordedGlob, key: pattern, value: origin})
}

// replay applies one file's recorded writes to the real build info, skipping
// what an earlier file already wrote, exactly as the serial loop would have.
func replayRecordedOperations(buildInfo *BuildInfo, operations []recordedOperation) error {
	for _, operation := range operations {
		switch operation.kind {
		case recordedIdentifier:
			if _, ok := buildInfo.Identifier(operation.key); ok {
				continue
			}
			if err := buildInfo.AddIdentifier(operation.key, operation.value); err != nil {
				return err
			}
		case recordedStringHash:
			if _, ok := buildInfo.StringHash(operation.key); !ok {
				buildInfo.SetStringHash(operation.key, operation.value)
			}
		case recordedGlob:
			buildInfo.AddGlob(operation.key, operation.value)
		}
	}
	return nil
}

// parallelTransformEligible reports whether the per-file loop may run across
// the program's checkers. Each condition removes a way the serial loop's
// output depends on visit order: only full ID mode names identifiers without
// a running counter, and a caller-supplied macro runtime may keep state.
func parallelTransformEligible(input TransformInput, state *TransformState) bool {
	switch os.Getenv(parallelTransformEnv) {
	case "0":
		return false
	case "1":
	default:
		if len(input.Files) < parallelTransformMinFiles {
			return false
		}
	}
	return input.MacroRuntime == nil && input.Project.idMode == IDModeFull && !state.program.SingleThreaded()
}

// newWorkerState is a TransformState for one parallel worker: the analysis
// results stay shared and read-only, everything a file's visit writes is the
// worker's own.
func newWorkerState(state *TransformState, c *checker.Checker, recording *transformRecording) *TransformState {
	worker := *state
	emitContext := printer.NewEmitContext()
	project := *state.project
	project.recording = recording
	worker.checker = c
	worker.project = &project
	worker.emitContext = emitContext
	worker.factory = emitContext.Factory.AsNodeFactory()
	worker.diagnostics = nil
	worker.generatedNames = make(map[string]map[string]int)
	worker.guardLibrary = ""
	worker.surfaces = nil
	worker.expressionSurfaces = nil
	worker.expressionChildren = nil
	return &worker
}

type parallelFileResult struct {
	transformed *ast.SourceFile
	state       *TransformState
	metadata    SourceMetadata
	operations  []recordedOperation
}

// transformFilesParallel runs the per-file loop with the files that hold no
// planned class spread across the program's checkers, then the planned files
// serially: a class transform re-parents the shared original tree, which no
// other checker may walk meanwhile. ok is false when anything failed; the
// caller then runs the serial loop from scratch, which reports the same
// error the serial loop always has, and nothing recorded here was applied.
func transformFilesParallel(state *TransformState, input TransformInput) (results []parallelFileResult, ok bool) {
	results = make([]parallelFileResult, len(input.Files))
	var unplanned, planned []int
	for index, sourceFile := range input.Files {
		_, isPlanned, err := state.planForFile(sourceFile)
		if err != nil {
			return nil, false
		}
		if isPlanned {
			planned = append(planned, index)
		} else {
			unplanned = append(unplanned, index)
		}
	}

	hashes := &sharedHashes{values: map[string]string{}}
	var groups [][]int
	var mu sync.Mutex
	state.program.ForEachCheckerParallel(func(index int, _ *checker.Checker) {
		mu.Lock()
		for len(groups) <= index {
			groups = append(groups, nil)
		}
		mu.Unlock()
	})
	if len(groups) < 2 {
		return nil, false
	}
	// Workers pull files from one queue, largest first, so a few heavy files
	// cannot leave one checker running long after the rest. Which checker
	// visits a file does not change its result.
	queue := slices.Clone(unplanned)
	slices.SortStableFunc(queue, func(a, b int) int {
		return len(input.Files[b].Text()) - len(input.Files[a].Text())
	})
	var next atomic.Int64
	var aborted atomic.Bool
	failed := make([]bool, len(groups))
	state.program.ForEachCheckerParallel(func(group int, c *checker.Checker) {
		recording := &transformRecording{hashes: hashes, identifiers: map[string]string{}}
		worker := newWorkerState(state, c, recording)
		for {
			position := int(next.Add(1)) - 1
			if position >= len(queue) || aborted.Load() {
				return
			}
			index := queue[position]
			recording.operations = nil
			transformed, err := transformSourceFile(worker, input.Files[index])
			if err != nil {
				failed[group] = true
				aborted.Store(true)
				return
			}
			metadata, err := printSource(input, newSourceMetadata(input.Files[index], transformed, worker.EmitContext()))
			if err != nil {
				failed[group] = true
				aborted.Store(true)
				return
			}
			results[index] = parallelFileResult{transformed: transformed, state: worker, metadata: metadata, operations: recording.operations}
		}
	})
	for _, groupFailed := range failed {
		if groupFailed {
			return nil, false
		}
	}

	recording := &transformRecording{hashes: hashes, identifiers: map[string]string{}}
	serial := newWorkerState(state, state.checker, recording)
	serial.emitContext, serial.factory = state.emitContext, state.factory
	for _, index := range planned {
		recording.operations = nil
		transformed, err := transformSourceFile(serial, input.Files[index])
		if err != nil {
			return nil, false
		}
		metadata, err := printSource(input, newSourceMetadata(input.Files[index], transformed, serial.EmitContext()))
		if err != nil {
			return nil, false
		}
		results[index] = parallelFileResult{transformed: transformed, state: serial, metadata: metadata, operations: recording.operations}
	}

	for _, result := range results {
		if err := replayRecordedOperations(state.project.buildInfo, result.operations); err != nil {
			return nil, false
		}
	}
	seen := map[*TransformState]bool{}
	for _, result := range results {
		if !seen[result.state] {
			seen[result.state] = true
			state.diagnostics = append(state.diagnostics, result.state.diagnostics...)
		}
	}
	return results, true
}
