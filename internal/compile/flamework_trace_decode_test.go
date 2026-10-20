package compile

import (
	"context"
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"rotor/internal/flamework"
	"rotor/internal/transformer"
	"rotor/tsgo/core"
	"rotor/tsgo/printer"
	"rotor/tsgo/sourcemap"
	"rotor/tsgo/tspath"
)

func TestFlameworkTraceDirectMatchesGeneratedJSON(t *testing.T) {
	for _, fixture := range []struct{ name, text string }{
		{"LF", "first\nsecond\nthird\n"},
		{"CRLFNonBMP", "😀first\r\n𐐀second\r\nthird\r\n"},
		{"LargeInlineSource", strings.Repeat("const greeting = \"😀<&>\";\r\n", 32768)},
	} {
		t.Run(fixture.name, func(t *testing.T) {
			text := fixture.text
			raw := generatedFlameworkTrace(t, text)
			encoded, err := json.Marshal(raw)
			if err != nil {
				t.Fatal(err)
			}
			old, err := newSourceTraceMap(string(encoded), "/authored/😀.ts", text)
			if err != nil {
				t.Fatal(err)
			}
			direct, err := newSourceTraceMapFromMappings(raw.Version, raw.Mappings, "/authored/😀.ts", text)
			if err != nil {
				t.Fatal(err)
			}
			want := &sourceTraceMap{fileName: "/authored/😀.ts", text: text, mappings: []traceMapping{
				{generatedLine: 0, generatedColumn: 2, sourceLine: 0, sourceColumn: 1},
				{generatedLine: 0, generatedColumn: 2, sourceLine: 0, sourceColumn: 0},
				{generatedLine: 0, generatedColumn: 5, sourceLine: 1, sourceColumn: 2},
				{generatedLine: 2, generatedColumn: 1, sourceLine: 1, sourceColumn: 4},
				{generatedLine: 3, generatedColumn: 0, sourceLine: 2, sourceColumn: 0},
			}}
			for route, trace := range map[string]*sourceTraceMap{"JSON": old, "direct": direct} {
				if !reflect.DeepEqual(trace, want) {
					t.Fatalf("%s trace = %#v, want %#v", route, trace, want)
				}
				if trace.OriginalSourceFileName() != want.fileName || trace.OriginalSourceText() != text {
					t.Fatalf("%s lost original filename/text", route)
				}
				for _, query := range []struct {
					line, column int
					want         *transformer.SourcePosition
				}{
					{-1, 0, nil}, {0, -1, nil}, {0, 0, nil}, {0, 1, nil},
					{0, 2, &transformer.SourcePosition{Line: 0, Column: 0}},
					{0, 4, &transformer.SourcePosition{Line: 0, Column: 0}},
					{0, 5, &transformer.SourcePosition{Line: 1, Column: 2}},
					// Unmapped segments and empty lines retain the preceding source mapping.
					{0, 8, &transformer.SourcePosition{Line: 1, Column: 2}},
					{1, 0, &transformer.SourcePosition{Line: 1, Column: 2}},
					{2, 0, &transformer.SourcePosition{Line: 1, Column: 2}},
					{2, 1, &transformer.SourcePosition{Line: 1, Column: 4}},
					{3, 0, &transformer.SourcePosition{Line: 2, Column: 0}},
					{math.MaxInt32, math.MaxInt32, &transformer.SourcePosition{Line: 2, Column: 0}},
				} {
					position := transformer.SourcePosition{Line: query.line, Column: query.column}
					if got := trace.OriginalPosition(position); !reflect.DeepEqual(got, query.want) {
						t.Fatalf("%s OriginalPosition(%+v) = %+v, want %+v", route, position, got, query.want)
					}
				}
			}
		})
	}
}

func generatedFlameworkTrace(tb testing.TB, text string) *sourcemap.RawSourceMap {
	tb.Helper()
	generator := sourcemap.NewGenerator("printed.ts", "", "/project", tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true, CurrentDirectory: "/project"})
	source := generator.AddSource("/project/original.ts")
	name := generator.AddName("greeting")
	for _, err := range []error{
		generator.SetSourceContent(source, text),
		generator.AddGeneratedMapping(0, 0),
		generator.AddNamedSourceMapping(0, 2, source, 0, 1, name),
		// A backtracking source position emits a duplicate generated position.
		generator.AddSourceMapping(0, 2, source, 0, 0),
		generator.AddSourceMapping(0, 5, source, 1, 2),
		generator.AddGeneratedMapping(0, 8),
		generator.AddSourceMapping(2, 1, source, 1, 4),
		generator.AddSourceMapping(3, 0, source, 2, 0),
	} {
		if err != nil {
			tb.Fatal(err)
		}
	}
	// RawSourceMap must commit the final pending mapping; the test expects it.
	return generator.RawSourceMap()
}

