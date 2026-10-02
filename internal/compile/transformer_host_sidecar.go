package compile

import (
	"context"
	"fmt"
	"sync"
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

	dir, configPath, key := canonicalTransformerRequestPaths(call.dir, call.configPath)
	slot := h.slot(key)
	stopWait := logStage(call.configPath, sidecarSessionWaitStage)
	slot.mu.Lock()
	stats.wait = stopWait()
	defer slot.mu.Unlock()

	stopPrep := logStage(call.configPath, sidecarPreparationStage)
	if slot.tracker == nil {
		slot.tracker = &sidecarSession{stamps: map[string]sidecarFileStamp{}, overlaid: map[string]string{}}
	}
	request, err := prepareTransformerRequest(call, slot.tracker, dir, configPath, &stats)
	if err != nil {
		stats.prep += stopPrep()
		return nil, stats, err
	}
	stats.prep += stopPrep()

	ctx, cancel := context.WithTimeout(nonNilContext(parent), timeout)
	defer cancel()
	stage := transformerRoundTripStageName(call.configPath, call.plugins)
	stopRoundTrip := logStageNamed(call.configPath, stage)
	response, err := h.callback(ctx, request)
	stats.roundTrip = stopRoundTrip()
	if err != nil {
		return nil, stats, fmt.Errorf("transformer callback failed: %w", err)
	}
	if call.state != nil {
		call.state.diskScanned = true
	}
	if response.Transport != nil {
		stats.requestBytes = response.Transport.RequestBytes
		stats.responseBytes = response.Transport.ResponseBytes
	}
	applyTransformerResponseMetrics(&stats, &response, call.configPath)
	return &response, stats, nil
}

func (h *callbackTransformerHost) validate(parent context.Context, dir, configPath string) (*sidecarResponse, sidecarCallStats, error) {
	var stats sidecarCallStats
	timeout, err := sidecarResponseTimeout()
	if err != nil {
		return nil, stats, err
	}
	request, dir, configPath, key := prepareTransformerValidationRequest(dir, configPath)
	slot := h.slot(key)
	stopWait := logStage(configPath, sidecarSessionWaitStage)
	slot.mu.Lock()
	stats.wait = stopWait()
	defer slot.mu.Unlock()

	ctx, cancel := context.WithTimeout(nonNilContext(parent), timeout)
	defer cancel()
	stopRoundTrip := logStage(configPath, sidecarRoundTripStage)
	response, err := h.callback(ctx, request)
	stats.roundTrip = stopRoundTrip()
	if err != nil {
		return nil, stats, fmt.Errorf("transformer callback failed: %w", err)
	}
	if response.Transport != nil {
		stats.requestBytes = response.Transport.RequestBytes
		stats.responseBytes = response.Transport.ResponseBytes
	}
	applyTransformerResponseMetrics(&stats, &response, configPath)
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

func applyTransformerResponseMetrics(stats *sidecarCallStats, response *sidecarResponse, configPath string) {
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
