package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSourceMapForIteratorFallbackTempOrder(t *testing.T) {
	dir := buildAuditProject(t)
	source := "declare const input: number | undefined;\n" +
		"for (const [key, value] of new Map<() => number, () => number>([\n" +
		"  [() => { const _0 = 10; const n = input ?? 1; return _0 + n; },\n" +
		"   () => input ?? 2],\n" +
		"])) print(key(), value());\n"
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(dir, "tsconfig.json")
	config, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	// These are the legacy bytes: the value callback gets fallback _0 before
	// the key callback gets _1, preserving the authored key-local _0 = 10.
	want := "for key, value in {\n" +
		"\t[function()\n\t\tlocal _0 = 10\n\t\tlocal _1 = input\n" +
		"\t\tif _1 == nil then\n\t\t\t_1 = 1\n\t\tend\n" +
		"\t\tlocal n = _1\n\t\treturn _0 + n\n\tend] = function()\n" +
		"\t\tlocal _0 = input\n\t\tif _0 == nil then\n\t\t\t_0 = 2\n\t\tend\n" +
		"\t\treturn _0\n\tend,\n} do\n\tprint(key(), value())\nend\nreturn nil\n"
	for _, maps := range []bool{false, true} {
		name := "without maps"
		textConfig := string(config)
		if maps {
			name = "with maps"
			textConfig = strings.Replace(textConfig, `"outDir": "out"`, `"outDir": "out", "sourceMap": true`, 1)
		}
		t.Run(name, func(t *testing.T) {
			if err := os.WriteFile(configPath, []byte(textConfig), 0o644); err != nil {
				t.Fatal(err)
			}
			if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
				t.Fatalf("build: %v (diagnostics: %v)", err, diags)
			}
			text, err := os.ReadFile(filepath.Join(dir, "out", "main.luau"))
			if err != nil {
				t.Fatal(err)
			}
			_, body, _ := strings.Cut(string(text), "\n")
			if body != want {
				t.Errorf("iterator callback bytes:\n%s\nwant:\n%s", body, want)
			}
			if maps {
				mapJSON, err := os.ReadFile(filepath.Join(dir, "out", "main.luau.map"))
				if err != nil {
					t.Fatal(err)
				}
				assertSourceMapContract(t, string(text), string(mapJSON), source)
				mappings := decodeSourceMapLines(t, string(mapJSON))
				for line, content := range strings.Split(string(text), "\n") {
					sourceLine := -1
					if strings.Contains(content, "local _1 = input") {
						sourceLine = 2
					} else if strings.Contains(content, "local _0 = input") {
						sourceLine = 3
					}
					if sourceLine >= 0 {
						if len(mappings[line]) == 0 || mappings[line][0].GeneratedCharacter != 0 || !mappings[line][0].IsSourceMapping() || mappings[line][0].SourceLine != sourceLine {
							t.Errorf("callback temporary on Luau line %d: want TS line %d, got %v", line+1, sourceLine+1, mappings[line])
						}
					}
				}
			}
		})
	}
}
