package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSourceMapTraceRetainsGeneratedOnlyLines(t *testing.T) {
	// Three Luau lines initially refer to three reprinted TS lines. The
	// transform has no authored counterpart before its first trace anchor.
	trace, err := newSourceTraceMap(`{"version":3,"mappings":";AAAA;AACA"}`, "/authored.ts", "first();\nlast();\n")
	if err != nil {
		t.Fatal(err)
	}
	raw := `{"version":3,"file":"main.luau","sources":["/printed.ts"],"sourcesContent":["injected();\nfirst();\nlast();\n"],"mappings":"AAAA;AACA;AACA"}`
	rewritten, err := rewriteSourceMapWithTrace(raw, trace)
	if err != nil {
		t.Fatal(err)
	}
	const want = ` 1 0->?   | injected()
 2 0->1:0 | first()
 3 0->2:0 | last()
`
	if got := annotateSourceMap("injected()\nfirst()\nlast()\n", rewritten); got != want {
		t.Errorf("trace rewrite:\ngot:\n%swant:\n%s", got, want)
	}
	assertSourceMapContract(t, "injected()\nfirst()\nlast()\n", rewritten, "first();\nlast();\n")
}

func TestSourceMapTraceDeduplicatesComposedPositions(t *testing.T) {
	trace, err := newSourceTraceMap(`{"version":3,"mappings":"AAAA;AACA"}`, "/authored.ts", "first();\nlast();\n")
	if err != nil {
		t.Fatal(err)
	}
	raw := `{"version":3,"file":"main.luau","sources":["/printed.ts"],"sourcesContent":["first();\nlast();\n"],"mappings":"AAAA,KAAK,KAAK;AACV"}`
	rewritten, err := rewriteSourceMapWithTrace(raw, trace)
	if err != nil {
		t.Fatal(err)
	}
	const want = ` 1 0->1:0 | first()
 2 0->2:0 | last()
`
	if got := annotateSourceMap("first()\nlast()\n", rewritten); got != want {
		t.Errorf("composed source map:\ngot:\n%swant:\n%s", got, want)
	}
}

func TestSourceMapFlameworkNestedCallbacks(t *testing.T) {
	// Use the real legacy plugin project, copied to a temporary directory with
	// its installed dependencies linked, just as the plugin integration suite does.
	dir := legacyFlameworkFixture(t)
	configPath := filepath.Join(dir, "tsconfig.json")
	config, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	withMaps := strings.Replace(string(config), `"outDir": "out"`, `"outDir": "out", "sourceMap": true`, 1)
	if err := os.WriteFile(configPath, []byte(withMaps), 0o644); err != nil {
		t.Fatal(err)
	}
	const source = `import { OnStart, Service } from "@flamework/core";

@Service()
export class TestService implements OnStart {
	public onStart(): void {
		const callback = () => {
			print("outer-callback");
			const inner = () => {
				print("inner-callback");
			};
			inner();
		};
		callback();
	}
}
`
	const relative = "server/services/test.service"
	if err := os.WriteFile(filepath.Join(dir, "src", filepath.FromSlash(relative)+".ts"), []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil || len(diags) != 0 {
		t.Fatalf("plugin build: %v (diagnostics: %v)", err, diags)
	}
	text := result.Outputs["out/"+relative+".luau"]
	if !strings.Contains(text, "defineMetadata") || !strings.Contains(text, "identifier") {
		t.Fatalf("real Flamework plugin did not inject metadata:\n%s", text)
	}
	mapBytes, err := os.ReadFile(filepath.Join(dir, "out", filepath.FromSlash(relative)+".luau.map"))
	if err != nil {
		t.Fatal(err)
	}
	assertSourceMapContract(t, text, string(mapBytes), source)
	mappings := decodeSourceMapLines(t, string(mapBytes))
	// Pin the whole nested-callback group, including both closing keywords.
	// The expected authored lines come from the literal source above, before
	// Flamework inserts its imports, decorators and metadata and reprints it.
	want := []struct {
		text string
		line int
	}{
		{"local callback = function()", 6},
		{`print("outer-callback")`, 7},
		{"local inner = function()", 8},
		{`print("inner-callback")`, 9},
		{"end", 10},
		{"inner()", 11},
		{"end", 12},
		{"callback()", 13},
	}
	start := -1
	lines := strings.Split(text, "\n")
	for line, content := range lines {
		if strings.TrimSpace(content) == want[0].text {
			start = line
			break
		}
	}
	if start < 0 || start+len(want) > len(lines) {
		t.Fatalf("nested callback group missing:\n%s", text)
	}
	for offset, expected := range want {
		line := start + offset
		if got := strings.TrimSpace(lines[line]); got != expected.text {
			t.Errorf("callback line %d = %q, want %q", offset+1, got, expected.text)
		}
		segments := mappings[line]
		if len(segments) == 0 || segments[0].GeneratedCharacter != 0 {
			t.Errorf("callback Luau line %d has no column-zero segment", line+1)
			continue
		}
		if !segments[0].IsSourceMapping() || segments[0].SourceLine+1 != expected.line {
			t.Errorf("callback Luau line %d maps to %+v, want authored TS line %d", line+1, segments[0], expected.line)
		}
	}
}