func TestFlameworkTraceDirectValidationParity(t *testing.T) {
	coordinateMap := func(generatedColumn, sourceLine, sourceColumn int) string {
		generator := sourcemap.NewGenerator("printed.ts", "", "/project", tspath.ComparePathsOptions{})
		source := generator.AddSource("/project/original.ts")
		if err := generator.AddSourceMapping(0, core.UTF16Offset(generatedColumn), source, sourceLine, core.UTF16Offset(sourceColumn)); err != nil {
			t.Fatal(err)
		}
		return generator.RawSourceMap().Mappings
	}
	const overflowError = "parse transformer trace map: coordinate exceeds 32-bit range"
	for _, tt := range []struct {
		name, mappings string
		version        int
		want           []traceMapping
		wantError      string
	}{
		{name: "empty", version: 3},
		{name: "unmapped only", version: 3, mappings: "A,K;C"},
		{name: "zero version", wantError: "parse transformer trace map: version = 0, want 3"},
		{name: "version before VLQ", version: 2, mappings: "%", wantError: "parse transformer trace map: version = 2, want 3"},
		{name: "invalid character", version: 3, mappings: "%", wantError: "parse transformer trace map mappings: Invalid character in VLQ"},
		{name: "truncated VLQ", version: 3, mappings: "g", wantError: "parse transformer trace map mappings: Error in decoding base64VLQFormatDecode, past the mapping string"},
		{name: "missing source line", version: 3, mappings: "AA", wantError: "parse transformer trace map mappings: Unsupported Format: No entries after sourceIndex"},
		{name: "missing source column", version: 3, mappings: "AAA", wantError: "parse transformer trace map mappings: Unsupported Format: No entries after sourceLine"},
		{name: "negative generated column", version: 3, mappings: "D", wantError: "parse transformer trace map mappings: Invalid generatedCharacter found"},
		{name: "negative source line", version: 3, mappings: "AADA", wantError: "parse transformer trace map mappings: Invalid sourceLine found"},
		{name: "32 bit boundary", version: 3, mappings: coordinateMap(math.MaxInt32, math.MaxInt32, math.MaxInt32), want: []traceMapping{{generatedColumn: math.MaxInt32, sourceLine: math.MaxInt32, sourceColumn: math.MaxInt32}}},
		{name: "generated column overflow", version: 3, mappings: coordinateMap(math.MaxInt32+1, 0, 0), wantError: overflowError},
		{name: "source line overflow", version: 3, mappings: coordinateMap(0, math.MaxInt32+1, 0), wantError: overflowError},
		{name: "source column overflow", version: 3, mappings: coordinateMap(0, 0, math.MaxInt32+1), wantError: overflowError},
	} {
		t.Run(tt.name, func(t *testing.T) {
			encoded, err := json.Marshal(sourcemap.RawSourceMap{Version: tt.version, Mappings: tt.mappings})
			if err != nil {
				t.Fatal(err)
			}
			old, oldError := newSourceTraceMap(string(encoded), "original.ts", "😀\r\n")
			direct, directError := newSourceTraceMapFromMappings(tt.version, tt.mappings, "original.ts", "😀\r\n")
			for route, err := range map[string]error{"JSON": oldError, "direct": directError} {
				got := ""
				if err != nil {
					got = err.Error()
				}
				if got != tt.wantError {
					t.Fatalf("%s error = %q, want %q", route, got, tt.wantError)
				}
			}
			if !reflect.DeepEqual(old, direct) {
				t.Fatalf("JSON trace = %#v, direct trace = %#v", old, direct)
			}
			if tt.wantError != "" {
				if old != nil || direct != nil {
					t.Fatal("invalid map returned a partial trace")
				}
			} else if !reflect.DeepEqual(direct.mappings, tt.want) {
				t.Fatalf("mappings = %#v, want %#v", direct.mappings, tt.want)
			}
		})
	}
}

