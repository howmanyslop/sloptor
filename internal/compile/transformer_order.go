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
	"rotor/tsgo/vfs/wrapvfs"
)

type transformerPluginConfig struct {
	Transform string
	raw       json.RawMessage
}

func effectiveTransformerPlugins(configPath string) ([]transformerPluginConfig, error) {
	plugins, _, err := resolveTransformerPlugins(filepath.Clean(configPath), make(map[string]bool), nil)
	return plugins, err
}

// effectiveTransformerPluginsFromSnapshot resolves the exact config graph that
// built a Program. It rejects a config that was not part of that parse instead
// of mixing a later disk edit into the sidecar request.
func effectiveTransformerPluginsFromSnapshot(configPath string, snapshot map[string]string) ([]transformerPluginConfig, error) {
	plugins, _, err := resolveTransformerPlugins(filepath.Clean(configPath), make(map[string]bool), snapshot)
	return plugins, err
}

func resolveTransformerPlugins(configPath string, active map[string]bool, snapshot map[string]string) ([]transformerPluginConfig, bool, error) {
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

	data, err := readTransformerConfig(absPath, snapshot)
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
		basePath, err := resolveExtendedConfig(absPath, extended, snapshot)
		if err != nil {
			return nil, false, err
		}
		plugins, declared, err := resolveTransformerPlugins(basePath, active, snapshot)
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

func readTransformerConfig(path string, snapshot map[string]string) ([]byte, error) {
	if snapshot == nil {
		return os.ReadFile(path)
	}
	text, ok := snapshot[filepath.ToSlash(filepath.Clean(path))]
	if !ok {
		return nil, fmt.Errorf("config was not captured by the parsed program")
	}
	return []byte(text), nil
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

func resolveExtendedConfig(configPath, extended string, snapshot map[string]string) (string, error) {
	if extended == "" {
		return "", fmt.Errorf("%s: extends path is empty", configPath)
	}
	if !filepath.IsAbs(extended) && !strings.HasPrefix(extended, ".") {
		host := configResolutionHost{currentDirectory: filepath.Dir(configPath), fs: configSnapshotFS(snapshot)}
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
	if _, err := readTransformerConfig(path, snapshot); err != nil {
		return "", fmt.Errorf("%s: resolve extends %q: %w", configPath, extended, err)
	}
	return filepath.Clean(path), nil
}

// configSnapshotFS supplies the filesystem operations used by ResolveConfig.
// Its package resolution rules stay with the same resolver used by the parser.
func configSnapshotFS(snapshot map[string]string) vfs.FS {
	base := osvfs.FS()
	if snapshot == nil {
		return base
	}
	caseSensitive := base.UseCaseSensitiveFileNames()
	normalize := func(path string) string { return normalizeOverlayPath(path, caseSensitive) }
	files := make(map[string]string, len(snapshot))
	directories := make(map[string]struct{})
	for path, text := range snapshot {
		files[normalize(path)] = text
		for directory := filepath.Dir(filepath.FromSlash(path)); ; directory = filepath.Dir(directory) {
			directories[normalize(directory)] = struct{}{}
			if parent := filepath.Dir(directory); parent == directory {
				break
			}
		}
	}
	return wrapvfs.Wrap(base, wrapvfs.Replacements{
		FileExists: func(path string) bool {
			_, exists := files[normalize(path)]
			return exists
		},
		ReadFile: func(path string) (string, bool) {
			text, exists := files[normalize(path)]
			return text, exists
		},
		DirectoryExists: func(path string) bool {
			_, exists := directories[normalize(path)]
			return exists
		},
		Realpath: func(path string) string { return filepath.ToSlash(filepath.Clean(path)) },
	})
}

type configResolutionHost struct {
	currentDirectory string
	fs               vfs.FS
}

func (h configResolutionHost) FS() vfs.FS {
	if h.fs != nil {
		return h.fs
	}
	return osvfs.FS()
}

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
