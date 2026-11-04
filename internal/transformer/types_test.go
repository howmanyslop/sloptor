package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// probeState builds the typeprobe project (testdata/typeprobe) — declarations
// of known types purely for predicate/truthiness tests.
func probeState(t *testing.T) *transformer.State {
	t.Helper()
	return buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "typeprobe"), "src/probe.ts")
}

// probeType returns the checker type of the unique identifier named name
// (declaration names carry the annotated type, unnarrowed).
func probeType(t *testing.T, s *transformer.State, name string) *checker.Type {
	t.Helper()
	return s.GetType(mustOneIdentifier(t, s.SourceFile.AsNode(), name))
}

// findFirstKind returns the first node of the given kind under root, in
// source order (used to probe `true`/`false` literal expressions, whose types
// are the checker's FRESH boolean literal types).
func findFirstKind(t *testing.T, root *ast.Node, kind ast.Kind) *ast.Node {
	t.Helper()
	var found *ast.Node
	var visit func(node *ast.Node) bool
	visit = func(node *ast.Node) bool {
		if found != nil {
			return true
		}
		if node.Kind == kind {
			found = node
			return true
		}
		return node.ForEachChild(visit)
	}
	root.ForEachChild(visit)
	if found == nil {
		t.Fatalf("no node of kind %v found", kind)
	}
	return found
}

func TestIsDefinitelyType(t *testing.T) {
	s := probeState(t)
	get := func(name string) *checker.Type { return probeType(t, s, name) }

	for _, tc := range []struct {
		name string
		got  bool
		want bool
	}{
		// leaves
		{"number is definitely number", transformer.IsDefinitelyType(s, get("num"), transformer.IsNumberType), true},
		{"number is not definitely string", transformer.IsDefinitelyType(s, get("num"), transformer.IsStringType), false},
		{"string is definitely string", transformer.IsDefinitelyType(s, get("str"), transformer.IsStringType), true},
		{"literal 5 is definitely number", transformer.IsDefinitelyType(s, get("five"), transformer.IsNumberType), true},
		{"literal empty string is definitely string", transformer.IsDefinitelyType(s, get("empty"), transformer.IsStringType), true},
		{"boolean is definitely boolean", transformer.IsDefinitelyType(s, get("bool"), transformer.IsBooleanType), true},
		// multiple callbacks = OR
		{"number is definitely string-or-number", transformer.IsDefinitelyType(s, get("num"), transformer.IsStringType, transformer.IsNumberType), true},
		// union: EVERY member must match
		{"0 | 1 is definitely number", transformer.IsDefinitelyType(s, get("zeroOne"), transformer.IsNumberType), true},
		{"number | string is not definitely number", transformer.IsDefinitelyType(s, get("numStr"), transformer.IsNumberType), false},
		{"number | string is not definitely string", transformer.IsDefinitelyType(s, get("numStr"), transformer.IsStringType), false},
		{"number | string is definitely string-or-number", transformer.IsDefinitelyType(s, get("numStr"), transformer.IsStringType, transformer.IsNumberType), true},
		{"string | undefined is not definitely string", transformer.IsDefinitelyType(s, get("strOpt"), transformer.IsStringType), false},
		// intersection: SOME member suffices
		{"branded string is definitely string", transformer.IsDefinitelyType(s, get("branded"), transformer.IsStringType), true},
		// any/unknown match nothing definitely
		{"unknown is not definitely string", transformer.IsDefinitelyType(s, get("unk"), transformer.IsStringType), false},
		{"any is not definitely string", transformer.IsDefinitelyType(s, get("anyVal"), transformer.IsStringType), false},
		// interfaces (exercises the class/interface base-type recursion)
		{"interface is definitely object", transformer.IsDefinitelyType(s, get("derived"), transformer.IsObjectType), true},
		{"interface is not definitely string", transformer.IsDefinitelyType(s, get("derived"), transformer.IsStringType), false},
		// generic constraint lookup: T extends string resolves to string
		{"T extends string is definitely string", transformer.IsDefinitelyType(s, get("conT"), transformer.IsStringType), true},
		{"T extends string is not definitely number", transformer.IsDefinitelyType(s, get("conT"), transformer.IsNumberType), false},
		// unconstrained type variable matches nothing definitely
		{"unconstrained U is not definitely string", transformer.IsDefinitelyType(s, get("unconU"), transformer.IsStringType), false},
	} {
		if tc.got != tc.want {
			t.Errorf("%s: got %v, want %v", tc.name, tc.got, tc.want)
		}
	}
}

