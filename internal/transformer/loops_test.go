package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
)

// The expected text in every test below is byte-for-byte what rbxtsc 3.0.0
// emits for the same source (verified by compiling each file through
// testdata/diff/project; header and trailing `return nil` stripped — those
// belong to TransformSourceFile, not the statement lists under test).

// renderFileStatements transforms the whole file's statement list and renders
// it, failing on any diagnostics.
func renderFileStatements(t *testing.T, relPath string) string {
	t.Helper()
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "control"), relPath)
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
	return render.RenderAST(statements)
}

// TestWhileConditionWithPrereqs: a condition whose transform produces
// prereqs (`i++ < 3`) cannot live in the while header — it is re-evaluated
// every iteration via `while true do` + prereqs + `if not cond then break`.
func TestWhileConditionWithPrereqs(t *testing.T) {
	want := `local i = 0
while true do
	local _original = i
	i += 1
	if not (_original < 3) then
		break
	end
	print(i)
end
`
	if got := renderFileStatements(t, "src/while.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestOptimizedNumericForLiteralBound: `for (let i = 0; i < 10; i++)` meets
// every precondition of the optimized numeric-for pass; the `<` bound is
// offset by -1 and constant-folds (`10` -> `9`). The non-literal-bound
// variant (`i < limit` -> `limit - 1`) is covered by diff fixture 05_control.
func TestOptimizedNumericForLiteralBound(t *testing.T) {
	want := `local total = 0
for i = 0, 9 do
	total += i
end
print(total)
`
	if got := renderFileStatements(t, "src/optfor.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestGeneralForStatement: condition `i !== 5` disqualifies the optimized
// pass, taking the fallback while-loop desugaring — outer `do` scope, the
// `_shouldIncrement` guard (increment at the top of every iteration except
// the first), and the relocated condition check.
func TestGeneralForStatement(t *testing.T) {
	want := `do
	local i = 0
	local _shouldIncrement = false
	while true do
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 5) then
			break
		end
		print(i)
	end
end
`
	if got := renderFileStatements(t, "src/genfor.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestBareReturnInNestedBlock: `return;` emits an explicit `return nil`
// (preserving JS `undefined`), and a free-standing block becomes `do ... end`.
// Functions are not transformable yet, so the function BODY's statement list
// is transformed directly (rbxtsc's body output is the comparison text).
func TestBareReturnInNestedBlock(t *testing.T) {
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", "control"), "src/return.ts")

	declaration := s.SourceFile.Statements.Nodes[0]
	if !ast.IsFunctionDeclaration(declaration) {
		t.Fatalf("expected FunctionDeclaration first, got %v", declaration.Kind)
	}
	body := declaration.AsFunctionDeclaration().Body
	statements := transformer.TransformStatementList(s, body, body.AsBlock().Statements.Nodes, nil)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}

	want := `do
	if value > 0 then
		return nil
	end
	print("nested")
end
print(value)
`
	if got := render.RenderAST(statements); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestThrowString: `throw "..."` becomes a plain `error("...")` call
// statement (no type checks upstream).
func TestThrowString(t *testing.T) {
	want := `local bad = math.random() > 2
if bad then
	error("something went wrong")
end
print("ok")
`
	if got := renderFileStatements(t, "src/throw.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestOptimizedLoopClosureNoCopies: a closure capturing the loop variable
// does NOT disqualify the optimized numeric-for pass (its loop variable is
// per-iteration in Luau already), so no copy machinery appears.
func TestOptimizedLoopClosureNoCopies(t *testing.T) {
	want := `local fns = {}
for i = 0, 2 do
	fns[i + 1] = function()
		return i
	end
end
print(fns[1]())
`
	if got := renderFileStatements(t, "src/optclosure.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestLoopClosureCaptureCopies: condition `i !== 3` forces the fallback, and
// the closure read of `i` (the "async read") triggers the per-iteration copy
// machinery — outer `local _i = 0` slot, `local i = _i` body-head copy, and
// the `_i = i` finalizer write-back. This input panicked in Phase 2.
func TestLoopClosureCaptureCopies(t *testing.T) {
	want := `do
	local _i = 0
	local _shouldIncrement = false
	while true do
		local i = _i
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 3) then
			break
		end
		local f = function()
			return i
		end
		print(f())
		_i = i
	end
end
`
	if got := renderFileStatements(t, "src/capture.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestLoopCopiesContinueFinalizer: a body WRITE to the loop variable (outside
// the incrementor) also triggers copies, and a `continue` in the body gets a
// clone of the finalizer (`_i = i`) spliced in front of it — inside the if
// statement, before the continue.
func TestLoopCopiesContinueFinalizer(t *testing.T) {
	want := `local total = 0
do
	local _i = 0
	local _shouldIncrement = false
	while true do
		local i = _i
		if _shouldIncrement then
			i += 1
		else
			_shouldIncrement = true
		end
		if not (i ~= 8) then
			break
		end
		if i == 2 then
			i = i + 1
			_i = i
			continue
		end
		total += i
		_i = i
	end
end
print(total)
`
	if got := renderFileStatements(t, "src/contfinal.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestLoopFinalizerNotSplicedIntoNestedLoop: the OUTER loop variable needs
// copies (body write), but the `continue` inside the NESTED loop targets the
// inner loop — addFinalizers must not descend into nested loop bodies, so the
// inner continue stays bare and `_i = i` appears only at the outer body tail.
// The inner loop variable `j` is only written by its incrementor, so the
// inner loop gets no copies at all.
func TestLoopFinalizerNotSplicedIntoNestedLoop(t *testing.T) {
	want := `local hits = 0
do
	local _i = 0
	while true do
		local i = _i
		if not (i ~= 3) then
			break
		end
		i = i + 1
		do
			local j = 0
			local _shouldIncrement = false
			while true do
				if _shouldIncrement then
					j += 1
				else
					_shouldIncrement = true
				end
				if not (j ~= 2) then
					break
				end
				if j == 1 then
					continue
				end
				hits += 1
			end
		end
		_i = i
	end
end
print(hits)
`
	if got := renderFileStatements(t, "src/nestedcont.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestLoopWriteOnlyVariableCopies: a loop variable written in the body with
// no incrementor and no closure still gets the copy treatment (no
// `_shouldIncrement` gate — there is no incrementor in the source). This is
// Phase 2's documented byte-divergence, now closed.
func TestLoopWriteOnlyVariableCopies(t *testing.T) {
	want := `local calls = 0
do
	local _j = 0
	while true do
		local j = _j
		if not (j ~= 5) then
			break
		end
		j = j + 2
		calls += 1
		_j = j
	end
end
print(calls)
`
	if got := renderFileStatements(t, "src/writeonly.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
