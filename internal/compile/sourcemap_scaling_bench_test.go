package compile

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/transformer"
)

// Keep parsing and typechecking outside the measured maps-on/off transforms.
// These shapes catch repeated cache scans and repeated long-line prefix scans
// that the ordinary multiline renderer benchmark does not exercise.
func BenchmarkSourceMapTransformScaling(b *testing.B) {
	for _, shape := range []string{"array_inline", "decorated"} {
		sizes := []int{1000, 4000, 16000}
		if shape == "decorated" {
			sizes = []int{100, 400, 1600}
		}
		for _, size := range sizes {
			b.Run(fmt.Sprintf("%s/n%d", shape, size), func(b *testing.B) {
				var source strings.Builder
				if shape == "array_inline" {
					source.WriteString("export const values = [")
					for i := range size {
						fmt.Fprintf(&source, "%d,", i)
					}
					source.WriteString("];\n")
				} else {
					source.WriteString("declare function deco(target: object, key: string, descriptor: TypedPropertyDescriptor<unknown>): void;\nexport class Decorated {\n")
					for i := range size {
						fmt.Fprintf(&source, " @deco\n method%d(a: number) {\n", i)
						for j := range 16 {
							fmt.Fprintf(&source, "  a += %d;\n", j+1)
						}
						source.WriteString("  return a;\n }\n")
					}
					source.WriteString("}\n")
				}
				benchmarkSourceMapTransform(b, source.String())
			})
		}
	}
}

func benchmarkSourceMapTransform(b *testing.B, source string) {
	b.Helper()
	dir := b.TempDir()
	for name, text := range map[string]string{
		"tsconfig.json": `{"compilerOptions":{"allowSyntheticDefaultImports":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"sourceMap":false,"experimentalDecorators":true,"typeRoots":["node_modules/@rbxts"],"rootDir":".","outDir":"out"},"include":["*.ts"]}`,
		"globals.d.ts":  noLibGlobalStubs + "\ninterface TypedPropertyDescriptor<T> { value?: T; }\n",
		"large.ts":      source,
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
	newState := func(maps bool) *transformer.State {
		state := transformer.NewState(program, checker, sourceFile, transformer.NewDiagService(), transformer.NewMultiState())
		state.SourceMapEnabled = maps
		return state
	}
	state := newState(true)
	transformer.TransformSourceFile(state)
	if state.Diags.HasErrors() {
		b.Fatalf("transform diagnostics: %v", state.Diags.Flush())
	}
	for _, maps := range []bool{false, true} {
		b.Run(fmt.Sprintf("maps_%t", maps), func(b *testing.B) {
			b.ReportAllocs()
			for range b.N {
				transformer.TransformSourceFile(newState(maps))
			}
		})
	}
}
