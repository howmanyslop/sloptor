package compile

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"rotor/tsgo/module"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/osvfs"
)

type transformerPluginConfig struct {
	Transform string
	raw       json.RawMessage
}

func effectiveTransformerPlugins(configPath string) ([]transformerPluginConfig, error) {
	plugins, _, err := resolveTransformerPlugins(filepath.Clean(configPath), make(map[string]bool))
	return plugins, err
}

func resolveTransformerPlugins(configPath string, active map[string]bool) ([]transformerPluginConfig, bool, error) {
	absPath, err := filepath.Abs(configPath)
	if err != nil {
		return nil, false, fmt.Errorf("resolve tsconfig path %q: %w", configPath, err)
	}
	absPath = filepath.Clean(absPath)
	if active[absPath] {
		return nil, false, fmt.Errorf("%s: circular tsconfig extends chain", absPath)
	}
	active[absPath] = true
	defer delete(active, absPath)

	data, err := os.ReadFile(absPath)
	if err != nil {
		return nil, false, fmt.Errorf("%s: read extended tsconfig: %w", absPath, err)
	}
	var root map[string]json.RawMessage
	if err := json.Unmarshal([]byte(stripJSONC(string(data))), &root); err != nil {
		return nil, false, fmt.Errorf("%s: parse tsconfig: %w", absPath, err)
	}

	var inherited []transformerPluginConfig
	inheritedDeclared := false
	extends, err := parseExtends(root["extends"])
	if err != nil {
		return nil, false, fmt.Errorf("%s: %w", absPath, err)
	}
	for _, extended := range extends {
		basePath, err := resolveExtendedConfig(absPath, extended)
		if err != nil {
			return nil, false, err
		}
		plugins, declared, err := resolveTransformerPlugins(basePath, active)
		if err != nil {
			return nil, false, err
		}
		if declared {
			inherited = plugins
			inheritedDeclared = true
		}
	}

	compilerOptionsRaw, exists := root["compilerOptions"]
	if !exists {
		return inherited, inheritedDeclared, nil
	}
	var compilerOptions map[string]json.RawMessage
	if err := json.Unmarshal(compilerOptionsRaw, &compilerOptions); err != nil {
		return nil, false, fmt.Errorf("%s: compilerOptions must be an object", absPath)
	}
	pluginsRaw, declared := compilerOptions["plugins"]
	if !declared {
		return inherited, inheritedDeclared, nil
	}
	var entries []json.RawMessage
	if err := json.Unmarshal(pluginsRaw, &entries); err != nil {
		return nil, false, fmt.Errorf("%s: compilerOptions.plugins must be an array", absPath)
	}
	plugins := make([]transformerPluginConfig, 0, len(entries))
	for _, entry := range entries {
		var header struct {
			Transform string `json:"transform"`
		}
		if json.Unmarshal(entry, &header) != nil || header.Transform == "" {
			continue
		}
		plugins = append(plugins, transformerPluginConfig{Transform: header.Transform, raw: append(json.RawMessage(nil), entry...)})
	}
	return plugins, true, nil
}

func parseExtends(raw json.RawMessage) ([]string, error) {
	if len(raw) == 0 {
		return nil, nil
	}
	var single string
	if json.Unmarshal(raw, &single) == nil {
		return []string{single}, nil
	}
	var multiple []string
	if json.Unmarshal(raw, &multiple) == nil {
		return multiple, nil
	}
	return nil, errors.New("extends must be a string or array of strings")
}

func resolveExtendedConfig(configPath, extended string) (string, error) {
	if extended == "" {
		return "", fmt.Errorf("%s: extends path is empty", configPath)
	}
	if !filepath.IsAbs(extended) && !strings.HasPrefix(extended, ".") {
		host := configResolutionHost{currentDirectory: filepath.Dir(configPath)}
		resolved := module.ResolveConfig(filepath.ToSlash(extended), filepath.ToSlash(configPath), host)
		if !resolved.IsResolved() {
			return "", fmt.Errorf("%s: resolve extends %q: config not found", configPath, extended)
		}
		return filepath.Clean(filepath.FromSlash(resolved.ResolvedFileName)), nil
	}
	path := extended
	if !filepath.IsAbs(path) {
		path = filepath.Join(filepath.Dir(configPath), filepath.FromSlash(path))
	}
	if filepath.Ext(path) == "" {
		path += ".json"
	}
	if _, err := os.Stat(path); err != nil {
		return "", fmt.Errorf("%s: resolve extends %q: %w", configPath, extended, err)
	}
	return filepath.Clean(path), nil
}

type configResolutionHost struct {
	currentDirectory string
}

func (h configResolutionHost) FS() vfs.FS { return osvfs.FS() }

func (h configResolutionHost) GetCurrentDirectory() string {
	return filepath.ToSlash(h.currentDirectory)
}

func splitTransformerPlugins(plugins []transformerPluginConfig, after string) ([]transformerPluginConfig, []transformerPluginConfig, error) {
	if after == "" {
		return nil, append([]transformerPluginConfig(nil), plugins...), nil
	}
	if after == legacyFlameworkTransformer {
		return nil, nil, fmt.Errorf("flamework.after cannot anchor native Flamework to itself: %q", after)
	}
	anchor := -1
	matches := 0
	for index, plugin := range plugins {
		if plugin.Transform == after {
			anchor = index
			matches++
		}
	}
	if matches == 0 {
		return nil, nil, fmt.Errorf("flamework.after %q does not match an effective tsconfig transformer plugin", after)
	}
	if matches != 1 {
		return nil, nil, fmt.Errorf("flamework.after %q matches %d effective tsconfig transformer plugins; anchor must be unique", after, matches)
	}
	return append([]transformerPluginConfig(nil), plugins[:anchor+1]...), append([]transformerPluginConfig(nil), plugins[anchor+1:]...), nil
}

func (p transformerPluginConfig) marshalJSON() json.RawMessage {
	if len(p.raw) > 0 {
		return append(json.RawMessage(nil), p.raw...)
	}
	encoded, _ := json.Marshal(struct {
		Transform string `json:"transform"`
	}{Transform: p.Transform})
	return encoded
}
