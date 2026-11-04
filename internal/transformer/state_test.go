package transformer

import (
	"context"
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/checker"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

func TestCapture(t *testing.T) {
	s := NewTestState() // helper: state with nil checker for mechanics tests
	expr, prereqs := s.Capture(func() luau.Expression {
		s.Prereq(luau.NewVariableDeclaration(luau.ID("x"), luau.Num(1)))
		return luau.ID("x")
	})
	if prereqs.Size() != 1 {
		t.Fatal("prereq not captured")
	}
	if _, ok := expr.(*luau.Identifier); !ok {
		t.Fatal("wrong expr")
	}
	// stack must be balanced: capturing again works
	_, p2 := s.Capture(func() luau.Expression { return luau.Nil() })
	if p2.Size() != 0 {
		t.Fatal("stack leaked")
	}
}

func TestPushToVarIfComplex(t *testing.T) {
	s := NewTestState()
	// simple kinds pass through untouched
	id := luau.ID("x")
	if got := s.PushToVarIfComplex(id, "val"); got != luau.Expression(id) {
		t.Fatal("identifier must not be pushed")
	}
	// complex expressions get a temp + prereq
	_, prereqs := s.Capture(func() luau.Expression {
		return s.PushToVarIfComplex(luau.NewBinary(luau.ID("a"), "+", luau.ID("b")), "sum")
	})
	if prereqs.Size() != 1 {
		t.Fatal("complex expr must produce a local prereq")
	}
}

func TestPushToVarIfNonID(t *testing.T) {
	s := NewTestState()
	// Identifier and TemporaryIdentifier pass through untouched...
	id := luau.ID("x")
	if got := s.PushToVarIfNonID(id, "val"); got != luau.AnyIdentifier(id) {
		t.Fatal("identifier must not be pushed")
	}
	temp := luau.TempID("t")
	if got := s.PushToVarIfNonID(temp, "val"); got != luau.AnyIdentifier(temp) {
		t.Fatal("temporary identifier must not be pushed")
	}
	// ...but simple literals (which PushToVarIfComplex would skip) ARE pushed.
	prereqs := s.CaptureStatements(func() {
		if _, ok := s.PushToVarIfNonID(luau.Str("hi"), "str").(*luau.TemporaryIdentifier); !ok {
			t.Fatal("non-identifier must become a temp")
		}
	})
	if prereqs.Size() != 1 {
		t.Fatal("non-identifier must produce a local prereq")
	}
}

func TestNoPrereqsPanics(t *testing.T) {
	s := NewTestState()
	// happy path: no prereqs created
	expr := s.NoPrereqs(func() luau.Expression { return luau.Nil() })
	if _, ok := expr.(*luau.NilLiteral); !ok {
		t.Fatal("wrong expr")
	}
	// adding a prereq inside NoPrereqs must panic (upstream assert)
	defer func() {
		if recover() == nil {
			t.Fatal("NoPrereqs must panic when a prereq is added")
		}
		// the deferred pop must have kept the stack balanced
		if len(s.prereqStack) != 0 {
			t.Fatal("prereq stack corrupted after panic")
		}
	}()
	s.NoPrereqs(func() luau.Expression {
		s.Prereq(luau.NewBreak())
		return luau.Nil()
	})
}

func TestNestedCaptureIsolation(t *testing.T) {
	s := NewTestState()
	_, outer := s.Capture(func() luau.Expression {
		s.Prereq(luau.NewVariableDeclaration(luau.ID("a"), luau.Num(1)))
		_, inner := s.Capture(func() luau.Expression {
			s.Prereq(luau.NewVariableDeclaration(luau.ID("b"), luau.Num(2)))
			s.Prereq(luau.NewVariableDeclaration(luau.ID("c"), luau.Num(3)))
			return luau.ID("b")
		})
		if inner.Size() != 2 {
			t.Fatal("inner capture must see exactly its own prereqs")
		}
		return luau.ID("a")
	})
	// inner prereqs must NOT leak into the outer list
	if outer.Size() != 1 {
		t.Fatalf("outer capture leaked inner prereqs: got %d, want 1", outer.Size())
	}
}

func TestRuntimeLib(t *testing.T) {
	s := NewTestState()
	if s.UsesRuntimeLib {
		t.Fatal("UsesRuntimeLib must start false")
	}
	expr := s.RuntimeLib(nil, "instanceof")
	if !s.UsesRuntimeLib {
		t.Fatal("RuntimeLib must set UsesRuntimeLib")
	}
	prop, ok := expr.(*luau.PropertyAccessExpression)
	if !ok {
		t.Fatalf("RuntimeLib must return a PropertyAccessExpression, got %T", expr)
	}
	if prop.Name != "instanceof" {
		t.Fatalf("property name = %q, want %q", prop.Name, "instanceof")
	}
	base, ok := prop.Expression.(*luau.Identifier)
	if !ok || base.Name != "TS" {
		t.Fatalf("base must be Identifier %q, got %#v", "TS", prop.Expression)
	}
	// no warning outside ReplicatedFirst game files
	if len(s.Diags.Flush()) != 0 {
		t.Fatal("unexpected diagnostics")
	}

	// ReplicatedFirst game files warn on every call (upstream: not deduped)
	s.ProjectType = ProjectTypeGame
	s.IsInReplicatedFirst = true
	s.RuntimeLib(nil, "async")
	s.RuntimeLib(nil, "await")
	ds := s.Diags.Flush()
	if len(ds) != 2 {
		t.Fatalf("got %d diagnostics, want 2", len(ds))
	}
	for _, d := range ds {
		if d.Code != "runtimeLibUsedInReplicatedFirst" || !d.Warning {
			t.Fatalf("unexpected diagnostic %+v", d)
		}
	}
}

// TestPushToVarNameHints checks the valueToIdStr hint-derivation port and the
// upstream `name || valueToIdStr(expression)` fallback shape.
func TestPushToVarNameHints(t *testing.T) {
	s := NewTestState()
	tests := []struct {
		name string
		expr luau.Expression
		hint string
		want string
	}{
		// explicit hint always wins
		{"explicit hint", luau.NewBinary(luau.ID("a"), "+", luau.ID("b")), "sum", "sum"},
		// Identifier X -> "x" (uncapitalized)
		{"identifier", luau.ID("Foo"), "", "foo"},
		// PropertyAccess A.B -> "b"
		{"property access", luau.NewPropertyAccess(luau.ID("A"), "Bar"), "", "bar"},
		// CallExpression X.new() -> "x"
		{"constructor call", luau.NewCall(luau.NewPropertyAccess(luau.ID("Foo"), "new"), luau.NewList[luau.Expression]()), "", "foo"},
		// non-"new" calls and other expressions -> anonymous ""
		{"plain call", luau.NewCall(luau.ID("f"), luau.NewList[luau.Expression]()), "", ""},
		{"binary", luau.NewBinary(luau.ID("a"), "+", luau.ID("b")), "", ""},
		// invalid Luau identifier names -> anonymous ""
		{"reserved word", luau.NewPropertyAccess(luau.ID("a"), "end"), "", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s.CaptureStatements(func() {
				temp := s.PushToVar(tt.expr, tt.hint)
				if temp.Name != tt.want {
					t.Fatalf("temp name hint = %q, want %q", temp.Name, tt.want)
				}
			})
		})
	}
}

