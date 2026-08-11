package migrate

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"rotor/tsgo/module"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/osvfs"
)

func resolveExtendsPath(configPath, value string) (string, error) {
	if value == "" {
		return "", fmt.Errorf("extends path is empty")
	}
	if !filepath.IsAbs(value) && !isRelativeExtendsPath(value) {
		host := migrationConfigResolutionHost{currentDirectory: filepath.Dir(configPath)}
		resolved := module.ResolveConfig(filepath.ToSlash(value), filepath.ToSlash(configPath), host)
		if !resolved.IsResolved() {
			return "", fmt.Errorf("cannot resolve package extends %q", value)
		}
		return filepath.Clean(filepath.FromSlash(resolved.ResolvedFileName)), nil
	}
	candidate := value
	if !filepath.IsAbs(value) {
		candidate = filepath.Join(filepath.Dir(configPath), filepath.FromSlash(value))
	}
	if resolved, ok := resolveConfigFile(candidate); ok {
		return resolved, nil
	}
	return "", fmt.Errorf("missing extends config %q", filepath.Clean(candidate))
}

func isRelativeExtendsPath(path string) bool {
	return path == "." || path == ".." || strings.HasPrefix(path, "./") || strings.HasPrefix(path, "../") || strings.HasPrefix(path, `.\`) || strings.HasPrefix(path, `..\`)
}

func resolveConfigFile(path string) (string, bool) {
	for _, candidate := range []string{path, path + ".json"} {
		if info, err := os.Stat(candidate); err == nil && info.Mode().IsRegular() {
			return filepath.Clean(candidate), true
		}
	}
	data, err := os.ReadFile(filepath.Join(path, "package.json"))
	if err != nil {
		return "", false
	}
	var pkg struct {
		Main string `json:"main"`
	}
	if json.Unmarshal(data, &pkg) != nil || pkg.Main == "" {
		return "", false
	}
	return resolveConfigFile(filepath.Join(path, filepath.FromSlash(pkg.Main)))
}

type migrationConfigResolutionHost struct{ currentDirectory string }

func (h migrationConfigResolutionHost) FS() vfs.FS { return osvfs.FS() }

func (h migrationConfigResolutionHost) GetCurrentDirectory() string {
	return filepath.ToSlash(h.currentDirectory)
}

func findProperty(object *jsoncNode, key string) *jsoncProperty {
	for index := range object.properties {
		if object.properties[index].key == key {
			return &object.properties[index]
		}
	}
	return nil
}

func objectUsesTrailingComma(object *jsoncNode) bool { return object.trailingComma }

func decodeString(document *jsoncDocument, node *jsoncNode) (string, error) {
	if node.kind != jsoncString {
		return "", &JSONCMigrationError{Path: document.path, Reason: "expected string option"}
	}
	var value string
	if err := json.Unmarshal(document.raw[node.start:node.end], &value); err != nil {
		return "", &JSONCMigrationError{Path: document.path, Reason: fmt.Sprintf("decode string: %v", err)}
	}
	return value, nil
}

func decodeBool(document *jsoncDocument, node *jsoncNode) (bool, error) {
	if node.kind != jsoncBool {
		return false, &JSONCMigrationError{Path: document.path, Reason: "expected boolean option"}
	}
	return string(document.raw[node.start:node.end]) == "true", nil
}

func detectNewline(raw []byte) string {
	if bytes.Contains(raw, []byte("\r\n")) {
		return "\r\n"
	}
	return "\n"
}

func detectIndentUnit(raw []byte) string {
	lines := bytes.Split(raw, []byte("\n"))
	for _, line := range lines {
		trimmed := bytes.TrimLeft(line, " \t\r")
		indent := line[:len(line)-len(trimmed)]
		indent = bytes.TrimSuffix(indent, []byte("\r"))
		if len(indent) > 0 {
			return string(indent)
		}
	}
	return "\t"
}

func lineIndent(raw []byte, position int) string {
	start := bytes.LastIndexByte(raw[:position], '\n') + 1
	end := start
	for end < position && (raw[end] == ' ' || raw[end] == '\t') {
		end++
	}
	return string(raw[start:end])
}

func replaceBytes(raw []byte, start, end int, replacement []byte) []byte {
	result := make([]byte, 0, len(raw)-(end-start)+len(replacement))
	result = append(result, raw[:start]...)
	result = append(result, replacement...)
	result = append(result, raw[end:]...)
	return result
}
