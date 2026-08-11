package flamework

import "rotor/tsgo/ast"

func transformFlameworkNewExpression(state *TransformState, node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	if state.checker.GetResolvedSignature(node) == nil {
		return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
	}
	result, err := transformFlameworkCall(state, node, runtime)
	if err != nil {
		return expressionTransformResult{}, err
	}
	if macroTransformChanged(node, result.Expression) {
		return expressionTransformResult{
			expression:    result.Expression,
			prerequisites: result.Prerequisites,
			imports:       macroTransformImports(state, node, result),
		}, nil
	}
	children, err := transformFlameworkExpressionChildrenWithRuntime(state, result.Expression, runtime)
	if err != nil {
		return expressionTransformResult{}, err
	}
	children.prerequisites = append(result.Prerequisites, children.prerequisites...)
	children.imports = append(macroTransformImports(state, node, result), children.imports...)
	return children, nil
}
