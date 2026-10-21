package compile

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"time"

	"rotor/internal/config"
	"rotor/internal/logservice"
	"rotor/tsgo/vfs/osvfs"
)

type parallelPrefixKey struct{}

type cliTransformerBuild struct{ split chan struct{} }

// MarkOneShotCLITransformerBuild is called only by the actual non-watch,
// non-declaration CLI build entry. Option copies share a nonblocking token;
// they do not replace the effective project Context or Timings context.
func MarkOneShotCLITransformerBuild(opts *ProjectOptions) {
	opts.cliTransformerBuild = &cliTransformerBuild{split: make(chan struct{}, 1)}
}

type parallelPrefix struct {
	attestation *config.ParallelTransformersConfig
	build       *cliTransformerBuild
}

func parallelPrefixContext(ctx context.Context, dir string, pipeline *flameworkPipeline, opts ProjectOptions) context.Context {
	if pipeline == nil || len(pipeline.prefix) == 0 || len(pipeline.suffix) != 0 || opts.census != nil || opts.EmitDeclarationOnly || opts.cliTransformerBuild == nil || TransformerCallbackFromContext(ctx) != nil {
		return ctx
	}
	if cfg, err := config.Load(dir); err == nil && parallelConfigSupported(cfg) {
		return context.WithValue(ctx, parallelPrefixKey{}, &parallelPrefix{attestation: cfg.ParallelTransformers, build: opts.cliTransformerBuild})
	}
	return ctx
}

func (build *cliTransformerBuild) trySplit() bool {
	select {
	case build.split <- struct{}{}:
		return true
	default:
		return false
	}
}

func parallelConfigSupported(cfg *config.Config) bool {
	p := cfg.ParallelTransformers
	if p == nil || p.Workers != 2 || !p.BatchIndependent || p.TypeScript == "" || len(p.Plugins) == 0 {
		return false
	}
	// Follow Load's forward-compatible unknown-key semantics, but never admit
	// an attestation whose unsupported fields may change its meaning.
	for _, warning := range cfg.Warnings {
		if strings.Contains(warning, `"parallelTransformers`) {
			return false
		}
	}
	return true
}

func parallelRequestSupported(call transformerCall, request sidecarRequest) bool {
	if call.parallel == nil || call.parallel.Workers != 2 || !call.parallel.BatchIndependent ||
		len(request.CompileFileNames) < 2 || len(request.RootFileNames) != 0 || len(request.Plugins) == 0 {
		return false
	}
	// Every overlay must be an authored compile source, never a config,
	// declaration, package manifest, or another stage's synthetic input.
	compileNames := make(map[string]bool, len(request.CompileFileNames))
	for _, name := range request.CompileFileNames {
		key := normalizeOverlayPath(name, osvfs.FS().UseCaseSensitiveFileNames())
		if compileNames[key] {
			return false
		}
		compileNames[key] = true
	}
	for _, changed := range request.ChangedFiles {
		name := strings.ToLower(changed.FileName)
		if !compileNames[normalizeOverlayPath(changed.FileName, osvfs.FS().UseCaseSensitiveFileNames())] ||
			(!strings.HasSuffix(name, ".ts") && !strings.HasSuffix(name, ".tsx")) || strings.HasSuffix(name, ".d.ts") {
			return false
		}
	}
	return parallelResolutionScope(call.dir, request.CompileFileNames)
}

