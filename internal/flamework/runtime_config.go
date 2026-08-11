package flamework

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
)

const RuntimeConfigFileName = "flamework.json"

var ErrInvalidRuntimeConfig = errors.New("flamework: invalid flamework.json")

type RuntimeLogLevel string

const (
	RuntimeLogLevelNone    RuntimeLogLevel = "none"
	RuntimeLogLevelVerbose RuntimeLogLevel = "verbose"
)

// RuntimeConfig is the runtime configuration exposed through flamework.json.
// Pointer fields preserve absent values separately from explicit zero values.
type RuntimeConfig struct {
	LogLevel                  *RuntimeLogLevel
	Profiling                 *bool
	DisableDependencyWarnings *bool
	additional                map[string]json.RawMessage
}

// LoadRuntimeConfig loads <root>/flamework.json. The boolean distinguishes an
// absent file from a present empty object, matching upstream build metadata.
func LoadRuntimeConfig(root string) (RuntimeConfig, bool, error) {
	configPath := filepath.Join(root, RuntimeConfigFileName)
	data, err := os.ReadFile(configPath)
	if errors.Is(err, os.ErrNotExist) {
		return RuntimeConfig{}, false, nil
	}
	if err != nil {
		return RuntimeConfig{}, false, fmt.Errorf("flamework: read %s: %w", RuntimeConfigFileName, err)
	}
	config, err := parseRuntimeConfig(data)
	if err != nil {
		return RuntimeConfig{}, true, err
	}
	return config, true, nil
}

func parseRuntimeConfig(data []byte) (RuntimeConfig, error) {
	if err := validateJSONSyntax(data); err != nil {
		return RuntimeConfig{}, fmt.Errorf("%w: %w", ErrInvalidRuntimeConfig, err)
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	var properties map[string]json.RawMessage
	if err := decoder.Decode(&properties); err != nil || properties == nil {
		return RuntimeConfig{}, fmt.Errorf("%w: expected JSON object", ErrInvalidRuntimeConfig)
	}
	var trailing json.RawMessage
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		return RuntimeConfig{}, fmt.Errorf("%w: trailing JSON value", ErrInvalidRuntimeConfig)
	}

	config := RuntimeConfig{additional: make(map[string]json.RawMessage)}
	for name, raw := range properties {
		switch name {
		case "logLevel":
			var level RuntimeLogLevel
			if err := json.Unmarshal(raw, &level); err != nil || (level != RuntimeLogLevelNone && level != RuntimeLogLevelVerbose) {
				return RuntimeConfig{}, fmt.Errorf("%w: logLevel must be none or verbose", ErrInvalidRuntimeConfig)
			}
			config.LogLevel = &level
		case "profiling":
			var value *bool
			if err := json.Unmarshal(raw, &value); err != nil || value == nil {
				return RuntimeConfig{}, fmt.Errorf("%w: profiling must be boolean", ErrInvalidRuntimeConfig)
			}
			config.Profiling = value
		case "disableDependencyWarnings":
			var value *bool
			if err := json.Unmarshal(raw, &value); err != nil || value == nil {
				return RuntimeConfig{}, fmt.Errorf("%w: disableDependencyWarnings must be boolean", ErrInvalidRuntimeConfig)
			}
			config.DisableDependencyWarnings = value
		default:
			config.additional[name] = append(json.RawMessage(nil), raw...)
		}
	}
	return config, nil
}

// MarshalJSON retains schema-permitted additional properties and emits map
// keys deterministically through encoding/json.
func (c RuntimeConfig) MarshalJSON() ([]byte, error) {
	properties := make(map[string]json.RawMessage, len(c.additional)+3)
	for name, raw := range c.additional {
		properties[name] = append(json.RawMessage(nil), raw...)
	}
	if c.LogLevel != nil {
		properties["logLevel"] = json.RawMessage(strconv.Quote(string(*c.LogLevel)))
	}
	if c.Profiling != nil {
		properties["profiling"] = json.RawMessage(strconv.FormatBool(*c.Profiling))
	}
	if c.DisableDependencyWarnings != nil {
		properties["disableDependencyWarnings"] = json.RawMessage(strconv.FormatBool(*c.DisableDependencyWarnings))
	}
	return json.Marshal(properties)
}

// MarshalRuntimeConfigArtifact builds deterministic
// include/flamework/config.json bytes without writing them.
func MarshalRuntimeConfigArtifact(game *RuntimeConfig, packages map[string]RuntimeConfig) ([]byte, bool, error) {
	if game == nil && len(packages) == 0 {
		return nil, false, nil
	}
	payload := struct {
		Game     *RuntimeConfig           `json:"game,omitempty"`
		Packages map[string]RuntimeConfig `json:"packages"`
	}{Game: game, Packages: packages}
	if payload.Packages == nil {
		payload.Packages = map[string]RuntimeConfig{}
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return nil, false, fmt.Errorf("flamework: marshal runtime config artifact: %w", err)
	}
	return data, true, nil
}
