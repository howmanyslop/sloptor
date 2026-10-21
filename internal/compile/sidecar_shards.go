package compile

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strconv"
	"strings"
	"sync"

	"rotor/tsgo/ast"
)

const (
	sidecarShardsEnv = "ROTOR_SIDECAR_SHARDS"
	// sidecarShardMaxDefault caps the default fan-out. Every shard is a Node
	// process holding its own TypeScript program, so past four the shared
	// import closure each one has to parse and bind outweighs the transform
	// time it splits.
	sidecarShardMaxDefault = 4
	// sidecarShardMinFiles keeps small projects and incremental rebuilds on one
	// warm worker: below this many files per shard, spawning another Node and
	// building another program costs more than the transform it would take on.
	sidecarShardMinFiles = 256
)

// sidecarShardCount is how many workers a transform of fileCount files fans
// out across. ROTOR_SIDECAR_SHARDS overrides the CPU-derived default; 1 turns
// sharding off.
func sidecarShardCount(fileCount int) int {
	limit := min(sidecarShardMaxDefault, max(1, runtime.NumCPU()/2))
	if raw := os.Getenv(sidecarShardsEnv); raw != "" {
		if n, err := strconv.Atoi(raw); err == nil && n >= 1 {
			limit = n
		}
	}
	return max(1, min(limit, fileCount/sidecarShardMinFiles))
}

// splitSidecarShards partitions files into count groups of similar total text
// length: largest file first, each onto the lightest shard so far. Shards run
// programless for transformers that never read their program, where file
// locality buys nothing and one heavy directory (a React tree) would otherwise
// land on a single worker. Each shard keeps the caller's file order.
func splitSidecarShards(files []*ast.SourceFile, count int) [][]*ast.SourceFile {
	order := make([]int, len(files))
	for index := range order {
		order[index] = index
	}
	slices.SortStableFunc(order, func(a, b int) int {
		if sizeA, sizeB := len(files[a].Text()), len(files[b].Text()); sizeA != sizeB {
			return sizeB - sizeA
		}
		return strings.Compare(files[a].FileName(), files[b].FileName())
	})
	loads := make([]int, count)
	owner := make([]int, len(files))
	for _, index := range order {
		lightest := 0
		for shard := 1; shard < count; shard++ {
			if loads[shard] < loads[lightest] {
				lightest = shard
			}
		}
		owner[index] = lightest
		loads[lightest] += len(files[index].Text()) + 1
	}
	shards := make([][]*ast.SourceFile, count)
	for index, file := range files {
		shards[owner[index]] = append(shards[owner[index]], file)
	}
	return slices.DeleteFunc(shards, func(shard []*ast.SourceFile) bool { return len(shard) == 0 })
}

// runShardedTransformerSidecar runs one transform across several workers when
// the request is large enough to pay for them. The configured transformers are
// per-file (see narrowedSidecarRoots), so each shard sees the same checker
// answers about its files that a single worker would.
func runShardedTransformerSidecar(ctx context.Context, call transformerCall) (*sidecarResponse, sidecarCallStats, error) {
	count := sidecarShardCount(len(call.compileFiles))
	if count <= 1 {
		return runTransformerSidecarWithContext(ctx, call.dir, call.configPath, call.compileFiles, call.stampFiles, call.overlays, call.plugins, call.state, call.sourceOrigin)
	}
	shards := splitSidecarShards(call.compileFiles, count)
	responses := make([]*sidecarResponse, len(shards))
	stats := make([]sidecarCallStats, len(shards))
	errs := make([]error, len(shards))
	var wg sync.WaitGroup
	for index, files := range shards {
		// Each shard reads the build state but must not publish to it while the
		// others are still reading; the merged result is published below.
		var state *sidecarBuildState
		if call.state != nil {
			state = &sidecarBuildState{stageOverlays: call.state.stageOverlays, diskScanned: call.state.diskScanned, rebases: call.state.rebases}
		}
		wg.Go(func() {
			responses[index], stats[index], errs[index] = runTransformerSidecarShard(ctx, index, call.dir, call.configPath, files, call.stampFiles, call.overlays, call.plugins, state, call.sourceOrigin)
		})
	}
	wg.Wait()

	merged := mergeSidecarShardStats(stats)
	for _, err := range errs {
		if err != nil {
			return nil, merged, err
		}
	}
	if call.state != nil {
		call.state.diskScanned = true
	}
	response := &sidecarResponse{}
	for _, part := range responses {
		response.Diagnostics = append(response.Diagnostics, part.Diagnostics...)
		response.Transformed = append(response.Transformed, part.Transformed...)
		response.AfterDeclarationsTransformers = max(response.AfterDeclarationsTransformers, part.AfterDeclarationsTransformers)
		if part.Metrics != nil && response.Metrics == nil {
			response.Metrics = part.Metrics
		}
	}
	return response, merged, nil
}

