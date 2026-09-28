package compile

import (
	"encoding/json"
	"testing"
)

func TestRewriteSourceMapPaths(t *testing.T) {
	tests := []struct {
		name          string
		generatedFile string
		sourceFile    string
		wantFile      string
		wantSource    string
	}{
		{
			name:          "same Windows volume",
			generatedFile: `D:\checkout\packages\core\out\core\action-state.luau`,
			sourceFile:    `D:\checkout\packages\core\src\core\action-state.ts`,
			wantFile:      "action-state.luau",
			wantSource:    "../../src/core/action-state.ts",
		},
		{
			name:          "different Windows volume",
			generatedFile: `D:\checkout\out\main.luau`,
			sourceFile:    `C:\sources\main.ts`,
			wantFile:      "main.luau",
			wantSource:    "file:///C:/sources/main.ts",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			raw := `{"version":3,"file":"wrong.luau","sources":[` + quoteJSON(t, test.sourceFile) + `],"sourcesContent":["source"],"mappings":"AAAA"}`
			rewritten, err := rewriteSourceMapPaths(raw, test.generatedFile)
			if err != nil {
				t.Fatal(err)
			}
			var sourceMap rawSourceMap
			if err := json.Unmarshal([]byte(rewritten), &sourceMap); err != nil {
				t.Fatal(err)
			}
			if sourceMap.File != test.wantFile {
				t.Errorf("file = %q, want %q", sourceMap.File, test.wantFile)
			}
			if len(sourceMap.Sources) != 1 || sourceMap.Sources[0] != test.wantSource {
				t.Errorf("sources = %v, want [%s]", sourceMap.Sources, test.wantSource)
			}
		})
	}
}

func quoteJSON(t *testing.T, value string) string {
	t.Helper()
	encoded, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return string(encoded)
}
