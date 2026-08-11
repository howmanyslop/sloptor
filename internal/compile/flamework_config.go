package compile

import (
	"errors"
	"slices"

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
		flamework = cfg.Flamework
		if validationErrors := cfg.ValidateFlamework(); len(validationErrors) > 0 {
			messages := make([]string, len(validationErrors))
			for index, validationError := range validationErrors {
				messages[index] = "config: " + validationError.Error()
			}
			return nil, messages, errors.New("compile: invalid Flamework configuration")
		}
	}

	if projectUsesLegacyFlameworkTransformer(parsed) {
		if flamework != nil {
			message := "[flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json"
			return nil, []string{message}, errors.New(message)
		}
		message := "rbxts-transformer-flamework is no longer supported; run `sloptor migrate flamework`"
		return nil, []string{message}, errors.New(message)
	}
	return flamework, nil, nil
}

func projectUsesLegacyFlameworkTransformer(parsed *tsoptions.ParsedCommandLine) bool {
	if parsed == nil {
		return false
	}
	if configName := parsed.ConfigName(); configName != "" {
		plugins, declared := configFileTransformerPlugins(normalizeSourceFilePath(configName))
		if declared {
			return slices.Contains(plugins, legacyFlameworkTransformer)
		}
	}
	for _, configPath := range parsed.ExtendedSourceFiles() {
		plugins, _ := configFileTransformerPlugins(normalizeSourceFilePath(configPath))
		if slices.Contains(plugins, legacyFlameworkTransformer) {
			return true
		}
	}
	return false
}
