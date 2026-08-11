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

func transformFlameworkBinaryExpression(state *TransformState, node *ast.Node) (*ast.Node, error) {
	binary := node.AsBinaryExpression()
	operator, mutates := flameworkAssignmentOperators[binary.OperatorToken.Kind]
	if !mutates || !isFlameworkAttributesAccess(state, binary.Left) {
		return transformFlameworkExpressionChildren(state, node)
	}

	right, err := transformFlameworkExpression(state, binary.Right)
	if err != nil {
		return nil, err
	}
	value := right
	if operator != ast.KindEqualsToken {
		value = state.factory.NewBinaryExpression(
			nil,
			binary.Left,
			nil,
			state.factory.NewToken(operator),
			right,
		)
	}
	return newFlameworkAttributeSetterCall(state, binary.Left, value, false)
}