// parallelResolutionScope proves that every batch's first source has the
// same ancestor package-resolution scope. Only unique ancestor directories
// are inspected; no dependency-store traversal is involved. Reject aliases
// (including source symlinks) rather than guessing their resolution scope.
func parallelResolutionScope(projectDir string, files []string) bool {
	root, err := filepath.Abs(filepath.FromSlash(projectDir))
	if err != nil {
		return false
	}
	physical, err := filepath.EvalSymlinks(root)
	if err != nil || !sameParallelPath(root, physical) {
		return false
	}
	checked := map[string]bool{}
	regularFiles := map[string]bool{}
	for _, file := range files {
		if !filepath.IsAbs(file) {
			return false
		}
		rel, err := filepath.Rel(root, filepath.Clean(file))
		if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) || filepath.IsAbs(rel) {
			return false
		}
		for _, segment := range strings.Split(rel, string(filepath.Separator)) {
			if strings.EqualFold(segment, "node_modules") {
				return false
			}
		}
		for dir := filepath.Dir(file); ; dir = filepath.Dir(dir) {
			key := normalizeOverlayPath(dir, osvfs.FS().UseCaseSensitiveFileNames())
			if checked[key] {
				break
			}
			actual, err := filepath.EvalSymlinks(dir)
			if err != nil || !sameParallelPath(dir, actual) {
				return false
			}
			entries, err := os.ReadDir(dir)
			if err != nil {
				return false
			}
			for _, entry := range entries {
				if sameParallelPath(dir, root) && strings.EqualFold(entry.Name(), "node_modules") {
					continue // The common root dependency scope is allowed.
				}
				// Conservative: even an unrelated symlink in a compile scope
				// selects serial. It also catches file aliases without one stat
				// per compile file.
				if entry.Type()&(os.ModeSymlink|os.ModeIrregular) != 0 || (!sameParallelPath(dir, root) && (strings.EqualFold(entry.Name(), "package.json") || strings.EqualFold(entry.Name(), "node_modules"))) {
					return false
				}
				if entry.Type().IsRegular() {
					regularFiles[normalizeOverlayPath(filepath.Join(dir, entry.Name()), osvfs.FS().UseCaseSensitiveFileNames())] = true
				}
			}
			checked[key] = true
			if sameParallelPath(dir, root) {
				break
			}
		}
		if !regularFiles[normalizeOverlayPath(file, osvfs.FS().UseCaseSensitiveFileNames())] {
			return false
		}
	}
	return true
}

func sameParallelPath(a, b string) bool {
	// canonicalSidecarPath resolves aliases, which must not be erased here.
	return normalizeOverlayPath(filepath.Clean(a), osvfs.FS().UseCaseSensitiveFileNames()) == normalizeOverlayPath(filepath.Clean(b), osvfs.FS().UseCaseSensitiveFileNames())
}

func splitParallelRequest(request sidecarRequest) [2]sidecarRequest {
	workers := [2]sidecarRequest{request, request}
	mid := (len(request.CompileFileNames) + 1) / 2
	workers[0].CompileFileNames = request.CompileFileNames[:mid]
	workers[1].CompileFileNames = request.CompileFileNames[mid:]
	workers[0].ParallelWorker = true
	workers[1].ParallelWorker = true
	return workers
}

// handled=false means no transform was sent. A non-nil error in that case
// consumes worker 0's existing serial retry budget; nil keeps the same session.
// Post-dispatch errors never enter that retry loop. Parent cancellation and
// worker 0's request deadline remain terminal even before dispatch.
func runParallelTransformer(parent context.Context, timeout time.Duration, sidecarDir string, slot *sidecarSlot, request sidecarRequest, attestation *config.ParallelTransformersConfig, stats *sidecarCallStats) (*sidecarResponse, bool, error) {
	return runParallelTransformerWithSpawner(parent, timeout, sidecarDir, slot, request, attestation, stats, spawnSidecarSessionMode)
}

