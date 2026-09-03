package flamework

import "rotor/tsgo/ast"

func transformFlameworkDeleteExpression(state *TransformState, node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	access := node.AsDeleteExpression().Expression
	if !isFlameworkAttributesAccess(state, access) {
		return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
	}
	setter, err := newFlameworkAttributeSetterCall(state, access, state.factory.NewIdentifier("undefined"), false)
	if err != nil {
		return expressionTransformResult{}, err
	}
	return expressionTransformResult{expression: setter}, nil
}
