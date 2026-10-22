package compile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/tsgo/sourcemap"
)

func decodeSourceMapLines(t *testing.T, mapJSON string) map[int][]sourcemap.Mapping {
	t.Helper()
	var sourceMap rawSourceMap
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		t.Fatal(err)
	}
	lines := map[int][]sourcemap.Mapping{}
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for mapping := range decoder.Values() {
		lines[mapping.GeneratedLine] = append(lines[mapping.GeneratedLine], *mapping)
	}
	if err := decoder.Error(); err != nil {
		t.Fatal(err)
	}
	return lines
}

func assertSourceMapContract(t *testing.T, text, mapJSON, source string) {
	t.Helper()
	var fields map[string]json.RawMessage
	if err := json.Unmarshal([]byte(mapJSON), &fields); err != nil {
		t.Fatal(err)
	}
	if len(fields) != 5 {
		t.Errorf("source map has %d fields, want exactly five: %v", len(fields), fields)
	}
	for _, key := range []string{"version", "file", "sources", "sourcesContent", "mappings"} {
		if _, ok := fields[key]; !ok {
			t.Errorf("source map missing %s", key)
		}
	}
	var sourceMap rawSourceMap
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		t.Fatal(err)
	}
	if sourceMap.Version != 3 || len(sourceMap.Sources) != 1 || sourceMap.File == "" {
		t.Errorf("invalid source map metadata: %+v", sourceMap)
	}
	if len(sourceMap.SourcesContent) != 1 || sourceMap.SourcesContent[0] != source {
		t.Errorf("sourcesContent = %q, want original bytes %q", sourceMap.SourcesContent, source)
	}
	lines := strings.Split(strings.TrimSuffix(text, "\n"), "\n")
	exportClose := -1
	for line, content := range lines {
		if content == "return {" {
			for end := line + 1; end < len(lines); end++ {
				if lines[end] == "}" {
					exportClose = end
					break
				}
			}
		}
	}
	for line, mappings := range decodeSourceMapLines(t, mapJSON) {
		if line < 0 || line >= len(lines) {
			t.Errorf("segment on nonexistent Luau line %d", line+1)
			continue
		}
		if len(mappings) != 1 || mappings[0].GeneratedCharacter != 0 {
			t.Errorf("Luau line %d: want one column-zero segment, got %+v", line+1, mappings)
		}
		// Only top-level wrappers are excluded; a nested return/table close is
		// executable output that must keep its mapping.
		if strings.HasPrefix(lines[line], "--") || strings.HasPrefix(lines[line], "local TS = ") ||
			lines[line] == "local exports = {}" || lines[line] == "return nil" ||
			lines[line] == "return exports" || lines[line] == "return {" || line == exportClose {
			t.Errorf("compiler header/export wrapper at Luau line %d is mapped: %q", line+1, lines[line])
		}
	}
}

func TestSourceMapRuntimeHeaders(t *testing.T) {
	for _, projectType := range []string{"game", "model", "package"} {
		t.Run(projectType, func(t *testing.T) {
			dir := t.TempDir()
			copyDir(t, filepath.Join("testdata", "runtimelib_"+projectType), dir)
			// Exercise runtime import, mutable exports, immutable export fields,
			// and return-nil/wrapper shapes in each project type.
			for name, source := range map[string]string{
				"main":      "declare class Base {}\nexport let current: unknown = undefined;\nexport function isBase(value: unknown) {\n\treturn value instanceof Base;\n}\n",
				"immutable": "export const value = 1;\n",
				"empty":     "export {};\n",
			} {
				if err := os.WriteFile(filepath.Join(dir, "src", name+".ts"), []byte(source), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			configPath := filepath.Join(dir, "tsconfig.json")
			config, err := os.ReadFile(configPath)
			if err != nil {
				t.Fatal(err)
			}
			withMaps := strings.Replace(string(config), `"outDir": "out"`, `"outDir": "out", "sourceMap": true`, 1)
			if err := os.WriteFile(configPath, []byte(withMaps), 0o644); err != nil {
				t.Fatal(err)
			}
			result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
			if err != nil || len(diags) != 0 {
				t.Fatalf("build: %v (diagnostics: %v)", err, diags)
			}
			for _, name := range []string{"main", "immutable", "empty"} {
				text := result.Outputs["out/"+name+".luau"]
				if name == "main" && (!strings.Contains(text, "local TS = ") || !strings.Contains(text, "local exports = {}")) {
					t.Fatalf("fixture did not exercise runtime import and exports table:\n%s", text)
				}
				source, err := os.ReadFile(filepath.Join(dir, "src", name+".ts"))
				if err != nil {
					t.Fatal(err)
				}
				mapBytes, err := os.ReadFile(filepath.Join(dir, "out", name+".luau.map"))
				if err != nil {
					t.Fatal(err)
				}
				assertSourceMapContract(t, text, string(mapBytes), string(source))
				mappings := decodeSourceMapLines(t, string(mapBytes))
				for line, content := range strings.Split(text, "\n") {
					if strings.Contains(content, "exports.current = nil") || strings.TrimSpace(content) == "value = value," {
						if len(mappings[line]) != 1 || !mappings[line][0].IsSourceMapping() {
							t.Errorf("export value at line %d lost its source mapping", line+1)
						}
					}
				}
			}
		})
	}
}