func TestIsPossiblyType(t *testing.T) {
	s := probeState(t)
	get := func(name string) *checker.Type { return probeType(t, s, name) }

	for _, tc := range []struct {
		name string
		got  bool
		want bool
	}{
		// truthiness-driving queries (digest §6 predicate semantics)
		{"number possibly 0", transformer.IsPossiblyType(s, get("num"), transformer.IsNumberLiteralType(0)), true},
		{"number possibly NaN", transformer.IsPossiblyType(s, get("num"), transformer.IsNaNType), true},
		{"number not possibly empty string", transformer.IsPossiblyType(s, get("num"), transformer.IsEmptyStringType), false},
		{"string possibly empty string", transformer.IsPossiblyType(s, get("str"), transformer.IsEmptyStringType), true},
		{"string not possibly 0", transformer.IsPossiblyType(s, get("str"), transformer.IsNumberLiteralType(0)), false},
		{"string not possibly NaN", transformer.IsPossiblyType(s, get("str"), transformer.IsNaNType), false},
		// literal types: isNumberLiteralType compares the literal value
		{"literal 5 not possibly 0", transformer.IsPossiblyType(s, get("five"), transformer.IsNumberLiteralType(0)), false},
		{"literal 5 possibly 5", transformer.IsPossiblyType(s, get("five"), transformer.IsNumberLiteralType(5)), true},
		{"literal 5 not possibly NaN", transformer.IsPossiblyType(s, get("five"), transformer.IsNaNType), false},
		{"literal 0 possibly 0", transformer.IsPossiblyType(s, get("zero"), transformer.IsNumberLiteralType(0)), true},
		{"literal 0 not possibly NaN", transformer.IsPossiblyType(s, get("zero"), transformer.IsNaNType), false},
		// union: SOME member suffices
		{"0 | 1 possibly 0", transformer.IsPossiblyType(s, get("zeroOne"), transformer.IsNumberLiteralType(0)), true},
		{"0 | 1 not possibly NaN", transformer.IsPossiblyType(s, get("zeroOne"), transformer.IsNaNType), false},
		{"string | undefined possibly string", transformer.IsPossiblyType(s, get("strOpt"), transformer.IsStringType), true},
		{"string | undefined possibly undefined", transformer.IsPossiblyType(s, get("strOpt"), transformer.IsUndefinedType), true},
		{"string | undefined possibly empty string", transformer.IsPossiblyType(s, get("strOpt"), transformer.IsEmptyStringType), true},
		{"string | undefined not possibly 0", transformer.IsPossiblyType(s, get("strOpt"), transformer.IsNumberLiteralType(0)), false},
		// intersection: SOME member suffices
		{"branded string possibly string", transformer.IsPossiblyType(s, get("branded"), transformer.IsStringType), true},
		{"branded string not possibly number", transformer.IsPossiblyType(s, get("branded"), transformer.IsNumberType), false},
		// any/unknown are possibly anything
		{"unknown possibly 0", transformer.IsPossiblyType(s, get("unk"), transformer.IsNumberLiteralType(0)), true},
		{"unknown possibly NaN", transformer.IsPossiblyType(s, get("unk"), transformer.IsNaNType), true},
		{"unknown possibly empty string", transformer.IsPossiblyType(s, get("unk"), transformer.IsEmptyStringType), true},
		{"unknown possibly undefined", transformer.IsPossiblyType(s, get("unk"), transformer.IsUndefinedType), true},
		{"any possibly string", transformer.IsPossiblyType(s, get("anyVal"), transformer.IsStringType), true},
		// unconstrained type variable is possibly anything
		{"unconstrained U possibly number", transformer.IsPossiblyType(s, get("unconU"), transformer.IsNumberType), true},
		{"unconstrained U possibly undefined", transformer.IsPossiblyType(s, get("unconU"), transformer.IsUndefinedType), true},
		// constrained type variable resolves to its constraint first
		{"T extends string possibly empty string", transformer.IsPossiblyType(s, get("conT"), transformer.IsEmptyStringType), true},
		{"T extends string not possibly number", transformer.IsPossiblyType(s, get("conT"), transformer.IsNumberType), false},
		// the rbxts `defined` special case: possibly anything EXCEPT a pure
		// undefined query
		{"defined possibly string", transformer.IsPossiblyType(s, get("definedVal"), transformer.IsStringType), true},
		{"defined not possibly undefined (sole callback)", transformer.IsPossiblyType(s, get("definedVal"), transformer.IsUndefinedType), false},
		{"defined possibly undefined-or-string (two callbacks)", transformer.IsPossiblyType(s, get("definedVal"), transformer.IsUndefinedType, transformer.IsStringType), true},
		// interfaces
		{"interface not possibly string", transformer.IsPossiblyType(s, get("derived"), transformer.IsStringType), false},
	} {
		if tc.got != tc.want {
			t.Errorf("%s: got %v, want %v", tc.name, tc.got, tc.want)
		}
	}
}

