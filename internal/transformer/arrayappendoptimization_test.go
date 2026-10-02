package transformer_test

import (
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

func renderArrayAppendFixture(t *testing.T, name string, enabled bool, expectedDiagnostics ...string) string {
	t.Helper()
	s := buildState(t, filepath.Join("testdata", "arrayappendoptimization"), "src/"+name+".ts")
	s.OptimizedArrayAppends = enabled

	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	diagnostics := s.Diags.Flush()
	if len(diagnostics) != len(expectedDiagnostics) {
		t.Errorf("diagnostics = %v, want codes %v", diagnostics, expectedDiagnostics)
	} else {
		for index, code := range expectedDiagnostics {
			if diagnostics[index].Code != code {
				t.Errorf("diagnostic %d code = %s, want %s", index, diagnostics[index].Code, code)
			}
		}
	}
	return render.RenderAST(statements)
}

func TestOptimizedArrayAppendsEmitsLoopCarriedIndex(t *testing.T) {
	want := `local result = {}
local _resultLength = 0
for _, value in source do
	-- ▼ Array.push ▼
	_resultLength += 1
	result[_resultLength] = value
	-- ▲ Array.push ▲
end
print(result)
`
	if got := renderArrayAppendFixture(t, "basic", true); got != want {
		t.Errorf("rendered output differs from optimized append form:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestOptimizedArrayAppendsHandlesLabeledLoops(t *testing.T) {
	got := renderArrayAppendFixture(t, "labeled", true)

	if strings.Contains(got, "table.insert") {
		t.Errorf("labeled loop retained table.insert:\n%s", got)
	}
	if !strings.Contains(got, "local _resultLength = 0") || !strings.Contains(got, "result[_resultLength] = value") {
		t.Errorf("labeled loop did not use a loop-carried index:\n%s", got)
	}
}

func TestOptimizedArrayAppendsSkipsZeroArgumentPush(t *testing.T) {
	want := `local result = {}
for index = 0, 2 do
	local _ = #result
end
`
	if got := renderArrayAppendFixture(t, "zero", true); got != want {
		t.Errorf("zero-argument push should retain the baseline output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestOptimizedArrayAppendsIsOptIn(t *testing.T) {
	got := renderArrayAppendFixture(t, "basic", false)

	if !strings.Contains(got, "table.insert(result, value)") {
		t.Errorf("disabled optimization changed Array.push output:\n%s", got)
	}
	if strings.Contains(got, "_resultLength") {
		t.Errorf("disabled optimization emitted an append counter:\n%s", got)
	}
}

func TestOptimizedArrayAppendsSupportsEveryLoopShape(t *testing.T) {
	got := renderArrayAppendFixture(t, "loops", true)

	if strings.Contains(got, "table.insert") {
		t.Errorf("supported loop retained table.insert:\n%s", got)
	}
	for _, target := range []string{"fromFor", "fromForOf", "fromWhile", "fromDo"} {
		if !strings.Contains(got, "local _"+target+"Length = 0") ||
			!strings.Contains(got, target+"[_"+target+"Length]") {
			t.Errorf("%s was not optimized:\n%s", target, got)
		}
	}
}

func TestOptimizedArrayAppendsPreservesArgumentOrderAndReturnLength(t *testing.T) {
	got := renderArrayAppendFixture(t, "multiple", true)

	if strings.Contains(got, "table.insert") {
		t.Errorf("multi-argument push retained table.insert:\n%s", got)
	}
	first := strings.Index(got, `nextValue("first")`)
	second := strings.Index(got, `nextValue("second")`)
	firstWrite := strings.Index(got, "_resultLength += 1")
	if first < 0 || second < 0 || firstWrite < 0 || first > second || second > firstWrite {
		t.Errorf("push arguments were not evaluated before indexed writes:\n%s", got)
	}
	if strings.Count(got, "_resultLength += 1") != 4 {
		t.Errorf("multi-site push emitted the wrong number of increments:\n%s", got)
	}
	if !strings.Contains(got, "local _length = _resultLength") ||
		!strings.Contains(got, "local length = _length") {
		t.Errorf("push return value did not use the loop-carried length:\n%s", got)
	}
	if !strings.Contains(got, "local unchanged = #result") {
		t.Errorf("zero-argument push did not retain its length result:\n%s", got)
	}
}

func TestOptimizedArrayAppendsSupportsNestedAndConditionalSites(t *testing.T) {
	got := renderArrayAppendFixture(t, "nested", true)

	if strings.Contains(got, "table.insert") {
		t.Errorf("nested append site retained table.insert:\n%s", got)
	}
	if strings.Count(got, "_resultLength += 1") != 3 {
		t.Errorf("nested append sites did not share one counter:\n%s", got)
	}
	if strings.Count(got, "local _resultLength = 0") != 1 {
		t.Errorf("nested append sites emitted more than one counter:\n%s", got)
	}
}

func TestOptimizedArrayAppendsAvoidsGeneratedNameCollisions(t *testing.T) {
	got := renderArrayAppendFixture(t, "collision", true)

	if !strings.Contains(got, "local _resultLength = 7") ||
		!strings.Contains(got, "local _resultLength_1 = 0") {
		t.Errorf("generated counter collided with a source binding:\n%s", got)
	}
}

func TestOptimizedArrayAppendsFallsBackWhenSafetyCannotBeProved(t *testing.T) {
	got := renderArrayAppendFixture(t, "fallbacks", true, "noVarArgsMacroSpread", "noAny")

	if strings.Contains(got, "Length = 0") {
		t.Errorf("unsafe append target was optimized:\n%s", got)
	}
	for _, target := range []string{
		"nonEmpty", "mutable", "separated", "observed", "aliased", "captured",
		"mutated", "indexed", "spread", "anyValues", "condition",
		"increment", "iterated", "first",
	} {
		if !strings.Contains(got, "table.insert("+target) {
			t.Errorf("%s did not retain the Array.push fallback:\n%s", target, got)
		}
	}
	if !strings.Contains(got, "custom:push(value)") {
		t.Errorf("user-defined push did not retain a method call:\n%s", got)
	}
}

func TestOptimizedArrayAppendsRejectsOutOfLoopCaptures(t *testing.T) {
	got := renderArrayAppendFixture(t, "capture", true)

	if strings.Contains(got, "_resultLength") {
		t.Errorf("captured array was optimized with a stale counter:\n%s", got)
	}
	if strings.Count(got, "table.insert(result") != 2 {
		t.Errorf("captured array did not retain both Array.push calls:\n%s", got)
	}
	if strings.Count(got, "table.insert(later") != 2 {
		t.Errorf("array captured by a later declaration did not retain both Array.push calls:\n%s", got)
	}
}

func TestOptimizedArrayAppendsSnapshotsConsumedReturnValues(t *testing.T) {
	got := renderArrayAppendFixture(t, "valueuses", true)

	if strings.Contains(got, "table.insert") {
		t.Errorf("consumed push retained table.insert:\n%s", got)
	}
	for _, snapshot := range []string{
		"local _length = _resultLength",
		"local _length_1 = _resultLength",
		"local _length_2 = _resultLength",
	} {
		if !strings.Contains(got, snapshot) {
			t.Errorf("missing return-value snapshot %q:\n%s", snapshot, got)
		}
	}
	if !strings.Contains(got, "print(_length, _length_1)") ||
		!strings.Contains(got, "result[_resultLength] = _length_2") {
		t.Errorf("later pushes can change an earlier returned length:\n%s", got)
	}
}
