package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/transformer"
)

// The CALL_MACROS happy paths (assert truthiness expansion, typeOf,
// typeIs type()/typeof() split, classIs, identity, $tuple-in-return,
// $getModuleTree) are oracle-pinned in the 28_collectionmacros diff fixture;
// these tests cover the misuse diagnostics, which abort the compile before
// any output exists to diff.

// TestRangeMacroOutsideForOf: `$range(...)` anywhere but a for-of loop
// expression reaches the CALL macro, which is the error path
// (noRangeMacroOutsideForOf) — the for-of transform intercepts the valid
// position before the expression transform.
func TestRangeMacroOutsideForOf(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/rangemisuse.ts")
	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	if ds := s.Diags.Flush(); !hasDiagnostic(ds, "noRangeMacroOutsideForOf", "only valid as an expression of a for-of loop") {
		t.Errorf("no noRangeMacroOutsideForOf diagnostic; got: %v", ds)
	}
}

// TestTupleMacroOutsideReturn: `$tuple(...)` anywhere but a return
// expression reaches the CALL macro, which is the error path
// (noTupleMacroOutsideReturn) — transformReturnStatementInner intercepts the
// valid position before the expression transform.
func TestTupleMacroOutsideReturn(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/tuplemisuse.ts")
	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	if ds := s.Diags.Flush(); !hasDiagnostic(ds, "noTupleMacroOutsideReturn", "only valid as an expression of a return statement") {
		t.Errorf("no noTupleMacroOutsideReturn diagnostic; got: %v", ds)
	}
}

// TestCallMacroIndexWithoutCall: a call-macro identifier referenced without
// invoking it (`const f = identity`) has no value to emit —
// transformIdentifier's misuse guard raises noIndexWithoutCall (upstream
// transformIdentifier.ts L152-159).
func TestCallMacroIndexWithoutCall(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "calls"), "src/indexwithoutcall.ts")
	transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	if ds := s.Diags.Flush(); !hasDiagnostic(ds, "noIndexWithoutCall", "Cannot index a method without calling it") {
		t.Errorf("no noIndexWithoutCall diagnostic; got: %v", ds)
	}
}
