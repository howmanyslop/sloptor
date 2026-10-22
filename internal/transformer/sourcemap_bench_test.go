package transformer

import (
	"fmt"
	"testing"

	"rotor/internal/luau"
)

func BenchmarkRenderSourceMap(b *testing.B) {
	for _, depth := range []int{16, 64, 128} {
		b.Run(fmt.Sprintf("nested_%d", depth), func(b *testing.B) {
			var statement luau.Statement = luau.NewVariableDeclaration(luau.ID("value"), luau.Num(1))
			for range depth {
				statement = luau.NewDo(luau.NewList(statement))
			}
			benchmarkSourceMap(b, luau.NewList(statement))
		})
	}
	for _, kind := range []string{"declarations", "calls", "if_body"} {
		b.Run(kind, func(b *testing.B) {
			statements := luau.NewList[luau.Statement]()
			for range 128 {
				if kind == "calls" {
					statements.Push(luau.NewCallStatement(luau.NewCall(luau.ID("use"), luau.NewList[luau.Expression]())))
				} else {
					statements.Push(luau.NewVariableDeclaration(luau.TempID("value"), luau.Num(1)))
				}
			}
			if kind == "if_body" {
				statements = luau.NewList[luau.Statement](luau.NewIf(luau.Bool(true), statements, nil))
			}
			benchmarkSourceMap(b, statements)
		})
	}
}

func benchmarkSourceMap(b *testing.B, statements *luau.List[luau.Statement]) {
	state, sourceFile, _ := sourceMapLayoutFixture("blocks")
	// Add real anchors so map encoding is included in these measurements.
	for node := statements.Head; node != nil; node = node.Next {
		luau.SetOrigin(node.Value, &luau.Origin{Start: luau.Position{Line: 1}})
	}
	b.ReportAllocs()
	b.ResetTimer()
	for range b.N {
		state.RenderSourceMap(statements, sourceFile)
	}
}