func runParallelTransformerWithSpawner(parent context.Context, timeout time.Duration, sidecarDir string, slot *sidecarSlot, request sidecarRequest, attestation *config.ParallelTransformersConfig, stats *sidecarCallStats, spawn func(string, string, bool) (*sidecarSession, error)) (*sidecarResponse, bool, error) {
	first := slot.session
	ctx, cancel := context.WithTimeout(nonNilContext(parent), timeout)
	defer cancel()
	preflight := request
	preflight.Operation = "parallel-preflight"
	preflight.ParallelAttestation = attestation
	check := func(session *sidecarSession) (bool, error) {
		response, err := parallelRoundTrip(ctx, session, preflight, stats)
		return err == nil && response.ParallelReady && len(response.Diagnostics) == 0, err
	}
	ready, err := check(first)
	if err != nil {
		first.close()
		slot.session = nil
		first.stderr.drainTo()
		return nil, nonNilContext(parent).Err() != nil || errors.Is(err, context.DeadlineExceeded), err
	}
	if !ready {
		if err := nonNilContext(parent).Err(); err != nil {
			first.close()
			slot.session = nil
			first.stderr.drainTo()
			return nil, true, err
		}
		return nil, false, nil
	}
	second, err := spawn(request.ProjectDir, sidecarDir, true)
	if err != nil {
		return nil, false, nil
	}
	stats.additionalSpawns++
	ready, err = check(second)
	if !ready || err != nil {
		cleanupErr := second.closeWithExpectedKill(errors.Is(err, context.DeadlineExceeded))
		parentErr := nonNilContext(parent).Err()
		if parentErr != nil || cleanupErr != nil {
			primaryCleanupErr := first.closeAndWait()
			slot.session = nil
			first.stderr.drainTo()
			return nil, true, errors.Join(parentErr, cleanupErr, primaryCleanupErr)
		}
		return nil, false, nil
	}
	cancel()
	ctx, cancel = context.WithTimeout(nonNilContext(parent), timeout)
	defer cancel()
	workers := [2]*sidecarSession{first, second}
	requests := splitParallelRequest(request)
	type result struct {
		response *sidecarResponse
		stats    sidecarCallStats
		err      error
	}
	results := [2]result{}
	done := make(chan int, 2)
	stopRoundTrip := logStageNamed(request.TsConfigPath, transformerRoundTripStageName(request.TsConfigPath, request.Plugins))
	for index := range workers {
		go func(index int) {
			r := &results[index]
			r.response, r.err = parallelRoundTrip(ctx, workers[index], requests[index], &r.stats)
			done <- index
		}(index)
	}
	for range workers {
		index := <-done
		if results[index].err != nil {
			cancel()
		}
	}
	wall := stopRoundTrip()
	// Same ownership and two-second EOF/kill/reap policy as ordinary sessions.
	// Reap both before forwarding stderr so pipe-reader scheduling cannot
	// reorder or omit the final bytes of either range.
	type cleanupResult struct {
		index int
		err   error
	}
	reaped := make(chan cleanupResult, 2)
	for index, worker := range workers {
		go func() { reaped <- cleanupResult{index, worker.closeAndWait()} }()
	}
	cleanupErrors := [2]error{}
	for range workers {
		result := <-reaped
		if result.err != nil {
			cleanupErrors[result.index] = fmt.Errorf("transformer worker %d cleanup: %w", result.index, result.err)
		}
	}
	slot.session = nil
	for index := range results {
		r := &results[index]
		stats.requestBytes += r.stats.requestBytes
		stats.responseBytes += r.stats.responseBytes
		stats.decode += r.stats.decode
	}
	stats.roundTrip += wall
	if results[0].err == nil && parallelInternalFailure(results[0].response) {
		response := *results[0].response
		response.ParallelBatchDiagnostics = 0
		response.ParallelSourceDiagnostics = 0
		var secondMetrics *TransformerResponseMetrics
		if results[1].response != nil {
			secondMetrics = results[1].response.Metrics
		}
		response.Metrics = mergeParallelMetrics(response.Metrics, secondMetrics, wall)
		first.stderr.drainTo()
		applyTransformerResponseMetrics(stats, &response, request.TsConfigPath)
		return &response, true, nil
	}
	// Prefer a real transport failure to its sibling's cancellation, while
	// keeping range order for two independent transport failures.
	for _, r := range results {
		if r.err != nil && !errors.Is(r.err, context.Canceled) {
			for _, worker := range workers {
				worker.stderr.drainTo()
			}
			return nil, true, errors.Join(r.err, cleanupErrors[0], cleanupErrors[1])
		}
	}
	for _, r := range results {
		if r.err != nil {
			for _, worker := range workers {
				worker.stderr.drainTo()
			}
			return nil, true, errors.Join(r.err, cleanupErrors[0], cleanupErrors[1])
		}
	}
	response, last, err := mergeParallelResponses(results[0].response, results[1].response, wall)
	for index := 0; index <= last; index++ {
		workers[index].stderr.drainTo()
	}
	// An ordered in-band catch already fails the build with the serial shape.
	// Otherwise, incomplete drains/abnormal exits must not produce green output.
	if err == nil && parallelInternalFailure(response) {
		applyTransformerResponseMetrics(stats, response, request.TsConfigPath)
		return response, true, nil
	}
	err = errors.Join(err, cleanupErrors[0], cleanupErrors[1])
	if err != nil {
		return nil, true, err
	}
	applyTransformerResponseMetrics(stats, response, request.TsConfigPath)
	logservice.WriteLineIfVerbose(projectLabel(request.TsConfigPath) + ": transformer parallel batch completed (workers=2)")
	return response, true, nil
}

