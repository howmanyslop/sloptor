package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/transformer"
	"rotor/tsgo/ast"
)

// refsFixtureState builds the walker fixture and returns the state plus the
// declaration-name identifiers for the outer `counter`, the shadowed inner
// `counter`, and `lonely`.
func refsFixtureState(t *testing.T) (s *transformer.State, outerDef, innerDef, lonelyDef *ast.Node) {
	t.Helper()
	s = buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "typeprobe"), "src/refs.ts")

	declName := func(statement *ast.Node) *ast.Node {
		t.Helper()
		if !ast.IsVariableStatement(statement) {
			t.Fatalf("expected VariableStatement, got %v", statement.Kind)
		}
		return statement.AsVariableStatement().DeclarationList.
			AsVariableDeclarationList().Declarations.Nodes[0].AsVariableDeclaration().Name()
	}

	statements := s.SourceFile.Statements.Nodes
	outerDef = declName(statements[0])
	var shadowFn *ast.Node
	for _, statement := range statements {
		if ast.IsFunctionDeclaration(statement) {
			shadowFn = statement
		}
	}
	if shadowFn == nil {
		t.Fatal("shadow function not found")
	}
	innerDef = declName(shadowFn.AsFunctionDeclaration().Body.AsBlock().Statements.Nodes[0])
	lonelyDef = declName(statements[len(statements)-1])
	return s, outerDef, innerDef, lonelyDef
}

// TestWalkerFindsReferencesInOrder: the walker reports exactly the outer
// `counter` references — write, read, shorthand-property value, closure
// read — in document order, skipping the definition itself and every
// shadowed (different-symbol) identifier of the same text.
func TestWalkerFindsReferencesInOrder(t *testing.T) {
	s, outerDef, _, _ := refsFixtureState(t)

	var refs []*ast.Node
	found := transformer.ForEachSymbolReference(s.Checker, outerDef, s.SourceFile.AsNode(), func(token *ast.Node) bool {
		refs = append(refs, token)
		return false
	})
	if found {
		t.Error("walker returned true although no callback returned true")
	}
	if len(refs) != 4 {
		t.Fatalf("got %d references, want 4", len(refs))
	}

	// 1: assignment LHS — a write; 2-4: reads.
	wantWrites := []bool{true, false, false, false}
	for i, ref := range refs {
		if got := ast.IsWriteAccess(ref); got != wantWrites[i] {
			t.Errorf("ref %d: IsWriteAccess = %v, want %v", i, got, wantWrites[i])
		}
	}

	// In-order: each ref starts after the previous one.
	for i := 1; i < len(refs); i++ {
		if refs[i].Pos() <= refs[i-1].Pos() {
			t.Errorf("refs out of document order at %d", i)
		}
	}

	// 3: the shorthand `{ counter }` — matched via the shorthand value
	// symbol, not getSymbolAtLocation (which yields the property symbol).
	if !ast.IsShorthandPropertyAssignment(refs[2].Parent) {
		t.Errorf("ref 2 parent = %v, want ShorthandPropertyAssignment", refs[2].Parent.Kind)
	}

	// 4: inside the arrow function.
	if ast.FindAncestor(refs[3], ast.IsArrowFunction) == nil {
		t.Error("ref 3 must sit inside the arrow function")
	}
}

// TestWalkerEarlyOut: a callback returning true stops the walk after the
// first reference and surfaces as the walker's return value.
func TestWalkerEarlyOut(t *testing.T) {
	s, outerDef, _, _ := refsFixtureState(t)

	visited := 0
	found := transformer.ForEachSymbolReference(s.Checker, outerDef, s.SourceFile.AsNode(), func(token *ast.Node) bool {
		visited++
		return true
	})
	if !found {
		t.Error("walker must return true when the callback does")
	}
	if visited != 1 {
		t.Errorf("visited %d references, want 1 (early-out)", visited)
	}
}

// TestWalkerScopedContainer: only references inside the searchContainer are
// reported — the arrow function contains exactly one of the four.
func TestWalkerScopedContainer(t *testing.T) {
	s, outerDef, _, _ := refsFixtureState(t)

	var arrow *ast.Node
	var find func(node *ast.Node) bool
	find = func(node *ast.Node) bool {
		if ast.IsArrowFunction(node) {
			arrow = node
			return true
		}
		return node.ForEachChild(find)
	}
	find(s.SourceFile.AsNode())
	if arrow == nil {
		t.Fatal("arrow function not found")
	}

	visited := 0
	transformer.ForEachSymbolReference(s.Checker, outerDef, arrow, func(token *ast.Node) bool {
		visited++
		return false
	})
	if visited != 1 {
		t.Errorf("visited %d references in arrow container, want 1", visited)
	}
}

// TestWalkerShadowedSymbol: the inner `counter` resolves to its own symbol —
// exactly two references (compound write + return read), both inside the
// function, with the outer four untouched.
func TestWalkerShadowedSymbol(t *testing.T) {
	s, _, innerDef, _ := refsFixtureState(t)

	var refs []*ast.Node
	transformer.ForEachSymbolReference(s.Checker, innerDef, s.SourceFile.AsNode(), func(token *ast.Node) bool {
		refs = append(refs, token)
		return false
	})
	if len(refs) != 2 {
		t.Fatalf("got %d references for shadowed symbol, want 2", len(refs))
	}
	if !ast.IsWriteAccess(refs[0]) {
		t.Error("compound assignment LHS must be a write access")
	}
	if ast.IsWriteAccess(refs[1]) {
		t.Error("return-statement read must not be a write access")
	}
	for i, ref := range refs {
		if ast.FindAncestor(ref, ast.IsFunctionDeclaration) == nil {
			t.Errorf("ref %d must sit inside the shadow function", i)
		}
	}
}

// TestIsSymbolReferenced: positive for a referenced symbol, negative for a
// declared-but-never-referenced one (the definition itself never counts).
func TestIsSymbolReferenced(t *testing.T) {
	s, outerDef, _, lonelyDef := refsFixtureState(t)

	if !transformer.IsSymbolReferenced(s.Checker, outerDef, s.SourceFile.AsNode()) {
		t.Error("IsSymbolReferenced(counter) = false, want true")
	}
	if transformer.IsSymbolReferenced(s.Checker, lonelyDef, s.SourceFile.AsNode()) {
		t.Error("IsSymbolReferenced(lonely) = true, want false")
	}
}
