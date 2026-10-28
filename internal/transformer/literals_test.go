package transformer

import (
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
)

// parseFirstExpression parses source as a TS file and returns a checker-free
// state plus the expression of the first statement (which must be an
// expression statement). Literal transforms never consult the checker, so the
// parse-only program is enough.
func parseFirstExpression(t *testing.T, source string) (*State, *ast.Node) {
	t.Helper()
	sourceFile := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/test.ts",
		Path:     tspath.Path("/test.ts"),
	}, source, core.ScriptKindTS)
	s := NewState(nil, nil, sourceFile, NewDiagService(), NewMultiState())
	statement := sourceFile.Statements.Nodes[0]
	if !ast.IsExpressionStatement(statement) {
		t.Fatalf("first statement is %s, want ExpressionStatement", statement.Kind)
	}
	return s, statement.AsExpressionStatement().Expression
}

func renderExpression(t *testing.T, expression luau.Expression) string {
	t.Helper()
	return render.Render(render.NewRenderState(), expression)
}

func TestTemplateWithTableExpressionWrapsInParens(t *testing.T) {
	s, expression := parseFirstExpression(t, "`${[1, 2]}`;")
	var result luau.Expression
	prereqs := s.CaptureStatements(func() { result = TransformExpression(s, expression) })
	if result == nil {
		t.Fatal("TransformExpression returned nil")
	}
	if prereqs.IsNonEmpty() {
		t.Fatal("template literal must not produce prereqs")
	}
	// The transformer emits the raw Array part; the RENDERER adds the parens
	// (`{{}}` is invalid Luau) — the transformer must not wrap or escape.
	interpolated, ok := result.(*luau.InterpolatedString)
	if !ok {
		t.Fatalf("got %T, want *luau.InterpolatedString", result)
	}
	if _, ok := interpolated.Parts.Head.Value.(*luau.Array); !ok {
		t.Fatalf("part = %T, want raw *luau.Array (renderer parenthesizes)", interpolated.Parts.Head.Value)
	}
	if got, want := renderExpression(t, result), "`{({ 1, 2 })}`"; got != want {
		t.Errorf("rendered = %q, want %q", got, want)
	}
	if diags := s.Diags.Flush(); len(diags) != 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}
}

func TestTemplatePartsPreserveRawEscapes(t *testing.T) {
	// Cooked text would turn \n into a newline; the emitted part must keep
	// the raw two-character escape (the renderer handles Luau escaping).
	s, expression := parseFirstExpression(t, "`a\\n${[1]}\\t{b}`;")
	result := s.NoPrereqs(func() luau.Expression { return TransformExpression(s, expression) })
	interpolated := result.(*luau.InterpolatedString)
	parts := interpolated.Parts.ToSlice()
	if len(parts) != 3 {
		t.Fatalf("got %d parts, want 3", len(parts))
	}
	if head := parts[0].(*luau.InterpolatedStringPart); head.Text != "a\\n" {
		t.Errorf("head text = %q, want %q (raw escape preserved)", head.Text, "a\\n")
	}
	if tail := parts[2].(*luau.InterpolatedStringPart); tail.Text != "\\t{b}" {
		t.Errorf("tail text = %q, want %q (renderer escapes the brace)", tail.Text, "\\t{b}")
	}
}

func TestObjectLiteralComputedKey(t *testing.T) {
	s, expression := parseFirstExpression(t, "({ x: 1, [\"computed key\"]: 3 });")
	// Reach through the parenthesized wrapper to transform the object itself.
	object := SkipDownwards(expression)
	var result luau.Expression
	prereqs := s.CaptureStatements(func() { result = TransformExpression(s, object) })
	if result == nil {
		t.Fatal("TransformExpression returned nil")
	}
	if prereqs.IsNonEmpty() {
		t.Fatal("literal-only object must stay inline (no prereqs)")
	}
	m, ok := result.(*luau.Map)
	if !ok {
		t.Fatalf("got %T, want inline *luau.Map", result)
	}
	fields := m.Fields.ToSlice()
	if len(fields) != 2 {
		t.Fatalf("got %d fields, want 2", len(fields))
	}
	// Identifier key -> string "x" (renderer emits `x = 1`); computed string
	// key -> string "computed key" (renderer emits `["computed key"] = 3`).
	if idx := fields[0].Index.(*luau.StringLiteral); idx.Value != "x" {
		t.Errorf("field 0 index = %q, want %q", idx.Value, "x")
	}
	if idx := fields[1].Index.(*luau.StringLiteral); idx.Value != "computed key" {
		t.Errorf("field 1 index = %q, want %q", idx.Value, "computed key")
	}
	want := "{\n\tx = 1,\n\t[\"computed key\"] = 3,\n}"
	if got := renderExpression(t, result); got != want {
		t.Errorf("rendered = %q, want %q", got, want)
	}
}

