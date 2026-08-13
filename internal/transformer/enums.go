package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/jsnum"
)

// This file ports statements/transformEnumDeclaration.ts and
// util/hasMultipleDefinitions.ts.

// hasMultipleDefinitions ports util/hasMultipleDefinitions.ts (L3-14): counts
// symbol declarations passing filter; true when more than one. Shared by the
// enum and namespace merging bans.
func hasMultipleDefinitions(symbol *ast.Symbol, filter func(declaration *ast.Node) bool) bool {
	amtValueDefinitions := 0
	for _, declaration := range symbol.Declarations {
		if filter(declaration) {
			amtValueDefinitions++
			if amtValueDefinitions > 1 {
				return true
			}
		}
	}
	return false
}

// needsInverseEntry ports transformEnumDeclaration.ts needsInverseEntry
// (L14-16): `typeof getConstantValue(member) !== "string"` — number-valued
// members get an inverse entry, and so do computed members the evaluator
// could not fold (their constant value is undefined, which is "not a string").
func needsInverseEntry(s *State, member *ast.Node) bool {
	_, isString := s.Checker.GetConstantValue(member).(string)
	return !isString
}

// transformEnumDeclaration ports statements/transformEnumDeclaration.ts
// (L18-128). String-only enums collapse to a plain map; everything else gets
// the `local E` header + do-block with the `_inverse` table behind a
// `setmetatable({}, { __index = _inverse })`, member and inverse assignments
// interleaved per member. Const enums emit nothing (member accesses are
// inlined by getConstantValueLiteral); `declare enum` never reaches here (the
// dispatch-level declare skip catches it).
func transformEnumDeclaration(s *State, node *ast.Node) *luau.List[luau.Statement] {
	// const enum: no emit unless preserveConstEnums
	if ast.HasSyntacticModifier(node, ast.ModifierFlagsConst) &&
		(s.Program == nil || !s.Program.Options().PreserveConstEnums.IsTrue()) {
		return luau.NewList[luau.Statement]()
	}

	symbol := s.Checker.GetSymbolAtLocation(node.Name())
	if symbol != nil && hasMultipleDefinitions(symbol, func(declaration *ast.Node) bool {
		return ast.IsEnumDeclaration(declaration) &&
			!ast.HasSyntacticModifier(declaration, ast.ModifierFlagsConst)
	}) {
		AddDiagnosticWithCache(s.Diags, symbol, DiagNoEnumMerging(node),
			s.Multi.IsReportedByMultipleDefinitionsCache)
		return luau.NewList[luau.Statement]()
	}

	ValidateIdentifier(s, node.Name())

	left := TransformIdentifierDefined(s, node.Name())
	isHoisted := symbol != nil && s.IsHoisted[symbol]

	members := node.AsEnumDeclaration().Members.Nodes

	// FAST PATH: all members string-valued -> plain map, no inverse table
	allStrings := true
	for _, member := range members {
		if needsInverseEntry(s, member) {
			allStrings = false
			break
		}
	}
	if allStrings {
		fields := luau.NewList[*luau.MapField]()
		for _, member := range members {
			fields.Push(luau.NewMapField(
				s.PushToVarIfComplex(transformPropertyName(s, member.Name()), ""),
				luau.Str(s.Checker.GetConstantValue(member).(string)),
			))
		}
		right := luau.NewMap(fields)
		if isHoisted {
			return luau.NewList[luau.Statement](luau.NewAssignment(left, "=", right))
		}
		return luau.NewList[luau.Statement](luau.NewVariableDeclaration(left, right))
	}

	// GENERAL PATH: setmetatable + inverse, inside a do-block
	statements := s.CaptureStatements(func() {
		inverseID := s.PushToVar(luau.NewMap(luau.NewList[*luau.MapField]()), "inverse")
		s.Prereq(luau.NewAssignment(left, "=", luau.NewCall(
			luau.GlobalID("setmetatable"),
			luau.NewList[luau.Expression](
				luau.NewMap(luau.NewList[*luau.MapField]()),
				luau.NewMap(luau.NewList(luau.NewMapField(luau.Str("__index"), inverseID))),
			),
		)))

		for _, member := range members {
			name := transformPropertyName(s, member.Name())
			mutateCheckNode := member.Name()
			if ast.IsComputedPropertyName(mutateCheckNode) {
				mutateCheckNode = mutateCheckNode.AsComputedPropertyName().Expression
			}
			index := name
			if expressionMightMutate(s, name, mutateCheckNode) {
				// note: we don't use pushToVarIfComplex here
				// because identifier also needs to be pushed
				// since the value calculation might reassign the variable
				index = s.PushToVar(name, "")
			}

			var valueExp luau.Expression
			switch value := s.Checker.GetConstantValue(member).(type) {
			case string:
				valueExp = luau.Str(value)
			case jsnum.Number:
				valueExp = luau.Num(float64(value))
			case float64:
				valueExp = luau.Num(value)
			default:
				// constantValue is always number without initializer, so assert is safe
				initializer := member.AsEnumMember().Initializer
				if initializer == nil {
					panic("transformer: transformEnumDeclaration: computed member without initializer") // upstream assert
				}
				valueExp = s.PushToVarIfComplex(TransformExpression(s, initializer), "value")
			}

			s.Prereq(luau.NewAssignment(
				luau.NewComputedIndex(left, index),
				"=",
				valueExp,
			))

			if needsInverseEntry(s, member) {
				s.Prereq(luau.NewAssignment(
					luau.NewComputedIndex(inverseID, valueExp),
					"=",
					index,
				))
			}
		}
	})

	list := luau.NewList[luau.Statement](luau.NewDo(statements))
	if !isHoisted {
		list.Unshift(luau.NewVariableDeclaration(left, nil))
	}
	return list
}
