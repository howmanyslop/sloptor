package render

import (
	"strings"
	"testing"

	"rotor/internal/luau"
)

// buildGoldenAST constructs a program shaped like typical transformer output.
func buildGoldenAST() *luau.List[luau.Statement] {
	// local function map(array, callback)
	// 	local result = {}
	// 	for i, v in ipairs(array) do
	// 		result[i] = callback(v)
	// 	end
	// 	return result
	// end
	arrayID, callbackID := luau.ID("array"), luau.ID("callback")
	resultID := luau.ID("result")
	iID, vID := luau.ID("i"), luau.ID("v")
	fnBody := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(resultID, luau.NewArray(luau.NewList[luau.Expression]())),
		luau.NewFor(
			luau.NewList[luau.AnyIdentifier](iID, vID),
			luau.NewCall(luau.ID("ipairs"), luau.NewList[luau.Expression](arrayID)),
			luau.NewList[luau.Statement](
				luau.NewAssignment(
					luau.NewComputedIndex(resultID, iID),
					"=",
					luau.NewCall(callbackID, luau.NewList[luau.Expression](vID)),
				),
			),
		),
		luau.NewReturn(resultID),
	)
	mapDecl := luau.NewFunctionDeclaration(
		true, luau.ID("map"),
		luau.NewList[luau.AnyIdentifier](arrayID, callbackID), false,
		fnBody,
	)

	// local doubled = map({ 1, 2, 3 }, function(n)
	// 	return n * 2
	// end)
	nID := luau.ID("n")
	callbackFn := luau.NewFunctionExpression(
		luau.NewList[luau.AnyIdentifier](nID), false,
		luau.NewList[luau.Statement](luau.NewReturn(luau.NewBinary(nID, "*", luau.Num(2)))),
	)
	call := luau.NewCall(luau.ID("map"), luau.NewList[luau.Expression](
		luau.NewArray(luau.NewList[luau.Expression](luau.Num(1), luau.Num(2), luau.Num(3))),
		callbackFn,
	))
	doubledDecl := luau.NewVariableDeclaration(luau.ID("doubled"), call)

	// if #doubled > 0 and doubled[1] == 2 then print("ok") else error("bad") end
	cond := luau.NewBinary(
		luau.NewBinary(luau.NewUnary("#", luau.ID("doubled")), ">", luau.Num(0)),
		"and",
		luau.NewBinary(luau.NewComputedIndex(luau.ID("doubled"), luau.Num(1)), "==", luau.Num(2)),
	)
	check := luau.NewIf(cond,
		luau.NewList[luau.Statement](luau.NewCallStatement(
			luau.NewCall(luau.ID("print"), luau.NewList[luau.Expression](luau.Str("ok"))))),
		luau.NewList[luau.Statement](luau.NewCallStatement(
			luau.NewCall(luau.ID("error"), luau.NewList[luau.Expression](luau.Str("bad"))))),
	)

	return luau.NewList[luau.Statement](mapDecl, doubledDecl, check)
}

const goldenWant = `local function map(array, callback)
	local result = {}
	for i, v in ipairs(array) do
		result[i] = callback(v)
	end
	return result
end
local doubled = map({ 1, 2, 3 }, function(n)
	return n * 2
end)
if #doubled > 0 and doubled[1] == 2 then
	print("ok")
else
	error("bad")
end
`

func TestGoldenProgram(t *testing.T) {
	got := renderAST(t, buildGoldenAST())
	if got != goldenWant {
		t.Errorf("golden mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, goldenWant)
		// first-diff helper
		g, w := got, goldenWant
		for i := 0; i < len(g) && i < len(w); i++ {
			if g[i] != w[i] {
				t.Errorf("first diff at byte %d: got %q, want %q (context: %q)", i, g[i], w[i], g[max(0, i-20):min(len(g), i+20)])
				break
			}
		}
	}
	if strings.Contains(got, "\r") {
		t.Error("output must never contain carriage returns")
	}
}

func BenchmarkRenderGolden(b *testing.B) {
	for b.Loop() {
		ast := buildGoldenAST()
		_ = RenderAST(ast)
	}
}
