package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// The expected text in every test below is byte-for-byte what rbxtsc 3.0.0
// emits for the same source (verified via scratch files compiled through
// testdata/diff/project — see tools/oracle/oracle.ps1 for the technique),
// minus the version header and the synthesized `return nil` module tail.

func renderSwitchFile(t *testing.T, relPath string) string {
	t.Helper()
	s := buildState(t, filepath.Join("testdata", "switch"), relPath)
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
	return render.RenderAST(statements)
}

// TestSwitchCasePrereqsFallthrough: a case expression with prereqs (`i++`)
// reachable by fallthrough takes the guarded variant — the prereqs run only
// when not already falling through, the flag takes over the comparison
// (`_fallthrough = _exp == _original`), and the clause condition collapses to
// just `_fallthrough`. The preceding empty case emits only the flag update.
// Operand capture follows 3.0.0-dev-3106b14 and is covered by the shared runtime fixture.
func TestSwitchCasePrereqsFallthrough(t *testing.T) {
	want := `local i = 10
local function pick(n)
	local _exp = n
	repeat
		local _fallthrough = false
		if _exp == 0 then
			_fallthrough = true
		end
		if not _fallthrough then
			local _original = i
			i += 1
			_fallthrough = _exp == _original
		end
		if _fallthrough then
			return "low"
		end
		return "high"
	until true
end
print(pick(0), i)
`
	if got := renderSwitchFile(t, "src/prereq.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestSwitchCrossClauseHoist: the checkVariableHoist end-to-end proof — a
// case-local declared in case 0 and written from case 1 hoists to `local
// shared` BEFORE case 0's `if` (keyed on the CaseClause, emitted by
// createHoistDeclaration inside transformCaseClause), and the declaration
// inside the clause demotes to an assignment. No `_fallthrough` is declared:
// both bodies end in `break`.
func TestSwitchCrossClauseHoist(t *testing.T) {
	want := `local function describe(n)
	local result = 0
	repeat
		local shared
		if n == 0 then
			shared = n + 1
			result = shared
			break
		end
		if n == 1 then
			shared = n + 2
			result = shared
			break
		end
	until true
	return result
end
print(describe(0), describe(1))
`
	if got := renderSwitchFile(t, "src/casehoist.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestSwitchClausesAfterDefaultDropped: upstream quirk — the clause loop
// breaks at the default clause, so `case 1` and `case 2` AFTER it vanish from
// the output entirely (TS would still match them; ported verbatim). Clauses
// BEFORE the default emit normally.
func TestSwitchClausesAfterDefaultDropped(t *testing.T) {
	want := `local x = 5
local y = 0
repeat
	if x == 0 then
		y = 9
		break
	end
	y = 1
until true
print(y)
`
	if got := renderSwitchFile(t, "src/afterdefault.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestSwitchInsideLoop: TS break-in-switch emits a plain Luau `break` that
// exits the `repeat ... until true` — the enclosing numeric for is untouched.
// The complex subject (`i % 3`) gets `local _exp` as a prereq before the
// repeat. PORT NOTE quirk: the TS `continue` (which targets the LOOP in TS)
// emits a Luau `continue` inside the repeat, where it jumps to the repeat's
// `until` and acts like a switch-break instead — `total += 100` still runs.
// Upstream emits exactly this; pinned byte-for-byte.
func TestSwitchInsideLoop(t *testing.T) {
	want := `local total = 0
for i = 0, 4 do
	local _exp = i % 3
	repeat
		if _exp == 0 then
			total += 1
			break
		end
		if _exp == 1 then
			continue
		end
		total += 10
	until true
	total += 100
end
print(total)
`
	if got := renderSwitchFile(t, "src/inloop.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNestedSwitch: each switch is its own repeat; the inner `break` exits
// the inner repeat, the outer clause's trailing `break` exits the outer one.
func TestNestedSwitch(t *testing.T) {
	want := `local function grid(a, b)
	local r = 0
	repeat
		if a == 1 then
			repeat
				if b == 1 then
					r = 11
					break
				end
				r = 19
			until true
			break
		end
		r = 99
	until true
	return r
end
print(grid(1, 1), grid(1, 5), grid(2, 0))
`
	if got := renderSwitchFile(t, "src/nested.ts"); got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
