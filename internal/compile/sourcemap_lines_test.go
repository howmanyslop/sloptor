package compile

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/tsgo/sourcemap"
)

// compileWithSourceMap builds a package fixture and reads the emitted artifacts.
// name is the source file's stem, without its .ts extension.
func compileWithSourceMap(t *testing.T, name, source string) (luau, mapJSON string) {
	t.Helper()
	dir := writeProject(t, "@scope/source-map-"+name, "")
	sourcePath := filepath.Join(dir, "src", name+".ts")
	if name != "main" {
		if err := os.Rename(filepath.Join(dir, "src", "main.ts"), sourcePath); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(sourcePath, []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfig, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	withMaps := strings.Replace(string(tsconfig), `"outDir": "out"`, `"outDir": "out", "sourceMap": true`, 1)
	if err := os.WriteFile(tsconfigPath, []byte(withMaps), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions: %v (diagnostics: %v)", err, diags)
	}
	read := func(suffix string) string {
		t.Helper()
		text, err := os.ReadFile(filepath.Join(dir, "out", name+suffix))
		if err != nil {
			t.Fatal(err)
		}
		return string(text)
	}
	return read(".luau"), read(".luau.map")
}

// annotateSourceMap uses 1-based lines and 0-based UTF-16 columns. It retains
// every segment, including generated-only segments and duplicate mappings.
func annotateSourceMap(luauText, mapJSON string) string {
	var sourceMap struct {
		Mappings string `json:"mappings"`
	}
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		panic(fmt.Errorf("decode source map: %w", err))
	}
	segments := map[int][]string{}
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for mapping := range decoder.Values() {
		segment := fmt.Sprintf("%d->?", mapping.GeneratedCharacter)
		if mapping.IsSourceMapping() {
			segment = fmt.Sprintf("%d->%d:%d", mapping.GeneratedCharacter, mapping.SourceLine+1, mapping.SourceCharacter)
		}
		segments[mapping.GeneratedLine] = append(segments[mapping.GeneratedLine], segment)
	}
	if err := decoder.Error(); err != nil {
		panic(fmt.Errorf("decode source map mappings: %w", err))
	}
	lines := strings.Split(strings.TrimSuffix(luauText, "\n"), "\n")
	annotations := make([]string, len(lines))
	segmentWidth := 1
	for line := range lines {
		annotation := strings.Join(segments[line], " ")
		if annotation == "" {
			annotation = "-"
		}
		annotations[line] = annotation
		segmentWidth = max(segmentWidth, len(annotation))
	}
	lineWidth := max(2, len(fmt.Sprint(len(lines))))
	var annotated strings.Builder
	for line, text := range lines {
		if strings.HasPrefix(text, "-- Compiled with sloptor v") {
			text = "-- Compiled with sloptor vX"
		}
		fmt.Fprintf(&annotated, "%*d %-*s | %s\n", lineWidth, line+1, segmentWidth, annotations[line], text)
	}
	return annotated.String()
}

func TestSourceMapLines(t *testing.T) {
	tests := []struct {
		name   string
		source string
		want   string
	}{
		{
			name: "header",
			source: `declare class Base {}
export let current: unknown = undefined;
export function isBase(value: unknown) {
	return value instanceof Base;
}
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 -      | local TS = _G[script]
 3 -      | local exports = {}
 4 0->2:0 | exports.current = nil
 5 0->3:0 | local function isBase(value)
 6 0->4:1 | 	return TS.instanceof(value, Base)
 7 0->5:0 | end
 8 0->3:0 | exports.isBase = isBase
 9 -      | return exports
`,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			luau, mapJSON := compileWithSourceMap(t, test.name, test.source)
			if got := annotateSourceMap(luau, mapJSON); got != test.want {
				t.Errorf("source map lines:\ngot:\n%swant:\n%s", got, test.want)
			}
		})
	}
}
