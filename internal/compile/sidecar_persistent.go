package compile

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"time"

	"rotor/internal/logservice"
	"rotor/tsgo/ast"
)

func localSidecarControlRequest(key string, request sidecarRequest, timeout time.Duration) (*sidecarResponse, sidecarCallStats, error) {
	var stats sidecarCallStats
	slot := sidecarSlotFor(key)
	slot.mu.Lock()
	defer slot.mu.Unlock()
	if slot.session == nil || slot.session.dead {
		return nil, stats, errors.New("transformer result worker is no longer available")
	}
	started := time.Now()
	payload, err := json.Marshal(request)
	stats.prep = time.Since(started)
	if err != nil {
		return nil, stats, err
	}
	stats.requestBytes = int64(len(payload))
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	roundTripStarted := time.Now()
	line, err := slot.session.writeAndRead(ctx, payload)
	stats.roundTrip = time.Since(roundTripStarted)
	cancel()
	slot.session.stderr.drainTo()
	if err != nil {
		slot.session.close()
		slot.session = nil
		return nil, stats, err
	}
	stats.responseBytes = int64(len(line))
	decodeStarted := time.Now()
	var response sidecarResponse
	decodeErr := json.Unmarshal(bytes.TrimSpace(line), &response)
	stats.decode = time.Since(decodeStarted)
	if decodeErr != nil {
		return nil, stats, slot.session.fail(decodeErr)
	}
	for _, logLine := range response.Logs {
		logservice.WriteLine(logLine)
	}
	if response.Metrics != nil {
		stats.nodeWallMs = response.Metrics.WallMs
		stats.nodeCPUUserUs = response.Metrics.CPUUserUs
		stats.nodeCPUSystemUs = response.Metrics.CPUSystemUs
		stats.nodeVersion = response.Metrics.NodeVersion
	}
	return &response, stats, nil
}

func persistentTransformerSidecar(
	dir, configPath string,
	compileFiles, rootFiles, stampFiles []*ast.SourceFile,
	overlays map[string]string,
	plugins []json.RawMessage,
	state *sidecarBuildState,
	timeout time.Duration,
) (*sidecarResponse, sidecarCallStats, error) {
	preparationStarted := time.Now()
	workspaceDir := dir
	if state != nil && state.workspaceDir != "" {
		workspaceDir = state.workspaceDir
	}
	identity, err := resolveSidecarWorkerIdentityForWorkspace(dir, configPath, workspaceDir)
	if err != nil {
		return nil, sidecarCallStats{prep: time.Since(preparationStarted)}, err
	}
	stampNames := make([]string, 0, len(stampFiles))
	for _, sourceFile := range stampFiles {
		stampNames = append(stampNames, sourceFile.FileName())
	}
	sidecarOverlays, overlayReads := mergeSidecarOverlays(compileFiles, overlays, state, true)
	includeSyntheticSidecarOverlays(configPath, stampFiles, sidecarOverlays)
	fileContentIdentities, identityReads, err := sidecarSourceContentIdentities(stampFiles, sidecarOverlays)
	if err != nil {
		return nil, sidecarCallStats{prep: time.Since(preparationStarted), reads: overlayReads + identityReads}, err
	}
	request := sidecarRequest{
		Protocol:              sidecarNodeProtocolVersion,
		Operation:             "transform",
		TsConfigPath:          filepath.FromSlash(identity.ConfigPath),
		ProjectDir:            filepath.FromSlash(identity.ProjectDir),
		CompileFileNames:      make([]string, 0, len(compileFiles)),
		FileNames:             append([]string(nil), stampNames...),
		Plugins:               plugins,
		FileContentIdentities: &fileContentIdentities,
	}
	if state != nil {
		request.leaseOwner = state.leaseOwner
	}
	for _, sourceFile := range compileFiles {
		request.CompileFileNames = append(request.CompileFileNames, filepath.FromSlash(sourceFile.FileName()))
	}
	request.RootFileNames = narrowedSidecarRoots(compileFiles, rootFiles)
	preparationDuration := time.Since(preparationStarted)

	response, stats, err := persistentSidecarRequest(identity, request, stampNames, sidecarOverlays, timeout)
	stats.prep += preparationDuration
	stats.reads += overlayReads + identityReads
	if err != nil {
		return nil, stats, err
	}
	if state != nil {
		state.diskScanned = true
	}
	if response.ResultHandle != "" {
		response.lease = newSidecarTraceLease(response.ResultHandle, func(control sidecarRequest) (*sidecarResponse, sidecarCallStats, error) {
			control.TsConfigPath = filepath.FromSlash(identity.ConfigPath)
			control.ProjectDir = filepath.FromSlash(identity.ProjectDir)
			control.leaseOwner = request.leaseOwner
			result, controlStats, err := persistentSidecarRequest(identity, control, nil, nil, timeout)
			absorbPersistentSidecarMetrics(&controlStats, result, configPath, false)
			if err != nil {
				response.abandon()
			}
			return result, controlStats, err
		})
	}
	absorbPersistentSidecarMetrics(&stats, response, configPath, true)
	return response, stats, nil
}

