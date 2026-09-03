package flamework

import "rotor/tsgo/ast"

var flameworkAssignmentOperators = map[ast.Kind]ast.Kind{
	ast.KindEqualsToken:                                  ast.KindEqualsToken,
	ast.KindBarEqualsToken:                               ast.KindBarToken,
	ast.KindPlusEqualsToken:                              ast.KindPlusToken,
	ast.KindMinusEqualsToken:                             ast.KindMinusToken,
	ast.KindCaretEqualsToken:                             ast.KindCaretToken,
	ast.KindSlashEqualsToken:                             ast.KindSlashToken,
	ast.KindBarBarEqualsToken:                            ast.KindBarBarToken,
	ast.KindPercentEqualsToken:                           ast.KindPercentToken,
	ast.KindAsteriskEqualsToken:                          ast.KindAsteriskToken,
	ast.KindAmpersandEqualsToken:                         ast.KindAmpersandToken,
	ast.KindQuestionQuestionEqualsToken:                  ast.KindQuestionQuestionToken,
	ast.KindAsteriskAsteriskEqualsToken:                  ast.KindAsteriskAsteriskToken,
	ast.KindLessThanLessThanEqualsToken:                  ast.KindLessThanLessThanToken,
	ast.KindAmpersandAmpersandEqualsToken:                ast.KindAmpersandAmpersandToken,
	ast.KindGreaterThanGreaterThanEqualsToken:            ast.KindGreaterThanGreaterThanToken,
	ast.KindGreaterThanGreaterThanGreaterThanEqualsToken: ast.KindGreaterThanGreaterThanGreaterThanToken,
}

func transformFlameworkBinaryExpression(state *TransformState, node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	binary := node.AsBinaryExpression()
	operator, mutates := flameworkAssignmentOperators[binary.OperatorToken.Kind]
	if !mutates || !isFlameworkAttributesAccess(state, binary.Left) {
		return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
	}

	right, err := transformFlameworkExpressionWithRuntime(state, binary.Right, runtime)
	if err != nil {
		return expressionTransformResult{}, err
	}
	value := right.expression
	if operator != ast.KindEqualsToken {
		value = state.factory.NewBinaryExpression(
			nil,
			binary.Left,
			nil,
			state.factory.NewToken(operator),
			right.expression,
		)
	}
	setter, err := newFlameworkAttributeSetterCall(state, binary.Left, value, false)
	if err != nil {
		return expressionTransformResult{}, err
	}
	// The macro that produced `value` may have requested imports and hoisted
	// statements; the setter call replaces the expression, not those.
	return expressionTransformResult{expression: setter, prerequisites: right.prerequisites, imports: right.imports}, nil
}
