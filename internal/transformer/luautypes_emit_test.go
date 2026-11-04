package transformer_test

import (
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// The luauTypes emission sites (luautypes_emit.go): which lines of a rendered
// fixture carry an annotation, and that the result is accepted by the Luau
// checker in strict mode. Lines are pinned individually, so a change to an
// unrelated transform does not fail these tests.

// requireLines fails for every line of want that rendered does not contain.
func requireLines(t *testing.T, rendered string, want []string) {
	t.Helper()
	for _, line := range want {
		if !strings.Contains(rendered, line+"\n") {
			t.Errorf("missing line %q", line)
		}
	}
	if t.Failed() {
		t.Logf("--- rendered ---\n%s", rendered)
	}
}

// requireClean runs the analyzer over rendered and fails on any type or
// syntax error.
func requireClean(t *testing.T, rendered string) {
	t.Helper()
	findings := analyzeLuauStrict(t, findLuauAnalyzer(t), rendered, true)
	if len(findings) != 0 {
		t.Errorf("does not analyze clean:\n%s\n--- rendered ---\n%s", strings.Join(findings, "\n"), rendered)
	}
}

// Parameters, `...`, `self`, generics and returns are spelled from the
// builder; a defaulted parameter is re-bound to its non-nil type right after
// its default; async bodies return the settled value and the TS.async local
// carries the Promise-returning type; a void body with a bare `return` is
// `...nil`.
func TestLuauTypesEmissionSignatures(t *testing.T) {
	rendered := renderLuauTypes(t, "src/emission.ts")
	requireLines(t, rendered, []string{
		"local function plain(a: number, b: string): boolean",
		"local function optionalParams(a: number, b: string?, c: number?): number",
		"local function restParams(first: number, ...: string): number",
		"local function restLoop(...: number): number",
		"local function restArray(...: string): { string }",
		"\tlocal rest = { ... }",
		"local function fact(n: number): number",
		"local function destructured(_param: { x: number, y: number }, _param_1: { string }): string",
		"local function thisParam(self: { x: number }, n: number): number",
		"local function luaTuple(): (number, string)",
		"local function optionalTuple(flag: boolean): (number, string?)",
		"local function nothing(): ()",
		"local function bareReturn(flag: boolean): ...nil",
		"local fetched: (n: number) -> Promise<number> = TS.async(function(n: number): number",
		"local fetchArrow: (n: number) -> Promise<number> = TS.async(function(n: number): number",
		"local awaited: () -> Promise<number> = TS.async(function(): number",
		"\tlocal value = TS.await(fetched(1))",
		"\tlocal _binding = TS.await(pair())",
		"local function counter(limit: number): Generator<number, nil, unknown>",
		"local identity = function<T>(value: T): T",
		"local function nested(): (n: number) -> number",
		"\tlocal double = function(n: number): number",
	})
	// The refinement shadow follows the default and infers from the cast.
	if want := "\tif c == nil then\n\t\tc = 2\n\tend\n\tlocal c = c :: number\n"; !strings.Contains(rendered, want) {
		t.Errorf("defaulted parameter is not re-bound after its default:\n%s", rendered)
	}
	requireClean(t, rendered)
}

// A logical chain's `_condition` temp is re-bound to the expression's TS
// type, and a literal spilled to a temp by such a chain keeps its shape:
// literal keys are assigned as properties, not through a pinned key.
func TestLuauTypesEmissionRefinesConditionTemps(t *testing.T) {
	rendered := renderLuauTypes(t, "src/emission.ts")
	requireLines(t, rendered, []string{
		"local function merged(defaults: Options, partial: { host: string?, port: number? }): Options",
		"\tlocal _condition = _condition :: string",
		"\t_object.host = _condition",
		"\tlocal _condition_1 = _condition_1 :: number",
		"\t_object.port = _condition_1",
		"\treturn _object",
		"local function fallback(name: string?): string",
	})
	if strings.Contains(rendered, "_object[_left]") {
		t.Errorf("a literal key must not be pinned to a temp with luauTypes on:\n%s", rendered)
	}
}

// Object-literal methods take an explicit `self`: the literal's contextual
// alias when there is one, a written `this` parameter's type, and `any`
// otherwise. The local declaring a typed literal is not annotated: the
// literal already is a Counter.
func TestLuauTypesEmissionObjectLiteralMethods(t *testing.T) {
	rendered := renderLuauTypes(t, "src/emission.ts")
	requireLines(t, rendered, []string{
		"local typed = {",
		"\tinc = function(self: Counter): ()",
		"\tinc = function(self: any): ()",
	})
	if got := strings.Count(rendered, "function(self: Counter): ()"); got != 2 {
		t.Errorf("expected the contextual and the explicit `this` method to both take `self: Counter`, found %d:\n%s", got, rendered)
	}
}

// Locals read like hand-written Luau: an initialized local is left to
// inference, even with a written TS type or a TS.import value. Only a local
// with no initializer, a hoisted one, or one whose written type differs from
// its initializer's widened type is annotated.
func TestLuauTypesEmissionLocals(t *testing.T) {
	rendered := renderLuauTypes(t, "src/locals.ts")
	requireLines(t, rendered, []string{
		"local aNumber = _primitives.aNumber",
		"local aString = _primitives.aString",
		"\tlocal count = 0",
		"\tlocal label: string?",
		"\tlocal value = if flag then \"a\" else 1",
		"\tlocal mode: \"r\" | \"w\" = \"r\"",
		"local later: () -> number",
		"function later(): number",
	})
	if !strings.Contains(rendered, "\tlocal names = { \"a\", \"b\" }\n") {
		t.Errorf("a const with an initializer must stay unannotated for Luau to infer:\n%s", rendered)
	}
	requireClean(t, rendered)
}

// In a `//!native` file with both options on, the native policy still owns
// every signature (byte-identical to nativeTypes alone) and luauTypes owns
// the locals.
func TestLuauTypesKeepsNativeSignatures(t *testing.T) {
	renderWith := func(luauTypes bool) string {
		s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "functions"), "src/native.ts")
		s.NativeTypes = true
		s.LuauTypes = luauTypes
		statements := transformer.TransformSourceFile(s)
		if ds := s.Diags.Flush(); len(ds) != 0 {
			t.Fatalf("unexpected diagnostics: %v", ds)
		}
		return render.RenderAST(statements)
	}
	// A function statement, or the callback handed to TS.async (whose
	// local is luauTypes' to annotate, so only the callback is compared).
	signature := regexp.MustCompile(`^\s*(?:local )?function .*$|TS\.async\(function.*$`)
	signatures := func(rendered string) []string {
		var lines []string
		for _, line := range strings.Split(rendered, "\n") {
			if match := signature.FindString(line); match != "" {
				lines = append(lines, strings.TrimSpace(match))
			}
		}
		return lines
	}
	native, both := renderWith(false), renderWith(true)
	if got, want := signatures(both), signatures(native); !slices.Equal(got, want) {
		t.Errorf("signatures changed when luauTypes joined nativeTypes:\ngot:\n%s\nwant:\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
	if !strings.Contains(both, "local loadAsync: (count: number) -> Promise<number> = TS.async(") {
		t.Errorf("luauTypes should still annotate the TS.async local in a native file:\n%s", both)
	}
	if strings.Contains(native, "local loadAsync:") {
		t.Errorf("nativeTypes alone must not annotate locals:\n%s", native)
	}
}
