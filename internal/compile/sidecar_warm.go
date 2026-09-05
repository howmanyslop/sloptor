package compile

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"rotor/internal/rojo"
	"rotor/tsgo/core"
	"rotor/tsgo/outputpaths"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

const persistentSidecarWarmupStopWait = 5 * time.Second

type persistentSidecarWarmup struct {
	cancel context.CancelFunc
	done   chan struct{}
	once   sync.Once
}

func startPersistentSidecarWarmupIfCold(projectDir string, opts ProjectOptions) *persistentSidecarWarmup {
	if sidecarIncrementalManifestExists(projectDir, opts.TsConfigPath) {
		return nil
	}
	return startPersistentSidecarWarmup(projectDir, opts)
}

func sidecarIncrementalManifestExists(projectDir, configuredConfigPath string) bool {
	dir, err := filepath.Abs(projectDir)
	if err != nil {
		return false
	}
	configPath := configuredConfigPath
	if configPath == "" {
		configPath = filepath.Join(dir, "tsconfig.json")
	} else if configPath, err = filepath.Abs(configPath); err != nil {
		return false
	}
	paths := []string{outputManifestPath(dir, configPath)}
	if buildInfoPath := configuredTSBuildInfoPath(configPath); buildInfoPath != "" {
		paths = append(paths, buildInfoPath)
	}
	for _, path := range paths {
		if info, err := os.Stat(path); err == nil && info.Mode().IsRegular() {
			return true
		}
	}
	return false
}

func configuredTSBuildInfoPath(configPath string) string {
	options := &core.CompilerOptions{}
	if !applyManifestCompilerOptions(configPath, make(map[string]bool), options) {
		return ""
	}
	if !options.IsIncremental() {
		return ""
	}
	absolute, err := filepath.Abs(configPath)
	if err != nil {
		return ""
	}
	options.ConfigFilePath = filepath.ToSlash(absolute)
	return rojo.RbxtscBuildInfoPath(filepath.FromSlash(outputpaths.GetBuildInfoFileName(options, tspath.ComparePathsOptions{
		CurrentDirectory:          filepath.ToSlash(filepath.Dir(absolute)),
		UseCaseSensitiveFileNames: osvfs.FS().UseCaseSensitiveFileNames(),
	})))
}

func applyManifestCompilerOptions(configPath string, active map[string]bool, options *core.CompilerOptions) bool {
	absolute, err := filepath.Abs(configPath)
	if err != nil {
		return false
	}
	absolute = filepath.Clean(absolute)
	if active[absolute] {
		return false
	}
	active[absolute] = true
	defer delete(active, absolute)
	data, err := os.ReadFile(absolute)
	if err != nil {
		return false
	}
	var root map[string]json.RawMessage
	if json.Unmarshal([]byte(stripJSONC(string(data))), &root) != nil {
		return false
	}
	extendedConfigs, err := parseExtends(root["extends"])
	if err != nil {
		return false
	}
	for _, extended := range extendedConfigs {
		parent, resolveErr := resolveExtendedConfig(absolute, extended)
		if resolveErr != nil || !applyManifestCompilerOptions(parent, active, options) {
			return false
		}
	}
	compilerOptionsRaw, ok := root["compilerOptions"]
	if !ok {
		return true
	}
	var compilerOptions map[string]json.RawMessage
	if json.Unmarshal(compilerOptionsRaw, &compilerOptions) != nil {
		return false
	}
	if value, present := compilerOptions["incremental"]; present {
		var enabled bool
		if json.Unmarshal(value, &enabled) == nil {
			options.Incremental = core.BoolToTristate(enabled)
		}
	}
	if value, present := compilerOptions["composite"]; present {
		var enabled bool
		if json.Unmarshal(value, &enabled) == nil {
			options.Composite = core.BoolToTristate(enabled)
		}
	}
	for key, destination := range map[string]*string{
		"outDir":          &options.OutDir,
		"rootDir":         &options.RootDir,
		"tsBuildInfoFile": &options.TsBuildInfoFile,
	} {
		if value, present := compilerOptions[key]; present {
			if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
				*destination = ""
				continue
			}
			var configured string
			if json.Unmarshal(value, &configured) == nil {
				*destination = manifestConfigPath(filepath.Dir(absolute), configured)
			}
		}
	}
	return true
}

func manifestConfigPath(configDir, configured string) string {
	const configDirTemplate = "${configDir}"
	if len(configured) >= len(configDirTemplate) && strings.EqualFold(configured[:len(configDirTemplate)], configDirTemplate) {
		configured = "." + configured[len(configDirTemplate):]
	}
	return tspath.GetNormalizedAbsolutePath(configured, filepath.ToSlash(configDir))
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
