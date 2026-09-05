package compile

import (
	"context"
	"path/filepath"
	"sync"
	"time"
)

const persistentSidecarWarmupStopWait = 5 * time.Second

type persistentSidecarWarmup struct {
	cancel context.CancelFunc
	done   chan struct{}
	once   sync.Once
}

func startPersistentSidecarWarmup(projectDir string, opts ProjectOptions) *persistentSidecarWarmup {
	if !PersistentSidecarDaemonEnabled() || opts.EmitDeclarationOnly {
		return nil
	}
	dir, err := filepath.Abs(projectDir)
	if err != nil {
		return nil
	}
	configPath := opts.TsConfigPath
	if configPath == "" {
		configPath = filepath.Join(dir, "tsconfig.json")
	} else if configPath, err = filepath.Abs(configPath); err != nil {
		return nil
	}
	plugins, err := effectiveTransformerPlugins(configPath)
	if err != nil {
		return nil
	}
	hasExternalTransformer := false
	for _, plugin := range plugins {
		if plugin.Transform != legacyFlameworkTransformer {
			hasExternalTransformer = true
			break
		}
	}
	if !hasExternalTransformer {
		return nil
	}

	ctx, cancel := context.WithCancel(context.Background())
	warmup := &persistentSidecarWarmup{cancel: cancel, done: make(chan struct{})}
	go func() {
		defer close(warmup.done)
		identity, err := resolveSidecarWorkerIdentityForWorkspace(dir, configPath, opts.sidecarWorkspaceDir)
		if err != nil {
			return
		}
		timeout, err := sidecarResponseTimeout()
		if err != nil {
			return
		}
		_, _, _ = persistentSidecarRequestContext(ctx, identity, sidecarRequest{
			Protocol:     sidecarNodeProtocolVersion,
			Operation:    "warm",
			TsConfigPath: filepath.FromSlash(identity.ConfigPath),
			ProjectDir:   filepath.FromSlash(identity.ProjectDir),
		}, nil, nil, timeout)
	}()
	return warmup
}

func (w *persistentSidecarWarmup) stop() {
	if w == nil {
		return
	}
	w.once.Do(func() {
		w.cancel()
		select {
		case <-w.done:
		case <-time.After(persistentSidecarWarmupStopWait):
		}
	})
}

func (w *persistentSidecarWarmup) wait() {
	if w == nil {
		return
	}
	<-w.done
}