// mergeSidecarShardStats folds per-shard stats into one call. Durations are
// wall time of the fan-out, so they take the slowest shard; counts and CPU add.
func mergeSidecarShardStats(parts []sidecarCallStats) sidecarCallStats {
	var merged sidecarCallStats
	for _, part := range parts {
		merged.wait = max(merged.wait, part.wait)
		merged.prep = max(merged.prep, part.prep)
		merged.roundTrip = max(merged.roundTrip, part.roundTrip)
		merged.decode += part.decode
		merged.requestBytes += part.requestBytes
		merged.responseBytes += part.responseBytes
		merged.stats += part.stats
		merged.reads += part.reads
		merged.changedFiles += part.changedFiles
		merged.spawned = merged.spawned || part.spawned
		merged.restarted = merged.restarted || part.restarted
		merged.nodeWallMs = max(merged.nodeWallMs, part.nodeWallMs)
		merged.nodeCPUUserUs += part.nodeCPUUserUs
		merged.nodeCPUSystemUs += part.nodeCPUSystemUs
		if part.nodeVersion != "" {
			merged.nodeVersion = part.nodeVersion
		}
	}
	return merged
}

// warmTransformerSidecarShards starts, in the background, the workers that a
// transform of fileCount files will fan out across, and has each load
// TypeScript, resolve the config and load the plugins. A caller with other
// work to do before the transform (the native jecs stage) hides those fixed
// costs behind it. Each shard's slot is taken before this returns, so a
// transform that arrives early waits for its warm worker instead of spawning
// a second one. Failures are dropped: the transform spawns its own worker.
func warmTransformerSidecarShards(dir, configPath string, plugins []json.RawMessage, fileCount int) {
	sidecarDir, err := resolveSidecarDir()
	if err != nil {
		return
	}
	timeout, err := sidecarResponseTimeout()
	if err != nil {
		return
	}
	dirPath, sidecarConfigPath, key := canonicalTransformerRequestPaths(dir, configPath)
	payload, err := json.Marshal(sidecarRequest{
		Protocol:     1,
		Operation:    "warm",
		TsConfigPath: filepath.FromSlash(sidecarConfigPath),
		ProjectDir:   filepath.FromSlash(dirPath),
		Plugins:      plugins,
	})
	if err != nil {
		return
	}
	for shard := range sidecarShardCount(fileCount) {
		slotKey := key
		if shard > 0 {
			slotKey += "#" + strconv.Itoa(shard)
		}
		slot := sidecarSlotFor(slotKey)
		slot.mu.Lock()
		go func() {
			defer slot.mu.Unlock()
			if slot.session != nil && !slot.session.dead {
				return
			}
			session, err := spawnSidecarSession(dirPath, sidecarDir)
			if err != nil {
				return
			}
			slot.session = session
			ctx, cancel := context.WithTimeout(context.Background(), timeout)
			defer cancel()
			if _, err := session.writeAndRead(ctx, payload); err != nil {
				session.close()
				slot.session = nil
				return
			}
			session.stderr.drainTo()
		}()
	}
}
