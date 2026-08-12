package flamework

import "rotor/tsgo/ast"

func transformFlameworkUnaryExpression(state *TransformState, node *ast.Node) (*ast.Node, error) {
	operand, operator, postfix := flameworkUnaryParts(node)
	binaryOperator, mutates := flameworkUnaryOperator(operator)
	if !mutates || !isFlameworkAttributesAccess(state, operand) {
		return transformFlameworkExpressionChildren(state, node)
	}
	value := state.factory.NewBinaryExpression(
		nil,
		operand,
		nil,
		state.factory.NewToken(binaryOperator),
		state.factory.NewNumericLiteral("1", ast.TokenFlagsNone),
	)
	return newFlameworkAttributeSetterCall(state, operand, value, postfix)
}

func flameworkUnaryParts(node *ast.Node) (operand *ast.Node, operator ast.Kind, postfix bool) {
	if ast.IsPostfixUnaryExpression(node) {
		unary := node.AsPostfixUnaryExpression()
		return unary.Operand, unary.Operator, true
	}
	unary := node.AsPrefixUnaryExpression()
	return unary.Operand, unary.Operator, false
}

func flameworkUnaryOperator(operator ast.Kind) (ast.Kind, bool) {
	switch operator {
	case ast.KindPlusPlusToken:
		return ast.KindPlusToken, true
	case ast.KindMinusMinusToken:
		return ast.KindMinusToken, true
	default:
		return ast.KindUnknown, false
	}
}