func parallelRoundTrip(ctx context.Context, session *sidecarSession, request sidecarRequest, stats *sidecarCallStats) (*sidecarResponse, error) {
	if err := ctx.Err(); err != nil {
		return nil, session.fail(fmt.Errorf("transformer sidecar response timed out: %w", err))
	}
	payload, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}
	stats.requestBytes += int64(len(payload))
	started := time.Now()
	line, err := session.writeAndRead(ctx, payload)
	stats.roundTrip += time.Since(started)
	if err != nil {
		return nil, err
	}
	stats.responseBytes += int64(len(line))
	started = time.Now()
	var response sidecarResponse
	err = json.Unmarshal(line, &response)
	stats.decode += time.Since(started)
	return &response, err
}

func mergeParallelResponses(first, second *sidecarResponse, wall time.Duration) (*sidecarResponse, int, error) {
	for index, response := range []*sidecarResponse{first, second} {
		if parallelInternalFailure(response) {
			// The serial catch discards the entire batch, even if earlier
			// files transformed successfully. Keep that first failing range's
			// exact diagnostic and discard stderr from later ranges.
			copy := *response
			copy.ParallelBatchDiagnostics = 0
			copy.ParallelSourceDiagnostics = 0
			copy.Metrics = mergeParallelMetrics(first.Metrics, second.Metrics, wall)
			return &copy, index, nil
		}
	}
	a, b := first.ParallelBatchDiagnostics, second.ParallelBatchDiagnostics
	x, y := first.ParallelSourceDiagnostics, second.ParallelSourceDiagnostics
	if a < 0 || b < 0 || a > len(first.Diagnostics) || b > len(second.Diagnostics) || a != b ||
		x < 0 || y < 0 || x > len(first.Diagnostics)-a || y > len(second.Diagnostics)-b ||
		(a != 0 && !reflect.DeepEqual(first.Diagnostics[:a], second.Diagnostics[:b])) || first.AfterDeclarationsTransformers != second.AfterDeclarationsTransformers {
		return nil, 1, fmt.Errorf("transformer workers returned incompatible batch diagnostics or declaration stages")
	}
	merged := *first
	merged.ParallelBatchDiagnostics = 0
	merged.ParallelSourceDiagnostics = 0
	// Common factory/config diagnostics occur once in serial, before the
	// ordered per-file diagnostics. This is an explicit protocol partition,
	// never diagnostic normalization, sorting or duplicate removal.
	// Serial collects all missing-source diagnostics before transformNodes
	// runs. Preserve that phase order too, including an error in range 1.
	merged.Diagnostics = append([]sidecarDiagnostic{}, first.Diagnostics[:a+x]...)
	merged.Diagnostics = append(merged.Diagnostics, second.Diagnostics[b:b+y]...)
	merged.Diagnostics = append(merged.Diagnostics, first.Diagnostics[a+x:]...)
	merged.Diagnostics = append(merged.Diagnostics, second.Diagnostics[b+y:]...)
	merged.Transformed = append(append([]sidecarOutputFile{}, first.Transformed...), second.Transformed...)
	merged.Metrics = mergeParallelMetrics(first.Metrics, second.Metrics, wall)
	return &merged, 1, nil
}

func parallelInternalFailure(response *sidecarResponse) bool {
	return response != nil && len(response.Transformed) == 0 && len(response.Diagnostics) == 1 &&
		response.Diagnostics[0].Category == "error" && response.Diagnostics[0].Code == "sidecar-internal"
}

func mergeParallelMetrics(a, b *TransformerResponseMetrics, wall time.Duration) *TransformerResponseMetrics {
	if a == nil || b == nil {
		return nil
	}
	metrics := &TransformerResponseMetrics{
		WallMs: wall.Milliseconds(), CPUUserUs: a.CPUUserUs + b.CPUUserUs,
		CPUSystemUs: a.CPUSystemUs + b.CPUSystemUs, NodeVersion: a.NodeVersion,
	}
	// Plugin times are summed worker work, not elapsed build time. Phase
	// timings overlap across workers and are deliberately omitted.
	metrics.Plugins = append([]TransformerPluginMetric{}, a.Plugins...)
	if len(a.Plugins) == len(b.Plugins) {
		for index := range metrics.Plugins {
			if metrics.Plugins[index].Transform != b.Plugins[index].Transform {
				metrics.Plugins = nil
				break
			}
			metrics.Plugins[index].Ms += b.Plugins[index].Ms
		}
	} else {
		metrics.Plugins = nil
	}
	return metrics
}
