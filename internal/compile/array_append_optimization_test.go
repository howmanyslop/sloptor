package compile

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestCompileFileOptimizedArrayAppendsOption(t *testing.T) {
	root, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	project := filepath.Join(root, "testdata", "diff", "project")
	input := filepath.Join(project, "src", "01_literals.ts")
	source := `declare const source: ReadonlyArray<number>;
const result = new Array<number>();
for (const value of source) {
	result.push(value);
}
print(result);

const optional: Array<number> | undefined = [];
for (const value of source) {
	optional?.push(value);
}

export const exported = new Array<number>();
for (const value of source) {
	exported.push(value);
}
`

	compile := func(enabled bool) string {
		t.Helper()
		got, diagnostics, err := CompileFileDetailedWithOptions(project, "src/01_literals.ts", ProjectOptions{
			OptimizedArrayAppends: enabled,
			Overlays:              map[string]string{input: source},
		})
		if err != nil {
			t.Fatalf("compile: %v (diagnostics: %v)", err, diagnostics)
		}
		if len(diagnostics) != 0 {
			t.Fatalf("diagnostics: %v", diagnostics)
		}
		return got
	}

	baseline := compile(false)
	if !strings.Contains(baseline, "table.insert(result, value)") || strings.Contains(baseline, "_resultLength") {
		t.Errorf("default output did not retain Array.push parity:\n%s", baseline)
	}

	optimized := compile(true)
	if strings.Contains(optimized, "table.insert(result") ||
		!strings.Contains(optimized, "local _resultLength = 0") ||
		!strings.Contains(optimized, "result[_resultLength] = value") {
		t.Errorf("optimized project output did not use indexed appends:\n%s", optimized)
	}
	if strings.Contains(optimized, "_optionalLength") || !strings.Contains(optimized, "table.insert(_result, value)") {
		t.Errorf("optional call did not retain its fallback:\n%s", optimized)
	}
	if strings.Contains(optimized, "_exportedLength") || !strings.Contains(optimized, "table.insert(exported, value)") {
		t.Errorf("exported array did not retain its fallback:\n%s", optimized)
	}
}