func TestFlameworkTraceExternalJSONErrors(t *testing.T) {
	for _, tt := range []struct{ raw, want string }{
		{"", "parse transformer trace map: unexpected end of JSON input"},
		{"{", "parse transformer trace map: unexpected end of JSON input"},
		{"[]", "parse transformer trace map: json: cannot unmarshal array into Go value of type compile.rawSourceMap"},
		{`{"version":"3"}`, "parse transformer trace map: json: cannot unmarshal string into Go struct field rawSourceMap.version of type int"},
		{`{"version":3,"mappings":1}`, "parse transformer trace map: json: cannot unmarshal number into Go struct field rawSourceMap.mappings of type string"},
		{`{"version":3,"sources":[1]}`, "parse transformer trace map: json: cannot unmarshal number into Go struct field rawSourceMap.sources of type string"},
		{`{"version":3,"sourcesContent":[1]}`, "parse transformer trace map: json: cannot unmarshal number into Go struct field rawSourceMap.sourcesContent of type string"},
		{`{"version":3}x`, "parse transformer trace map: invalid character 'x' after top-level value"},
		{"null", "parse transformer trace map: version = 0, want 3"},
	} {
		t.Run(tt.raw, func(t *testing.T) {
			trace, err := newSourceTraceMap(tt.raw, "original.ts", "text")
			if err == nil || err.Error() != tt.want || trace != nil {
				t.Fatalf("newSourceTraceMap = (%+v, %v), want (nil, %q)", trace, err, tt.want)
			}
		})
	}
}

func TestFlameworkTracePrintedSourceMatchesJSONRoute(t *testing.T) {
	dir, program, sources := sparseNativeProgram(t, map[string]string{
		"main.ts": "// 😀 authored\r\nexport const greeting = \"𐐀\";\r\nexport function identity(value: number) { return value; }\r\n",
	})
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	pipeline, diagnostics, err := prepareFlameworkPipeline(filepath.ToSlash(dir), program, ProjectOptions{})
	if err != nil {
		t.Fatalf("prepareFlameworkPipeline: %v (%v)", err, diagnostics)
	}
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	result, err := flamework.Transform(flamework.TransformInput{Program: program, Checker: checker, Files: sources, Project: pipeline.project})
	if err != nil || len(result.Diagnostics) != 0 {
		t.Fatalf("Transform: %v (%v)", err, result.Diagnostics)
	}
	for index, metadata := range result.Sources {
		file := result.Files[index]
		text, trace, err := printFlameworkSource(file, metadata)
		if err != nil {
			t.Fatal(err)
		}
		// The old native operation is the control: same printer/options and
		// pending-map flush, followed by Marshal and the external JSON constructor.
		writer := printer.NewTextWriter("\n", 0)
		generator := sourcemap.NewGenerator(filepath.Base(file.FileName()), "", filepath.Dir(file.FileName()), tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true, CurrentDirectory: filepath.Dir(file.FileName())})
		printer.NewPrinter(printer.PrinterOptions{SourceMap: true, InlineSources: true}, printer.PrintHandlers{}, metadata.EmitContext()).Write(file.AsNode(), metadata.Original(), writer, generator)
		encoded, err := json.Marshal(generator.RawSourceMap())
		if err != nil {
			t.Fatal(err)
		}
		old, err := newSourceTraceMap(string(encoded), metadata.Trace().OriginalFileName(), metadata.Trace().OriginalText())
		if err != nil {
			t.Fatal(err)
		}
		if want := writer.(interface{ String() string }).String(); text != want {
			t.Fatalf("printed source = %q, JSON route = %q", text, want)
		}
		if !reflect.DeepEqual(trace, old) || len(trace.mappings) == 0 || trace.text != metadata.Original().Text() || trace.fileName != metadata.FileName() {
			t.Fatalf("printed trace differs from JSON route or lost authored source: %s", file.FileName())
		}
	}
}

var benchmarkFlameworkTrace *sourceTraceMap

func BenchmarkFlameworkTraceDecode(b *testing.B) {
	for _, fixture := range []struct {
		name string
		text string
	}{
		{"Small", strings.Repeat("const greeting = \"😀<&>\";\r\n", 2)},
		{"Large", strings.Repeat("const greeting = \"😀<&>\";\r\n", 32768)},
	} {
		// Both routes and both source sizes use exactly the same mappings.
		raw := generatedFlameworkTrace(b, fixture.text)
		b.Run(fixture.name+"/JSON", func(b *testing.B) {
			b.ReportAllocs()
			for b.Loop() {
				encoded, err := json.Marshal(raw)
				if err != nil {
					b.Fatal(err)
				}
				trace, err := newSourceTraceMap(string(encoded), "/project/original.ts", fixture.text)
				if err != nil {
					b.Fatal(err)
				}
				benchmarkFlameworkTrace = trace
			}
		})
		b.Run(fixture.name+"/Direct", func(b *testing.B) {
			b.ReportAllocs()
			for b.Loop() {
				trace, err := newSourceTraceMapFromMappings(raw.Version, raw.Mappings, "/project/original.ts", fixture.text)
				if err != nil {
					b.Fatal(err)
				}
				benchmarkFlameworkTrace = trace
			}
		})
	}
}
