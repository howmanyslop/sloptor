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

func localSidecarControlRequest(key string, request sidecarRequest, timeout time.Duration) (*sidecarResponse, error) {
	slot := sidecarSlotFor(key)
	slot.mu.Lock()
	defer slot.mu.Unlock()
	if slot.session == nil || slot.session.dead {
		return nil, errors.New("transformer result worker is no longer available")
	}
	payload, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	line, err := slot.session.writeAndRead(ctx, payload)
	cancel()
	slot.session.stderr.drainTo()
	if err != nil {
		slot.session.close()
		slot.session = nil
		return nil, err
	}
	var response sidecarResponse
	if err := json.Unmarshal(bytes.TrimSpace(line), &response); err != nil {
		return nil, slot.session.fail(err)
	}
	for _, logLine := range response.Logs {
		logservice.WriteLine(logLine)
	}
	return &response, nil
}

func persistentTransformerSidecar(
	dir, configPath string,
	compileFiles, rootFiles, stampFiles []*ast.SourceFile,
	overlays map[string]string,
	plugins []json.RawMessage,
	state *sidecarBuildState,
	timeout time.Duration,
) (*sidecarResponse, sidecarCallStats, error) {
	workspaceDir := dir
	if state != nil && state.workspaceDir != "" {
		workspaceDir = state.workspaceDir
	}
	identity, err := resolveSidecarWorkerIdentityForWorkspace(dir, configPath, workspaceDir)
	if err != nil {
		return nil, sidecarCallStats{}, err
	}
	stampNames := make([]string, 0, len(stampFiles))
	for _, sourceFile := range stampFiles {
		stampNames = append(stampNames, sourceFile.FileName())
	}
	sidecarOverlays, overlayReads := mergeSidecarOverlays(compileFiles, overlays, state, true)
	request := sidecarRequest{
		Protocol:         sidecarNodeProtocolVersion,
		Operation:        "transform",
		TsConfigPath:     filepath.FromSlash(identity.ConfigPath),
		ProjectDir:       filepath.FromSlash(identity.ProjectDir),
		CompileFileNames: make([]string, 0, len(compileFiles)),
		FileNames:        append([]string(nil), stampNames...),
		Plugins:          plugins,
	}
	if state != nil {
		request.leaseOwner = state.leaseOwner
	}
	for _, sourceFile := range compileFiles {
		request.CompileFileNames = append(request.CompileFileNames, filepath.FromSlash(sourceFile.FileName()))
	}
	request.RootFileNames = narrowedSidecarRoots(compileFiles, rootFiles)

	response, stats, err := persistentSidecarRequest(identity, request, stampNames, sidecarOverlays, timeout)
	stats.reads += overlayReads
	if err != nil {
		return nil, stats, err
	}
	if state != nil {
		state.diskScanned = true
	}
	if response.ResultHandle != "" {
		response.lease = newSidecarTraceLease(response.ResultHandle, func(control sidecarRequest) (*sidecarResponse, error) {
			control.TsConfigPath = filepath.FromSlash(identity.ConfigPath)
			control.ProjectDir = filepath.FromSlash(identity.ProjectDir)
			control.leaseOwner = request.leaseOwner
			result, _, err := persistentSidecarRequest(identity, control, nil, nil, timeout)
			if err != nil {
				response.abandon()
			}
			return result, err
		})
	}
	absorbPersistentSidecarMetrics(&stats, response, configPath, true)
	return response, stats, nil
}

func persistentSidecarValidation(dir, configPath, workspaceDir string, timeout time.Duration) (*sidecarResponse, sidecarCallStats, error) {
	identity, err := resolveSidecarWorkerIdentityForWorkspace(dir, configPath, workspaceDir)
	if err != nil {
		return nil, sidecarCallStats{}, err
	}
	response, stats, err := persistentSidecarRequest(identity, sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "validate",
		TsConfigPath: filepath.FromSlash(identity.ConfigPath),
		ProjectDir:   filepath.FromSlash(identity.ProjectDir),
	}, nil, nil, timeout)
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
	if err := json.Unmarshal(bytes.TrimSpace(result.Payload), &response); err != nil {
		result.abandon()
		return nil, stats, fmt.Errorf("decode transformer sidecar response: %w", err)
	}
	response.abandon = result.abandon
	stats.decode = time.Since(decodeStarted)
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
