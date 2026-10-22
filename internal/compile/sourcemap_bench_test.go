package compile

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/sourcemap"
)

func BenchmarkSourceMapLarge(b *testing.B) {
	var source strings.Builder
	source.WriteString("declare function register(callback: () => void): number;\n")
	source.WriteString("declare function use(value: unknown): void;\n")
	for i := range 300 {
		fmt.Fprintf(&source, `export function f%d(a: number, b?: number) {
	const value = b ?? a;
	const handler = register(() => {
		use(value);
		if (value > %d) {
			use({ x: value, y: %d });
		}
	});
	return () => {
		use(handler);
	};
}
`, i, i, i)
	}

	dir := b.TempDir()
	for name, text := range map[string]string{
		"tsconfig.json": `{
			"compilerOptions": {
				"allowSyntheticDefaultImports": true,
				"module": "CommonJS", "moduleResolution": "Node",
				"noLib": true, "moduleDetection": "force", "strict": true,
				"target": "ESNext", "types": [], "sourceMap": false,
				"typeRoots": ["node_modules/@rbxts"], "rootDir": ".", "outDir": "out"
			},
			"include": ["*.ts"]
		}`,
		"globals.d.ts": noLibGlobalStubs,
		"large.ts":     source.String(),
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o644); err != nil {
			b.Fatal(err)
		}
	}
	absDir, program, diags, err := newProjectProgram(dir, "")
	if err != nil || len(diags) != 0 {
		b.Fatalf("newProjectProgram: %v (diagnostics: %v)", err, diags)
	}
	sourceFile := program.GetSourceFile(absDir + "/large.ts")
	if sourceFile == nil {
		b.Fatal("large.ts missing from program")
	}
	ctx := context.Background()
	if diags := preEmitProjectFileDiagnosticsWithOptions(ctx, program, sourceFile, ProjectOptions{}); len(diags) != 0 {
		b.Fatalf("TypeScript diagnostics: %v", diags)
	}
	checker, release := program.GetTypeCheckerForFile(ctx, sourceFile)
	defer release()
	newState := func() *transformer.State {
		return transformer.NewState(program, checker, sourceFile, transformer.NewDiagService(), transformer.NewMultiState())
	}
	state := newState()
	state.SourceMapEnabled = true
	statements := transformer.TransformSourceFile(state)
	if state.Diags.HasErrors() {
		b.Fatalf("transform diagnostics: %v", state.Diags.Flush())
	}
	text, mapJSON := state.RenderWithSourceMap(statements, sourceFile)
	var sourceMap struct {
		Mappings string `json:"mappings"`
	}
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		b.Fatal(err)
	}
	mappedLines := map[int]bool{}
	segments := 0
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for mapping := range decoder.Values() {
		segments++
		mappedLines[mapping.GeneratedLine] = true
	}
	if err := decoder.Error(); err != nil {
		b.Fatal(err)
	}
	nonBlank, mapped := 0, 0
	for line, text := range strings.Split(text, "\n") {
		if strings.TrimSpace(text) != "" {
			nonBlank++
			if mappedLines[line] {
				mapped++
			}
		}
	}
	b.Logf("fixture: %d TS lines; mappings: %d bytes; non-blank Luau lines: %d; mapped: %d; unmapped: %d; segments: %d",
		strings.Count(source.String(), "\n"), len(sourceMap.Mappings), nonBlank, mapped, nonBlank-mapped, segments)

	b.Run("transform", func(b *testing.B) {
		b.ReportAllocs()
		for range b.N {
			transformer.TransformSourceFile(newState())
		}
	})
	b.Run("render", func(b *testing.B) {
		b.ReportAllocs()
		for range b.N {
			render.RenderAST(statements)
		}
	})
	b.Run("render_with_map", func(b *testing.B) {
		b.ReportAllocs()
		for range b.N {
			state.RenderWithSourceMap(statements, sourceFile)
		}
	})
}
