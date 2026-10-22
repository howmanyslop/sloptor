package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports expressions/transformUnaryExpression.ts. The ++/-- forms
// here are the EXPRESSION positions (temps capture the old/new value); the
// statement specialization without temps lives in statements.go
// (transformUnaryExpressionStatement).

// transformPostfixUnaryExpression ports transformPostfixUnaryExpression
// (L13-40): `i++` in expression position captures the ORIGINAL value:
//
//	local _original = i
//	i += 1
//	-> _original
func transformPostfixUnaryExpression(s *State, node *ast.Node) luau.Expression {
	expression := node.AsPostfixUnaryExpression()

	validateNotAnyType(s, expression.Operand)

	writable := transformWritableExpression(s, expression.Operand, true)
	origValue := luau.TempID("original")

	s.Prereq(luau.NewVariableDeclaration(origValue, writable))

	var operator luau.AssignmentOperator
	switch expression.Operator {
	case ast.KindPlusPlusToken:
		operator = "+="
	case ast.KindMinusMinusToken:
		operator = "-="
	default:
		panic("transformer: transformPostfixUnaryExpression unknown operator: " + kindName(expression.Operator)) // upstream assertNever
	}
	assignment := luau.NewAssignment(writable, operator, luau.Num(1))
	s.setUpdateOperatorSourcePosition(assignment, node)
	s.Prereq(assignment)

	return origValue
}

// transformPrefixUnaryExpression ports transformPrefixUnaryExpression
// (L42-71).
func transformPrefixUnaryExpression(s *State, node *ast.Node) luau.Expression {
	expression := node.AsPrefixUnaryExpression()

	validateNotAnyType(s, expression.Operand)

	switch expression.Operator {
	case ast.KindPlusPlusToken, ast.KindMinusMinusToken:
		// `++i` in expression position: prereq the increment, return the
		// writable itself (the NEW value).
		writable := transformWritableExpression(s, expression.Operand, true)
		operator := luau.AssignmentOperator("+=")
		if expression.Operator == ast.KindMinusMinusToken {
			operator = "-="
		}
		assignment := luau.NewAssignment(writable, operator, luau.Num(1))
		s.setUpdateOperatorSourcePosition(assignment, node)
		s.Prereq(assignment)
		return writable
	case ast.KindPlusToken:
		s.Diags.Add(DiagNoUnaryPlus(node))
		return TransformExpression(s, expression.Operand)
	case ast.KindMinusToken:
		if !IsDefinitelyType(s, s.GetType(expression.Operand), IsNumberType) {
			s.Diags.Add(DiagNoNonNumberUnaryMinus(node))
		}
		return luau.NewUnary("-", TransformExpression(s, expression.Operand))
	case ast.KindExclamationToken:
		checks := CreateTruthinessChecks(s, TransformExpression(s, expression.Operand), expression.Operand, s.GetType(expression.Operand))
		return luau.NewUnary("not", checks)
	case ast.KindTildeToken:
		return luau.NewCall(luau.GlobalProperty("bit32", "bnot"), luau.NewList[luau.Expression](TransformExpression(s, expression.Operand)))
	}
	panic("transformer: transformPrefixUnaryExpression unknown operator: " + kindName(expression.Operator)) // upstream assertNever
}
