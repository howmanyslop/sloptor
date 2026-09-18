package compile

import (
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"rotor/internal/config"
	"rotor/tsgo/tsoptions"
)

const legacyFlameworkTransformer = "rbxts-transformer-flamework"

func prepareFlameworkConfig(dir string, parsed *tsoptions.ParsedCommandLine) (*config.FlameworkConfig, []string, error) {
	cfg, err := config.Load(dir)
	if err != nil && !errors.Is(err, config.ErrNotFound) {
		return nil, []string{err.Error()}, err
	}

	var flamework *config.FlameworkConfig
	if err == nil {
		if validationErrors := cfg.ValidateFlamework(); len(validationErrors) > 0 {
			messages := make([]string, len(validationErrors))
			for index, validationError := range validationErrors {
				messages[index] = "config: " + validationError.Error()
			}
			return nil, messages, errors.New("compile: invalid Flamework configuration")
		}
		flamework, err = selectFlameworkProfile(dir, parsed, cfg.Flamework)
		if err != nil {
			return nil, []string{err.Error()}, err
		}
	}

	if projectUsesLegacyFlameworkTransformer(parsed) {
		if flamework != nil {
			message := "[flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json"
			return nil, []string{message}, errors.New(message)
		}
	}
	return flamework, nil, nil
}

func selectFlameworkProfile(dir string, parsed *tsoptions.ParsedCommandLine, flamework *config.FlameworkConfig) (*config.FlameworkConfig, error) {
	if flamework == nil || len(flamework.Profiles) == 0 {
		return flamework, nil
	}
	if parsed == nil || parsed.ConfigName() == "" {
		return nil, errors.New("config: flamework profiles require an active tsconfig")
	}
	configPath := filepath.FromSlash(parsed.ConfigName())
	if !filepath.IsAbs(configPath) {
		configPath = filepath.Join(dir, configPath)
	}
	relative, err := filepath.Rel(dir, configPath)
	if err != nil {
		return nil, fmt.Errorf("config: resolve active tsconfig profile: %w", err)
	}
	active := normalizeFlameworkProfilePath(relative)
	for name, profile := range flamework.Profiles {
		if normalizeFlameworkProfilePath(name) == active {
			return &config.FlameworkConfig{
				After:                 profile.After,
				NoSemanticDiagnostics: profile.NoSemanticDiagnostics,
				Obfuscation:           profile.Obfuscation,
				IDGenerationMode:      profile.IDGenerationMode,
				HashPrefix:            profile.HashPrefix,
				Salt:                  profile.Salt,
				PreloadIDs:            profile.PreloadIDs,
				SkipUnchangedFiles:    profile.SkipUnchangedFiles,
				Optimizations:         profile.Optimizations,
			}, nil
		}
	}
	return nil, nil
}

func normalizeFlameworkProfilePath(path string) string {
	return filepath.ToSlash(filepath.Clean(filepath.FromSlash(strings.ReplaceAll(path, "\\", "/"))))
}

func projectUsesLegacyFlameworkTransformer(parsed *tsoptions.ParsedCommandLine) bool {
	if parsed == nil || parsed.ConfigName() == "" {
		return false
	}
	plugins, err := effectiveTransformerPluginsFromSnapshot(normalizeSourceFilePath(parsed.ConfigName()), parsed.ConfigParseSnapshot())
	if err != nil {
		return false
	}
	for _, plugin := range plugins {
		if plugin.Transform == legacyFlameworkTransformer {
			return true
		}
	}
	return false
}
