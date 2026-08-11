package flamework

import "rotor/tsgo/ast"

func transformFlameworkDeleteExpression(state *TransformState, node *ast.Node) (*ast.Node, error) {
	access := node.AsDeleteExpression().Expression
	if !isFlameworkAttributesAccess(state, access) {
		return transformFlameworkExpressionChildren(state, node)
	}
	return newFlameworkAttributeSetterCall(state, access, state.factory.NewIdentifier("undefined"), false)
}