func TestDirectPredicates(t *testing.T) {
	s := probeState(t)
	root := s.SourceFile.AsNode()
	get := func(name string) *checker.Type { return probeType(t, s, name) }

	// The FRESH boolean literal types survive only on `const x = true/false`
	// declaration names (getTypeOfSymbol); getTypeAtLocation on EXPRESSION
	// nodes goes through getRegularTypeOfExpression, which strips freshness —
	// in upstream too, so isBooleanLiteralType through state.getType(expr)
	// always sees the regular variants it matches. Probe both.
	freshTrue := probeType(t, s, "troof")
	freshFalse := probeType(t, s, "falsef")
	regularTrueExpr := s.GetType(findFirstKind(t, root, ast.KindTrueKeyword))

	for _, tc := range []struct {
		name string
		got  bool
		want bool
	}{
		{"isAnyType(any)", transformer.IsAnyType(s).Check(get("anyVal")), true},
		{"isAnyType(unknown)", transformer.IsAnyType(s).Check(get("unk")), false},
		{"isBooleanType(true literal)", transformer.IsBooleanType.Check(get("troo")), true},
		// isBooleanLiteralType compares identity against upstream
		// getTrueType()/getFalseType(), which TS implements as
		// `(fresh) => fresh ? freshType : regularType` — the no-argument
		// upstream call yields the REGULAR true/false types (the members of
		// the `boolean` union). A fresh literal-expression type does NOT
		// match; declared annotations and union members (regular variants) do.
		{"isBooleanLiteralType(true) on fresh true", transformer.IsBooleanLiteralType(s, true).Check(freshTrue), false},
		{"isBooleanLiteralType(false) on fresh true", transformer.IsBooleanLiteralType(s, false).Check(freshTrue), false},
		{"isBooleanLiteralType(false) on fresh false", transformer.IsBooleanLiteralType(s, false).Check(freshFalse), false},
		{"isBooleanLiteralType(true) on regular true (annotation)", transformer.IsBooleanLiteralType(s, true).Check(get("troo")), true},
		{"isBooleanLiteralType(true) on regular true (expression)", transformer.IsBooleanLiteralType(s, true).Check(regularTrueExpr), true},
		// non-literal fallback: plain boolean members count as possibly-false
		{"isBooleanLiteralType(false) falls back for non-literals", transformer.IsBooleanLiteralType(s, false).Check(get("num")), false},
		{"isNaNType(number)", transformer.IsNaNType.Check(get("num")), true},
		{"isNaNType(literal 5)", transformer.IsNaNType.Check(get("five")), false},
		{"isEmptyStringType(empty literal)", transformer.IsEmptyStringType.Check(get("empty")), true},
		{"isEmptyStringType(string)", transformer.IsEmptyStringType.Check(get("str")), true},
		{"isUndefinedType(number)", transformer.IsUndefinedType.Check(get("num")), false},
		{"isObjectType(interface)", transformer.IsObjectType.Check(get("derived")), true},
	} {
		if tc.got != tc.want {
			t.Errorf("%s: got %v, want %v", tc.name, tc.got, tc.want)
		}
	}
}

func TestWillCreateTruthinessChecks(t *testing.T) {
	s := probeState(t)
	for name, want := range map[string]bool{
		"bool":    false,
		"five":    false,
		"num":     true,
		"str":     true,
		"zero":    true,
		"numStr":  true,
		"strOpt":  true,
		"unk":     true,
		"derived": false,
	} {
		if got := transformer.WillCreateTruthinessChecks(s, probeType(t, s, name)); got != want {
			t.Errorf("WillCreateTruthinessChecks(%s) = %v, want %v", name, got, want)
		}
	}
}

