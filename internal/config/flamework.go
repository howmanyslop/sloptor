package config

import (
	"fmt"
	"strings"

	"github.com/BurntSushi/toml"
)

type FlameworkConfig struct {
	After                 string                 `json:"after,omitempty" toml:"after,omitempty"`
	NoSemanticDiagnostics bool                   `json:"noSemanticDiagnostics,omitempty" toml:"noSemanticDiagnostics,omitempty"`
	Obfuscation           bool                   `json:"obfuscation,omitempty" toml:"obfuscation,omitempty"`
	IDGenerationMode      string                 `json:"idGenerationMode,omitempty" toml:"idGenerationMode,omitempty"`
	HashPrefix            string                 `json:"hashPrefix,omitempty" toml:"hashPrefix,omitempty"`
	Salt                  string                 `json:"salt,omitempty" toml:"salt,omitempty"`
	PreloadIDs            bool                   `json:"preloadIds,omitempty" toml:"preloadIds,omitempty"`
	Optimizations         FlameworkOptimizations `json:"optimizations,omitempty" toml:"optimizations,omitempty"`
}

type FlameworkOptimizations struct {
	GuardGenerationDedupLimit *int `json:"guardGenerationDedupLimit,omitempty" toml:"guardGenerationDedupLimit,omitempty"`
}

func (c *Config) applyFlameworkDefaults() {
	if c.Flamework == nil || c.Flamework.IDGenerationMode != "" {
		return
	}
	if c.Flamework.Obfuscation {
		c.Flamework.IDGenerationMode = "obfuscated"
		return
	}
	c.Flamework.IDGenerationMode = "full"
}

func (c *Config) ValidateFlamework() []error {
	if c.Flamework == nil {
		return nil
	}
	var errs []error
	switch c.Flamework.IDGenerationMode {
	case "", "full", "obfuscated", "short", "tiny":
	default:
		errs = append(errs, fmt.Errorf("flamework.idGenerationMode must be one of full, obfuscated, short, tiny; got %q", c.Flamework.IDGenerationMode))
	}
	if strings.HasPrefix(c.Flamework.HashPrefix, "$") {
		errs = append(errs, fmt.Errorf("flamework.hashPrefix must not start with reserved prefix \"$\": %q", c.Flamework.HashPrefix))
	}
	if limit := c.Flamework.Optimizations.GuardGenerationDedupLimit; limit != nil && *limit < 0 {
		errs = append(errs, fmt.Errorf("flamework.optimizations.guardGenerationDedupLimit must be >= 0, got %d", *limit))
	}
	return errs
}

func validateFlameworkTOMLKeys(keys []toml.Key) error {
	for _, key := range keys {
		if len(key) > 0 && key[0] == "flamework" {
			return fmt.Errorf("flamework: unknown key %q", key.String())
		}
	}
	return nil
}