func TestValueToIdStr(t *testing.T) {
	// X.new() nested through a property: A.B.new() -> "b"
	inner := luau.NewPropertyAccess(luau.ID("A"), "B")
	call := luau.NewCall(luau.NewPropertyAccess(inner, "new"), luau.NewList[luau.Expression]())
	if got := ValueToIdStr(call); got != "b" {
		t.Fatalf("ValueToIdStr(A.B.new()) = %q, want %q", got, "b")
	}
	// temp identifiers are NOT luau.isIdentifier upstream -> ""
	if got := ValueToIdStr(luau.TempID("t")); got != "" {
		t.Fatalf("ValueToIdStr(tempId) = %q, want %q", got, "")
	}
}

func TestGetTypeMemoization(t *testing.T) {
	dir, err := filepath.Abs(filepath.Join("..", "..", "testdata", "spike-unit", "spike"))
	if err != nil {
		t.Fatal(err)
	}
	dir = filepath.ToSlash(dir)

	fs := bundled.WrapFS(osvfs.FS())
	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)

	configPath := dir + "/tsconfig.json"
	parsed, diags := tsoptions.GetParsedCommandLineOfConfigFile(configPath, nil, nil, host, nil)
	if len(diags) > 0 {
		t.Fatalf("config diagnostics: %v", diags)
	}

	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
	ctx := context.Background()
	chk, release := program.GetTypeChecker(ctx)
	defer release()

	sf := program.GetSourceFile(dir + "/src/main.ts")
	if sf == nil {
		t.Fatal("source file not found")
	}

	s := NewState(program, chk, sf, NewDiagService(), NewMultiState())
	if s.SourceFileText == "" {
		t.Fatal("SourceFileText must be captured from the source file")
	}

	// first top-level variable declaration node
	var decl *ast.Node
	for _, stmt := range sf.Statements.Nodes {
		if stmt.Kind == ast.KindVariableStatement {
			decl = stmt.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes[0]
			break
		}
	}
	if decl == nil {
		t.Fatal("no variable declaration in fixture")
	}

	t1 := s.GetType(decl)
	if t1 == nil {
		t.Fatal("GetType returned nil")
	}
	if s.GetType(decl) != t1 {
		t.Fatal("GetType must return the cached type")
	}
	// prove the second call reads the cache, not the checker: poison the cache
	sentinel := &checker.Type{}
	s.getTypeCache[decl] = sentinel
	if s.GetType(decl) != sentinel {
		t.Fatal("GetType must be memoized by node pointer")
	}
}
