package transformer_test

import (
	"path/filepath"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// All expected text in this file (and namespaces_test.go) is byte-for-byte
// what rbxtsc 3.0.0 emits for the same source (compiled through
// testdata/diff/project on 2026-06-07; header and trailing export return
// stripped — those belong to TransformSourceFile, not the statement list
// under test).

func renderModuleFixture(t *testing.T, projDir, relPath string) (string, *transformer.State) {
	t.Helper()
	s := buildState(t, filepath.Join("..", "..", "testdata", "transformer-unit", projDir), relPath)
	// TransformSourceFile maps the file's module symbol to `exports` before
	// transforming statements; reads of exported symbols consult it.
	if symbol := s.Checker.GetSymbolAtLocation(s.SourceFile.AsNode()); symbol != nil {
		s.SetModuleIDBySymbol(symbol, luau.GlobalID("exports"))
	}
	statements := transformer.TransformStatementList(s, s.SourceFile.AsNode(), s.SourceFile.Statements.Nodes, nil)
	return render.RenderAST(statements), s
}

// TestEnumBasic pins every non-hoisted enum shape (digest §4.2, oracle §6.3):
//
//   - numeric enums (implicit, explicit, and explicit-then-implicit
//     continuation) take the general path: `local E` + do-block with
//     `local _inverse = {}`, `E = setmetatable({}, { __index = _inverse })`,
//     and INTERLEAVED member/inverse assignments per member;
//   - string-only enums collapse to a plain map (no do-block, no inverse);
//   - heterogeneous enums use the general path, string members simply skip
//     their inverse entry;
//   - the checker REALLY folds constant expressions (`X = base * 2` with
//     `const base = 10` emits `Computed.X = 20`); only unfoldable computed
//     members (`"y".size()`) spill to `local _value = #"y"` with `_value`
//     reused for member and inverse assignment;
//   - const enums emit NOTHING and member accesses inline (`print(0, 1)`);
//   - `declare enum` emits nothing (dispatch declare skip);
//   - `export enum` needs nothing special (the export table picks the local
//     up — outside this statement-list test);
//   - non-identifier member names render as `Weird["a b"]`.
func TestEnumBasic(t *testing.T) {
	got, s := renderModuleFixture(t, "enums", "src/basic.ts")

	want := `local Fruit
do
	local _inverse = {}
	Fruit = setmetatable({}, {
		__index = _inverse,
	})
	Fruit.Apple = 0
	_inverse[0] = "Apple"
	Fruit.Banana = 1
	_inverse[1] = "Banana"
	Fruit.Cherry = 2
	_inverse[2] = "Cherry"
end
local Mixed
do
	local _inverse = {}
	Mixed = setmetatable({}, {
		__index = _inverse,
	})
	Mixed.A = 5
	_inverse[5] = "A"
	Mixed.B = 6
	_inverse[6] = "B"
	Mixed.C = 10
	_inverse[10] = "C"
end
local Color = {
	Red = "red",
	Green = "green",
}
local Hetero
do
	local _inverse = {}
	Hetero = setmetatable({}, {
		__index = _inverse,
	})
	Hetero.Num = 1
	_inverse[1] = "Num"
	Hetero.Str = "str"
end
local base = 10
local Computed
do
	local _inverse = {}
	Computed = setmetatable({}, {
		__index = _inverse,
	})
	Computed.X = 20
	_inverse[20] = "X"
	local _value = #"y"
	Computed.Y = _value
	_inverse[_value] = "Y"
end
local Exported
do
	local _inverse = {}
	Exported = setmetatable({}, {
		__index = _inverse,
	})
	Exported.E1 = 0
	_inverse[0] = "E1"
	Exported.E2 = 1
	_inverse[1] = "E2"
end
local Weird
do
	local _inverse = {}
	Weird = setmetatable({}, {
		__index = _inverse,
	})
	Weird["a b"] = 1
	_inverse[1] = "a b"
	Weird.ok = 2
	_inverse[2] = "ok"
end
local function uses()
	print(Fruit.Apple, Fruit[1], Mixed.B, Color.Red, Hetero.Str, Computed.X, Computed.Y, Weird.ok)
	print(0, 1)
	print(Exported.E2)
end
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestEnumHoisted pins the use-before-declare interplay (oracle §6.4): the
// hoist machinery emits `local E` at the premature use, so the enum transform
// skips its own `local E` header — the do-block's metatable line becomes the
// hoist local's assignment. Hoisted STRING enums take the Assignment branch
// of the fast path (`SE = { ... }`).
func TestEnumHoisted(t *testing.T) {
	got, s := renderModuleFixture(t, "enums", "src/hoist.ts")

	want := `local E
local function early()
	return E.A
end
do
	local _inverse = {}
	E = setmetatable({}, {
		__index = _inverse,
	})
	E.A = 0
	_inverse[0] = "A"
end
local SE
local function earlyStr()
	return SE.S
end
SE = {
	S = "s",
}
`
	if got != want {
		t.Errorf("rendered output differs from rbxtsc:\ngot:\n%s\nwant:\n%s", got, want)
	}
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Errorf("unexpected diagnostics: %v", ds)
	}
}

// TestEnumMerging pins the merging ban (digest §4.2/§7): both declarations of
// a merged enum emit nothing, and noEnumMerging is reported exactly ONCE per
// symbol (AddDiagnosticWithCache + IsReportedByMultipleDefinitionsCache),
// positioned at the first declaration transformed.
func TestEnumMerging(t *testing.T) {
	got, s := renderModuleFixture(t, "enums", "src/merge.ts")

	if got != "" {
		t.Errorf("merged enum should emit nothing, got:\n%s", got)
	}
	ds := s.Diags.Flush()
	if len(ds) != 1 {
		t.Fatalf("expected exactly 1 diagnostic, got %d: %v", len(ds), ds)
	}
	if ds[0].Code != "noEnumMerging" {
		t.Errorf("expected noEnumMerging, got %q", ds[0].Code)
	}
	if ds[0].Message != "Enum merging is not supported!" {
		t.Errorf("unexpected message: %q", ds[0].Message)
	}
}
