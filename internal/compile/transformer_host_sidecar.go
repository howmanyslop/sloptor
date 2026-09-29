package compile

import (
	"context"
	"encoding/json"
	"fmt"
	"path/filepath"
	"strings"
	"sync"

	"rotor/internal/logservice"
)

type sidecarTransformerHost struct{}

func (sidecarTransformerHost) transform(ctx context.Context, call transformerCall) (*sidecarResponse, sidecarCallStats, error) {
	return runTransformerSidecarWithContext(ctx, call.dir, call.configPath, call.compileFiles, call.stampFiles, call.overlays, call.plugins, call.state)
}

func (sidecarTransformerHost) validate(ctx context.Context, dir, configPath string) (*sidecarResponse, sidecarCallStats, error) {
	return runTransformerSidecarValidationWithContext(ctx, dir, configPath)
}

type callbackTransformerHost struct {
	callback TransformerCallback
	mu       sync.Mutex
	slots    map[string]*callbackTransformerSlot
}

type callbackTransformerSlot struct {
	mu      sync.Mutex
	tracker *sidecarSession
}

func newCallbackTransformerHost(callback TransformerCallback) transformerHost {
	return &callbackTransformerHost{callback: callback, slots: map[string]*callbackTransformerSlot{}}
}

func (h *callbackTransformerHost) transform(parent context.Context, call transformerCall) (*sidecarResponse, sidecarCallStats, error) {
	var stats sidecarCallStats
	timeout, err := sidecarResponseTimeout()
	if err != nil {
		return nil, stats, err
	}

	dir := canonicalSidecarPath(call.dir)
	configPath := canonicalSidecarPath(call.configPath)
	slot := h.slot(dir + "|" + configPath)
	stopWait := logStage(call.configPath, sidecarSessionWaitStage)
	slot.mu.Lock()
	stats.wait = stopWait()
	defer slot.mu.Unlock()

	stopPrep := logStage(call.configPath, sidecarPreparationStage)
	if slot.tracker == nil {
		slot.tracker = &sidecarSession{stamps: map[string]sidecarFileStamp{}, overlaid: map[string]string{}}
	}
	stampNames := make([]string, 0, len(call.stampFiles))
	for _, sourceFile := range call.stampFiles {
		stampNames = append(stampNames, sourceFile.FileName())
	}
	overlays, overlayReads := mergeSidecarOverlays(call.compileFiles, call.overlays, call.state, true)
	stats.reads += overlayReads
	skipDiskScan := call.state != nil && call.state.diskScanned && len(slot.tracker.stamps) > 0
	changedFiles, ioStats, err := slot.tracker.collectChangedFiles(stampNames, overlays, skipDiskScan)
	stats.stats += ioStats.stats
	stats.reads += ioStats.reads
	stats.changedFiles += ioStats.changedFiles
	if err != nil {
		stats.prep += stopPrep()
		return nil, stats, err
	}

	request := sidecarRequest{
		Protocol: 1, Operation: "transform",
		TsConfigPath: filepath.FromSlash(configPath), ProjectDir: filepath.FromSlash(dir),
		CompileFileNames: make([]string, 0, len(call.compileFiles)),
		ChangedFiles:     changedFiles, Plugins: call.plugins,
	}
	for _, sourceFile := range call.compileFiles {
		request.CompileFileNames = append(request.CompileFileNames, filepath.FromSlash(sourceFile.FileName()))
	}
	request.RootFileNames = narrowedSidecarRoots(call.compileFiles, call.stampFiles)
	payload, err := json.Marshal(request)
	stats.prep += stopPrep()
	if err != nil {
		return nil, stats, err
	}
	stats.requestBytes = int64(len(payload))

	ctx, cancel := context.WithTimeout(nonNilContext(parent), timeout)
	defer cancel()
	stage := sidecarRoundTripStage.traceName()
	if logservice.Verbose {
		if names := sidecarPluginNames(call.plugins, call.configPath); len(names) > 0 {
			stage += " (" + strings.Join(names, ", ") + ")"
		}
	}
	stopRoundTrip := logStageNamed(call.configPath, stage)
	response, err := h.callback(ctx, request)
	stats.roundTrip = stopRoundTrip()
	if err != nil {
		return nil, stats, fmt.Errorf("transformer callback failed: %w", err)
	}
	if call.state != nil {
		call.state.diskScanned = true
	}
	if encoded, encodeErr := json.Marshal(response); encodeErr == nil {
		stats.responseBytes = int64(len(encoded))
	}
	applyCallbackMetrics(&stats, &response, call.configPath)
	return &response, stats, nil
}

func (h *callbackTransformerHost) validate(parent context.Context, dir, configPath string) (*sidecarResponse, sidecarCallStats, error) {
	var stats sidecarCallStats
	timeout, err := sidecarResponseTimeout()
	if err != nil {
		return nil, stats, err
	}
	dir = canonicalSidecarPath(dir)
	configPath = canonicalSidecarPath(configPath)
	slot := h.slot(dir + "|" + configPath)
	stopWait := logStage(configPath, sidecarSessionWaitStage)
	slot.mu.Lock()
	stats.wait = stopWait()
	defer slot.mu.Unlock()

	request := sidecarRequest{Protocol: 1, Operation: "validate", TsConfigPath: filepath.FromSlash(configPath), ProjectDir: filepath.FromSlash(dir)}
	if encoded, encodeErr := json.Marshal(request); encodeErr == nil {
		stats.requestBytes = int64(len(encoded))
	}
	ctx, cancel := context.WithTimeout(nonNilContext(parent), timeout)
	defer cancel()
	stopRoundTrip := logStage(configPath, sidecarRoundTripStage)
	response, err := h.callback(ctx, request)
	stats.roundTrip = stopRoundTrip()
	if err != nil {
		return nil, stats, fmt.Errorf("transformer callback failed: %w", err)
	}
	if encoded, encodeErr := json.Marshal(response); encodeErr == nil {
		stats.responseBytes = int64(len(encoded))
	}
	applyCallbackMetrics(&stats, &response, configPath)
	return &response, stats, nil
}

func (h *callbackTransformerHost) slot(key string) *callbackTransformerSlot {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.slots[key] == nil {
		h.slots[key] = &callbackTransformerSlot{}
	}
	return h.slots[key]
}

func applyCallbackMetrics(stats *sidecarCallStats, response *sidecarResponse, configPath string) {
	if response.Metrics == nil {
		return
	}
	stats.nodeWallMs = response.Metrics.WallMs
	stats.nodeCPUUserUs = response.Metrics.CPUUserUs
	stats.nodeCPUSystemUs = response.Metrics.CPUSystemUs
	stats.nodeVersion = response.Metrics.NodeVersion
	logPluginMetrics(configPath, response.Metrics.Plugins)
}

func nonNilContext(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	return ctx
}
