package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"

	"github.com/BurntSushi/toml"
)

// ErrNotFound is returned by Load when the project has no rotor.toml.
// Callers treat the config as optional.
var ErrNotFound = errors.New("rotor.toml not found")

// ConfigFileName is the primary config file rotor loads.
const ConfigFileName = "rotor.toml"

// Load reads the project's rotor.toml into a *Config.
//
// Decoding is via github.com/BurntSushi/toml into the typed structs (toml
// tags). Unknown keys are tolerated for forward compatibility: BurntSushi's
// MetaData.Undecoded() reports them, and Load records each as a warning on the
// returned Config rather than failing.
//
// Returns (nil, ErrNotFound) when rotor.toml does not exist. Validation
// of compiler settings is left to Config.Validate, exactly as before.
func Load(projectDir string) (*Config, error) {
	absDir, err := filepath.Abs(projectDir)
	if err != nil {
		return nil, fmt.Errorf("config: resolving project dir: %w", err)
	}

	configPath := filepath.Join(absDir, ConfigFileName)
	if info, err := os.Stat(configPath); err != nil || info.IsDir() {
		return nil, ErrNotFound
	}

	data, err := os.ReadFile(configPath)
	if err != nil {
		return nil, fmt.Errorf("config: reading %s: %w", ConfigFileName, err)
	}

	cfg := &Config{}
	meta, err := toml.Decode(string(data), cfg)
	if err != nil {
		return nil, fmt.Errorf("config: %s: %w", ConfigFileName, err)
	}
	if err := validateFlameworkTOMLKeys(meta.Undecoded()); err != nil {
		return nil, fmt.Errorf("config: %s: %w", ConfigFileName, err)
	}
	cfg.applyFlameworkDefaults()

	// Unknown keys are forward-compatible: collect them as sorted warnings,
	// skipping the taplo schema directive that lives as a leading comment (it
	// is not a key, so it never appears here, but keep this robust).
	var warnings []string
	undecoded := meta.Undecoded()
	keys := make([]string, 0, len(undecoded))
	for _, k := range undecoded {
		keys = append(keys, k.String())
	}
	sort.Strings(keys)
	for _, k := range keys {
		warnings = append(warnings, fmt.Sprintf("%s: unknown key %q (ignored)", ConfigFileName, k))
	}
	cfg.Warnings = warnings
	return cfg, nil
}
