package transformer

import (
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
)

func TestChooseExportShape(t *testing.T) {
	tests := []struct {
		name                                                            string
		hasExportEquals, hasExportFrom, hasMutableExports, hasImmutable bool
		want                                                            ExportShape
	}{
		{"no exports", false, false, false, false, ExportShapeNone},
		{"export equals", true, false, false, false, ExportShapeExportEquals},
		{"export equals wins over everything", true, true, true, true, ExportShapeExportEquals},
		{"export from forces exports table", false, true, false, false, ExportShapeExportsTable},
		{"mutable export forces exports table", false, false, true, false, ExportShapeExportsTable},
		{"mutable + immutable still exports table", false, false, true, true, ExportShapeExportsTable},
		{"only immutable exports return map", false, false, false, true, ExportShapeReturnMap},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := ChooseExportShape(tt.hasExportEquals, tt.hasExportFrom, tt.hasMutableExports, tt.hasImmutable)
			if got != tt.want {
				t.Errorf("ChooseExportShape(%v, %v, %v, %v) = %v, want %v",
					tt.hasExportEquals, tt.hasExportFrom, tt.hasMutableExports, tt.hasImmutable, got, tt.want)
			}
		})
	}
}

func TestEnsureModuleReturn(t *testing.T) {
	render := func(list *luau.List[luau.Statement]) string { return render.RenderAST(list) }

	t.Run("empty ModuleScript gets return nil", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		ensureModuleReturn(list, true)
		if got := render(list); got != "return nil\n" {
			t.Errorf("got %q, want %q", got, "return nil\n")
		}
	})

	t.Run("non-return last statement gets return nil", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewVariableDeclaration(luau.ID("x"), luau.Num(1)))
		ensureModuleReturn(list, true)
		if got := render(list); got != "local x = 1\nreturn nil\n" {
			t.Errorf("got %q", got)
		}
	})

	t.Run("existing return suppresses return nil", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewReturn(luau.GlobalID("exports")))
		ensureModuleReturn(list, true)
		if got := render(list); got != "return exports\n" {
			t.Errorf("got %q", got)
		}
	})

	t.Run("trailing comments are skipped when finding the return", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewReturn(luau.GlobalID("exports")))
		list.Push(luau.NewComment(" trailing"))
		ensureModuleReturn(list, true)
		if got := render(list); got != "return exports\n-- trailing\n" {
			t.Errorf("got %q", got)
		}
	})

	t.Run("non-ModuleScript never gets return nil", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewVariableDeclaration(luau.ID("x"), luau.Num(1)))
		ensureModuleReturn(list, false)
		if got := render(list); got != "local x = 1\n" {
			t.Errorf("got %q", got)
		}
	})
}

func TestPrependHeader(t *testing.T) {
	t.Run("header is the first line", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewReturn(luau.Nil()))
		prependHeader(NewTestState(), list)
		want := "-- Compiled with sloptor v2.3.3\nreturn nil\n"
		if got := render.RenderAST(list); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("directive comments hoist above the header", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewComment("!strict"))
		list.Push(luau.NewComment(" regular comment"))
		list.Push(luau.NewReturn(luau.Nil()))
		prependHeader(NewTestState(), list)
		want := "--!strict\n-- Compiled with sloptor v2.3.3\n-- regular comment\nreturn nil\n"
		if got := render.RenderAST(list); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("runtime-lib import joins the header after the version comment", func(t *testing.T) {
		// transformSourceFile.ts:228-230; directive comments still hoist
		// above the whole header. A state without a Rojo context lands on the
		// Package `_G[script]` form (runtimeLibRbxPath undefined upstream).
		s := NewTestState()
		s.UsesRuntimeLib = true
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewComment("!strict"))
		list.Push(luau.NewReturn(luau.Nil()))
		prependHeader(s, list)
		want := "--!strict\n-- Compiled with sloptor v2.3.3\nlocal TS = _G[script]\nreturn nil\n"
		if got := render.RenderAST(list); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("directive scan stops at first non-directive", func(t *testing.T) {
		list := luau.NewList[luau.Statement]()
		list.Push(luau.NewComment(" leading"))
		list.Push(luau.NewComment("!native")) // not at head: stays below the header
		list.Push(luau.NewReturn(luau.Nil()))
		prependHeader(NewTestState(), list)
		want := "-- Compiled with sloptor v2.3.3\n-- leading\n--!native\nreturn nil\n"
		if got := render.RenderAST(list); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
}

func TestTransformStatementListPanicsWithoutDispatch(t *testing.T) {
	// dispatch.go wires TransformStatement in init(); simulate the unwired
	// state to keep the guard covered.
	prev := TransformStatement
	TransformStatement = nil
	defer func() { TransformStatement = prev }()

	defer func() {
		r := recover()
		if r == nil {
			t.Fatal("expected panic when dispatch is not wired")
		}
		if msg, ok := r.(string); !ok || msg != "transformer: statement dispatch not wired" {
			t.Fatalf("unexpected panic value: %v", r)
		}
	}()
	s := NewTestState()
	// The dispatch nil-check fires before the statement node is touched, so
	// a single nil entry is enough to reach it.
	TransformStatementList(s, nil, []*ast.Node{nil}, nil)
}
