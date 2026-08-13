package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// This file ports nodes/transformLogicalOrCoalescingAssignmentExpression.ts.

// transformCoalescingAssignmentExpression ports
// transformCoalescingAssignmentExpression (L8-36): `a ??= b` is a pure
// `if a == nil then a = b end` — NO truthiness machinery, and the RHS prereqs
// evaluate lazily INSIDE the if. No temps for identifier/id-rooted targets
// (transformWritableExpression with readAfterWrite=true only pins complex
// bases/indices).
func transformCoalescingAssignmentExpression(s *State, left, right *ast.Node) luau.WritableExpression {
	writable := transformWritableExpression(s, left, true)
	value, valuePrereqs := s.Capture(func() luau.Expression { return TransformExpression(s, right) })

	ifStatements := luau.NewList[luau.Statement]()
	ifStatements.PushList(valuePrereqs)
	ifStatements.Push(luau.NewAssignment(writable, "=", value))

	s.Prereq(luau.NewIf(
		luau.NewBinary(writable, "==", luau.Nil()),
		ifStatements,
		luau.NewList[luau.Statement](),
	))

	return writable
}

// transformLogicalAndAssignmentExpression ports
// transformLogicalAndAssignmentExpression (L38-76): capture a `_condition`
// temp from the writable, truthiness-test the ORIGINAL writable (NOT the
// temp), assign the RHS to the temp inside the if, then UNCONDITIONALLY write
// the temp back (a redundant self-assignment when the branch was not taken —
// verbatim upstream quirk).
func transformLogicalAndAssignmentExpression(s *State, left, right *ast.Node) luau.WritableExpression {
	writable := transformWritableExpression(s, left, true)
	value, valuePrereqs := s.Capture(func() luau.Expression { return TransformExpression(s, right) })

	conditionID := s.PushToVar(writable, "condition")

	ifStatements := luau.NewList[luau.Statement]()
	ifStatements.PushList(valuePrereqs)
	ifStatements.Push(luau.NewAssignment(conditionID, "=", value))

	s.Prereq(luau.NewIf(
		CreateTruthinessChecks(s, writable, left, s.GetType(left)),
		ifStatements,
		luau.NewList[luau.Statement](),
	))

	s.Prereq(luau.NewAssignment(writable, "=", conditionID))

	return writable
}

// transformLogicalOrAssignmentExpression ports
// transformLogicalOrAssignmentExpression (L78-116): identical to `&&=` except
// the if-condition is `not (<truthiness checks>)`.
func transformLogicalOrAssignmentExpression(s *State, left, right *ast.Node) luau.WritableExpression {
	writable := transformWritableExpression(s, left, true)
	value, valuePrereqs := s.Capture(func() luau.Expression { return TransformExpression(s, right) })

	conditionID := s.PushToVar(writable, "condition")

	ifStatements := luau.NewList[luau.Statement]()
	ifStatements.PushList(valuePrereqs)
	ifStatements.Push(luau.NewAssignment(conditionID, "=", value))

	s.Prereq(luau.NewIf(
		luau.NewUnary("not", CreateTruthinessChecks(s, writable, left, s.GetType(left))),
		ifStatements,
		luau.NewList[luau.Statement](),
	))

	s.Prereq(luau.NewAssignment(writable, "=", conditionID))

	return writable
}

// transformLogicalOrCoalescingAssignmentExpression ports
// transformLogicalOrCoalescingAssignmentExpression (L118-130): operator-token
// dispatch; the returned writable is the expression's value (a re-read of the
// target).
func transformLogicalOrCoalescingAssignmentExpression(s *State, node *ast.Node) luau.WritableExpression {
	expression := node.AsBinaryExpression()
	operator := expression.OperatorToken.Kind
	switch operator {
	case ast.KindQuestionQuestionEqualsToken:
		return transformCoalescingAssignmentExpression(s, expression.Left, expression.Right)
	case ast.KindAmpersandAmpersandEqualsToken:
		return transformLogicalAndAssignmentExpression(s, expression.Left, expression.Right)
	}
	return transformLogicalOrAssignmentExpression(s, expression.Left, expression.Right)
}

// transformLogicalOrCoalescingAssignmentExpressionStatement ports
// transformLogicalOrCoalescingAssignmentExpressionStatement (L132-137):
// statement position discards the writable and keeps only the prereqs.
func transformLogicalOrCoalescingAssignmentExpressionStatement(s *State, node *ast.Node) *luau.List[luau.Statement] {
	return s.CaptureStatements(func() {
		transformLogicalOrCoalescingAssignmentExpression(s, node)
	})
}
