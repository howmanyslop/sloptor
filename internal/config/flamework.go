package config

import (
	"fmt"
	"path"
	"strings"

	"github.com/BurntSushi/toml"
)

type FlameworkConfig struct {
	After                 string                      `json:"after,omitempty" toml:"after,omitempty"`
	NoSemanticDiagnostics bool                        `json:"noSemanticDiagnostics,omitempty" toml:"noSemanticDiagnostics,omitempty"`
	Obfuscation           bool                        `json:"obfuscation,omitempty" toml:"obfuscation,omitempty"`
	IDGenerationMode      string                      `json:"idGenerationMode,omitempty" toml:"idGenerationMode,omitempty"`
	HashPrefix            string                      `json:"hashPrefix,omitempty" toml:"hashPrefix,omitempty"`
	Salt                  string                      `json:"salt,omitempty" toml:"salt,omitempty"`
	PreloadIDs            bool                        `json:"preloadIds,omitempty" toml:"preloadIds,omitempty"`
	Optimizations         FlameworkOptimizations      `json:"optimizations,omitempty" toml:"optimizations,omitempty"`
	Profiles              map[string]FlameworkProfile `json:"profiles,omitempty" toml:"profiles,omitempty"`
}

type FlameworkProfile struct {
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
	if c.Flamework == nil {
		return
	}
	for name, profile := range c.Flamework.Profiles {
		profile.IDGenerationMode = defaultFlameworkIDGenerationMode(profile.IDGenerationMode, profile.Obfuscation)
		c.Flamework.Profiles[name] = profile
	}
	if len(c.Flamework.Profiles) == 0 {
		c.Flamework.IDGenerationMode = defaultFlameworkIDGenerationMode(c.Flamework.IDGenerationMode, c.Flamework.Obfuscation)
	}
}

func defaultFlameworkIDGenerationMode(mode string, obfuscation bool) string {
	if mode != "" {
		return mode
	}
	if obfuscation {
		return "obfuscated"
	}
	return "full"
}

func (c *Config) ValidateFlamework() []error {
	if c.Flamework == nil {
		return nil
	}
	var errs []error
	if len(c.Flamework.Profiles) > 0 && hasGlobalFlameworkOptions(c.Flamework) {
		errs = append(errs, fmt.Errorf("flamework: global options cannot be combined with flamework.profiles"))
	}
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
	normalizedNames := make(map[string]string, len(c.Flamework.Profiles))
	for name, profile := range c.Flamework.Profiles {
		normalized := path.Clean(strings.ReplaceAll(name, "\\", "/"))
		if normalized == "." || path.IsAbs(normalized) || normalized == ".." || strings.HasPrefix(normalized, "../") {
			errs = append(errs, fmt.Errorf("flamework.profiles.%s must be a project-relative tsconfig path", name))
		} else if previous, exists := normalizedNames[normalized]; exists {
			errs = append(errs, fmt.Errorf("flamework profiles %q and %q resolve to the same tsconfig", previous, name))
		} else {
			normalizedNames[normalized] = name
		}
		errs = append(errs, validateFlameworkProfile(name, profile)...)
	}
	return errs
}

func hasGlobalFlameworkOptions(flamework *FlameworkConfig) bool {
	return flamework.After != "" || flamework.NoSemanticDiagnostics || flamework.Obfuscation ||
		flamework.IDGenerationMode != "" || flamework.HashPrefix != "" || flamework.Salt != "" ||
		flamework.PreloadIDs || flamework.Optimizations.GuardGenerationDedupLimit != nil
}

func validateFlameworkProfile(name string, profile FlameworkProfile) []error {
	cfg := Config{Flamework: &FlameworkConfig{
		After:                 profile.After,
		NoSemanticDiagnostics: profile.NoSemanticDiagnostics,
		Obfuscation:           profile.Obfuscation,
		IDGenerationMode:      profile.IDGenerationMode,
		HashPrefix:            profile.HashPrefix,
		Salt:                  profile.Salt,
		PreloadIDs:            profile.PreloadIDs,
		Optimizations:         profile.Optimizations,
	}}
	errs := cfg.ValidateFlamework()
	for index, err := range errs {
		errs[index] = fmt.Errorf("flamework.profiles.%s: %w", name, err)
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