func TestCreateTruthinessChecks(t *testing.T) {
	s := probeState(t)
	for _, tc := range []struct {
		name string // identifier in probe.ts
		want string // exact rendered Luau
	}{
		{"bool", "x"},
		{"five", "x"},
		{"derived", "x"},
		{"num", "x ~= 0 and x == x and x"},
		{"zero", "x ~= 0 and x == x and x"},    // TS#32778: 0-possible adds the NaN check
		{"zeroOne", "x ~= 0 and x == x and x"}, // ditto (no NaN-able member)
		{"str", `x ~= "" and x`},
		{"empty", `x ~= "" and x`},
		{"strOpt", `x ~= "" and x`}, // undefined needs only the value check
		{"numStr", `x ~= 0 and x == x and x ~= "" and x`},
		{"unk", `x ~= 0 and x == x and x ~= "" and x`},
	} {
		t.Run(tc.name, func(t *testing.T) {
			node := mustOneIdentifier(t, s.SourceFile.AsNode(), tc.name)
			result, prereqs := s.Capture(func() luau.Expression {
				return transformer.CreateTruthinessChecks(s, luau.ID("x"), node, s.GetType(node))
			})
			if prereqs.IsNonEmpty() {
				t.Errorf("unexpected prerequisite statements for simple expression")
			}
			if got := render.Render(render.NewRenderState(), result); got != tc.want {
				t.Errorf("rendered checks = %q, want %q", got, tc.want)
			}
		})
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics (LogTruthyChanges off): %v", ds)
	}
}

func TestCreateTruthinessChecksPushesComplexExpToVar(t *testing.T) {
	s := probeState(t)

	// checks needed + complex expression -> pushToVarIfComplex("value")
	node := mustOneIdentifier(t, s.SourceFile.AsNode(), "num")
	result, prereqs := s.Capture(func() luau.Expression {
		call := luau.NewCall(luau.GlobalID("f"), luau.NewList[luau.Expression]())
		return transformer.CreateTruthinessChecks(s, call, node, s.GetType(node))
	})
	prereqs.Push(luau.NewReturn(result))
	want := "local _value = f()\nreturn _value ~= 0 and _value == _value and _value\n"
	if got := render.RenderAST(prereqs); got != want {
		t.Errorf("rendered = %q, want %q", got, want)
	}

	// no checks needed -> complex expression stays inline
	boolNode := mustOneIdentifier(t, s.SourceFile.AsNode(), "bool")
	result, prereqs = s.Capture(func() luau.Expression {
		call := luau.NewCall(luau.GlobalID("f"), luau.NewList[luau.Expression]())
		return transformer.CreateTruthinessChecks(s, call, boolNode, s.GetType(boolNode))
	})
	if prereqs.IsNonEmpty() {
		t.Errorf("boolean type must not push to var")
	}
	if got := render.Render(render.NewRenderState(), result); got != "f()" {
		t.Errorf("rendered = %q, want %q", got, "f()")
	}
}

func TestCreateTruthinessChecksTruthyChangeWarning(t *testing.T) {
	s := probeState(t)
	s.LogTruthyChanges = true

	emit := func(name string) []transformer.Diagnostic {
		node := mustOneIdentifier(t, s.SourceFile.AsNode(), name)
		s.CaptureStatements(func() {
			transformer.CreateTruthinessChecks(s, luau.ID("x"), node, s.GetType(node))
		})
		return s.Diags.Flush()
	}

	for _, tc := range []struct {
		name    string
		message string // "" = no warning
	}{
		{"numStr", `Value will be checked against 0, NaN, ""`},
		{"num", "Value will be checked against 0, NaN"},
		{"zero", "Value will be checked against 0, NaN"}, // workaround lists NaN too
		{"str", `Value will be checked against ""`},
		{"bool", ""},
		{"five", ""},
	} {
		ds := emit(tc.name)
		if tc.message == "" {
			if len(ds) != 0 {
				t.Errorf("%s: unexpected diagnostics %v", tc.name, ds)
			}
			continue
		}
		if len(ds) != 1 {
			t.Errorf("%s: got %d diagnostics, want 1", tc.name, len(ds))
			continue
		}
		if ds[0].Code != "truthyChange" || !ds[0].Warning || ds[0].Message != tc.message {
			t.Errorf("%s: diagnostic = %+v, want truthyChange warning %q", tc.name, ds[0], tc.message)
		}
	}
}