func persistentSidecarValidation(dir, configPath, workspaceDir string, timeout time.Duration) (*sidecarResponse, sidecarCallStats, error) {
	preparationStarted := time.Now()
	identity, err := resolveSidecarWorkerIdentityForWorkspace(dir, configPath, workspaceDir)
	if err != nil {
		return nil, sidecarCallStats{prep: time.Since(preparationStarted)}, err
	}
	preparationDuration := time.Since(preparationStarted)
	response, stats, err := persistentSidecarRequest(identity, sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "validate",
		TsConfigPath: filepath.FromSlash(identity.ConfigPath),
		ProjectDir:   filepath.FromSlash(identity.ProjectDir),
	}, nil, nil, timeout)
	stats.prep += preparationDuration
	if err == nil {
		absorbPersistentSidecarMetrics(&stats, response, configPath, false)
	}
	return response, stats, err
}

func persistentSidecarRequest(identity sidecarWorkerIdentity, request sidecarRequest, stampNames []string, overlays map[string]string, timeout time.Duration) (*sidecarResponse, sidecarCallStats, error) {
	return persistentSidecarRequestContext(context.Background(), identity, request, stampNames, overlays, timeout)
}

func persistentSidecarRequestContext(parent context.Context, identity sidecarWorkerIdentity, request sidecarRequest, stampNames []string, overlays map[string]string, timeout time.Duration) (*sidecarResponse, sidecarCallStats, error) {
	var stats sidecarCallStats
	started := time.Now()
	payload, err := json.Marshal(request)
	stats.prep = time.Since(started)
	if err != nil {
		return nil, stats, err
	}
	stats.requestBytes = int64(len(payload))

	ctx, cancel := context.WithTimeout(parent, timeout)
	defer cancel()
	roundTripStarted := time.Now()
	result, err := SidecarDaemonRoundTrip(ctx, SidecarDaemonCall{
		WorkspaceKey:   identity.WorkspaceKey,
		WorkerKey:      identity.WorkerKey,
		ProjectDir:     filepath.FromSlash(identity.ProjectDir),
		SidecarDir:     filepath.FromSlash(identity.SidecarDir),
		NodePath:       filepath.FromSlash(identity.NodePath),
		ChildEnv:       identity.ChildEnv,
		LeaseOwner:     request.leaseOwner,
		Payload:        payload,
		StampFileNames: stampNames,
		Overlays:       overlays,
	})
	stats.roundTrip = time.Since(roundTripStarted)
	if err != nil {
		return nil, stats, err
	}
	stats.responseBytes = int64(len(result.Payload))
	stats.stats = result.Stats
	stats.reads += result.Reads
	stats.changedFiles = result.ChangedFiles
	stats.spawned = result.Spawned

	decodeStarted := time.Now()
	var response sidecarResponse
	decodeErr := json.Unmarshal(bytes.TrimSpace(result.Payload), &response)
	stats.decode = time.Since(decodeStarted)
	if decodeErr != nil {
		result.abandon()
		return nil, stats, fmt.Errorf("decode transformer sidecar response: %w", decodeErr)
	}
	response.abandon = result.abandon
	for _, line := range response.Logs {
		logservice.WriteLine(line)
	}
	return &response, stats, nil
}

func absorbPersistentSidecarMetrics(stats *sidecarCallStats, response *sidecarResponse, configPath string, logPlugins bool) {
	if response == nil || response.Metrics == nil {
		return
	}
	stats.nodeWallMs = response.Metrics.WallMs
	stats.nodeCPUUserUs = response.Metrics.CPUUserUs
	stats.nodeCPUSystemUs = response.Metrics.CPUSystemUs
	stats.nodeVersion = response.Metrics.NodeVersion
	if logPlugins {
		logPluginMetrics(configPath, response.Metrics.Plugins)
	}
}