func TestNumericLiteralRawTextPassthrough(t *testing.T) {
	tests := []struct{ source, want string }{
		{"0xff;", "0xff"},
		{"0b101;", "0b101"},
		{"1_000_000;", "1_000_000"},
		{"3.25;", "3.25"},
	}
	for _, tt := range tests {
		s, expression := parseFirstExpression(t, tt.source)
		result := s.NoPrereqs(func() luau.Expression { return TransformExpression(s, expression) })
		number, ok := result.(*luau.NumberLiteral)
		if !ok {
			t.Fatalf("%s: got %T, want *luau.NumberLiteral", tt.source, result)
		}
		if number.Value != tt.want {
			t.Errorf("%s: value = %q, want %q (raw source text)", tt.source, number.Value, tt.want)
		}
	}
}

func TestStringLiteralRawTextSlicing(t *testing.T) {
	// Escapes pass through exactly as written in source — `\"` stays two
	// characters, `\\` stays two characters.
	s, expression := parseFirstExpression(t, `"escape \"quotes\" and \\ backslash";`)
	result := s.NoPrereqs(func() luau.Expression { return TransformExpression(s, expression) })
	str := result.(*luau.StringLiteral)
	if want := `escape \"quotes\" and \\ backslash`; str.Value != want {
		t.Errorf("value = %q, want %q", str.Value, want)
	}
}

// TestMapPointerDegradation documents the pointer mechanics of
// util/pointer.ts: fields stay on the inline map literal until a member with
// prereqs forces DisableMapInline, which materializes `local <name> = {...}`
// and turns later fields into `ptr[k] = v` prereq assignments — exactly what
// transformPropertyAssignment does when a key/value transform produced
// prereqs.
func TestMapPointerDegradation(t *testing.T) {
	s := NewTestState()
	statements := s.CaptureStatements(func() {
		ptr := CreateMapPointer("object")

		// While inline: fields land in the literal, no prereqs.
		AssignToMapPointer(s, ptr, luau.Str("a"), luau.Num(1))
		m, ok := ptr.Value.(*luau.Map)
		if !ok {
			t.Fatalf("pointer degraded too early: %T", ptr.Value)
		}
		if m.Fields.Size() != 1 {
			t.Fatalf("inline map has %d fields, want 1", m.Fields.Size())
		}

		// An element with prereqs (simulated) forces materialization.
		DisableMapInline(s, ptr)
		temp, ok := ptr.Value.(*luau.TemporaryIdentifier)
		if !ok {
			t.Fatalf("pointer value = %T, want *luau.TemporaryIdentifier", ptr.Value)
		}
		if temp.Name != "object" {
			t.Errorf("temp name hint = %q, want %q", temp.Name, "object")
		}
		// Disabling twice is a no-op (upstream guard).
		DisableMapInline(s, ptr)
		if ptr.Value != luau.Expression(temp) {
			t.Error("second DisableMapInline must not re-push")
		}

		// After materialization: fields become prereq assignments.
		AssignToMapPointer(s, ptr, luau.Str("b"), luau.Num(2))
	})

	// Temp ids render with the `_` prefix (`_object`).
	want := "local _object = {\n\ta = 1,\n}\n_object.b = 2\n"
	if got := render.RenderAST(statements); got != want {
		t.Errorf("prereqs rendered = %q, want %q", got, want)
	}
}

// TestArrayPointerDegradation covers the array twin (used by the spread path
// later; the inline-until-disabled contract is identical).
func TestArrayPointerDegradation(t *testing.T) {
	s := NewTestState()
	statements := s.CaptureStatements(func() {
		ptr := CreateArrayPointer("array")
		ptr.Value.(*luau.Array).Members.Push(luau.Num(1))
		DisableArrayInline(s, ptr)
		if _, ok := ptr.Value.(*luau.TemporaryIdentifier); !ok {
			t.Fatalf("pointer value = %T, want *luau.TemporaryIdentifier", ptr.Value)
		}
	})
	want := "local _array = { 1 }\n"
	if got := render.RenderAST(statements); got != want {
		t.Errorf("prereqs rendered = %q, want %q", got, want)
	}
}
