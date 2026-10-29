package transformer

import (
	"path/filepath"

	"rotor/internal/luau"
	"rotor/internal/rojo"
	"rotor/tsgo/ast"
)

// parentField ports Shared/constants.ts PARENT_FIELD: the instance property a
// RbxPathParent segment renders as.
const parentField = "Parent"

// createGetService ports util/createGetService.ts:
// `game:GetService("<serviceName>")`.
func createGetService(serviceName string) luau.IndexableExpression {
	return luau.NewMethodCall("GetService", luau.GlobalID("game"),
		luau.NewList[luau.Expression](luau.Str(serviceName)))
}

// propertyAccessExpressionChain ports util/expressionChain.ts
// propertyAccessExpressionChain: `["a","b"]` over `exp` -> `exp.a.b`.
func propertyAccessExpressionChain(expression luau.Expression, names []string) luau.IndexableExpression {
	acc := convertToIndexableExpression(expression)
	for _, name := range names {
		acc = luau.NewPropertyAccess(acc, name)
	}
	return acc
}

// CreateRuntimeLibImport ports TransformState.createRuntimeLibImport
// (TransformState.ts:202-265): the `local TS = ...` declaration emitted into
// the file header when UsesRuntimeLib is set.
//
//   - Game:    local TS = require(game:GetService("<p0>"):WaitForChild("<p1>"):...)
//     — the ONE place WaitForChild is emitted literally (imports use plain
//     property strings).
//   - Model:   local TS = require(script.<chain>) where chain is
//     RojoResolver.relative(fileRbxPath, runtimeLibRbxPath) as PROPERTY
//     accesses, RbxPathParent -> .Parent. A file the Rojo config doesn't
//     cover raises noRojoData and falls back to `local TS` (upstream
//     luau.none()).
//   - Package: local TS = _G[script] (runtimeLibRbxPath undefined upstream;
//     "we pass RuntimeLib access to packages via `_G[script] = TS`").
//
// The branch is keyed on RuntimeLibRbxPath presence, exactly like upstream's
// `if (this.runtimeLibRbxPath)` — a nil Rojo context (only reachable from
// states built outside CompileProject/CompileFile) therefore also lands on
// the Package form.
func (state *State) CreateRuntimeLibImport(node *ast.SourceFile) luau.Statement {
	tsID := luau.GlobalID("TS")

	if state.Rojo != nil && len(state.Rojo.RuntimeLibRbxPath) > 0 {
		if state.ProjectType == ProjectTypeGame {
			// Chain of WaitForChild method calls over the rbxPath tail,
			// nested inside require (TransformState.ts:205-228).
			expression := createGetService(state.Rojo.RuntimeLibRbxPath[0])
			for _, part := range state.Rojo.RuntimeLibRbxPath[1:] {
				expression = luau.NewMethodCall("WaitForChild", expression,
					luau.NewList[luau.Expression](luau.Str(part)))
			}
			require := luau.NewCall(luau.GlobalID("require"), luau.NewList[luau.Expression](expression))
			return luau.NewVariableDeclaration(tsID, require)
		}

		// Model: per-file relative property chain (TransformState.ts:229-253).
		sourceOutPath := state.Rojo.PathTranslator.GetOutputPath(node.FileName())
		rbxPath, ok := state.Rojo.Resolver.GetRbxPathFromFilePath(sourceOutPath)
		if !ok {
			relPath, err := filepath.Rel(state.Rojo.ProjectPath, sourceOutPath)
			if err != nil {
				relPath = sourceOutPath
			}
			state.Diags.Add(DiagNoRojoData(node.AsNode(), relPath, false))
			return luau.NewVariableDeclaration(tsID, nil)
		}

		relative := rojo.Relative(rbxPath, state.Rojo.RuntimeLibRbxPath)
		names := make([]string, len(relative))
		for i, segment := range relative {
			if segment.Parent {
				names[i] = parentField
			} else {
				names[i] = segment.Name
			}
		}
		require := luau.NewCall(luau.GlobalID("require"), luau.NewList[luau.Expression](
			propertyAccessExpressionChain(luau.GlobalID("script"), names),
		))
		return luau.NewVariableDeclaration(tsID, require)
	}

	// Package (TransformState.ts:254-264): local TS = _G[script]
	return luau.NewVariableDeclaration(tsID,
		luau.NewComputedIndex(luau.GlobalID("_G"), luau.GlobalID("script")))
}
