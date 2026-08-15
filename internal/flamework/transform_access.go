package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
)

func transformFlameworkAccessExpression(state *TransformState, node *ast.Node, runtime MacroRuntime) (expressionTransformResult, error) {
	if !sourceMayNeedFlameworkAccessRewrite(state, ast.GetSourceFileOfNode(node)) {
		return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
	}
	typeOfReceiver := state.checker.GetTypeAtLocation(node.Expression())
	hashType := state.checker.GetTypeOfPropertyOfType(typeOfReceiver, "_flamework_key_obfuscation")
	if hashType == nil || !hashType.IsStringLiteral() {
		return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
	}

	name, known := flameworkAccessName(node)
	if !known {
		if ast.IsElementAccessExpression(node) && ast.IsAsExpression(node.AsElementAccessExpression().ArgumentExpression) {
			return transformFlameworkExpressionChildrenWithRuntime(state, node, runtime)
		}
		return expressionTransformResult{}, &MacroError{
			Node:    node,
			Message: "This object has key obfuscation enabled and must be accessed directly.",
			Cause:   ErrDynamicObfuscatedAccess,
		}
	}

	receiverResult, err := transformFlameworkExpressionWithRuntime(state, node.Expression(), runtime)
	if err != nil {
		return expressionTransformResult{}, err
	}
	context, ok := hashType.AsLiteralType().Value().(string)
	if !ok {
		return expressionTransformResult{}, fmt.Errorf("%w: obfuscation context is not text", ErrDynamicObfuscatedAccess)
	}
	obfuscated := name
	if state.project.config.Obfuscation {
		obfuscated, err = state.project.HashString(name, context)
		if err != nil {
			return expressionTransformResult{}, fmt.Errorf("obfuscate Flamework key %q: %w", name, err)
		}
	}
	literal := state.factory.NewStringLiteral(name, ast.TokenFlagsNone)
	index := state.factory.NewAsExpression(
		state.factory.NewStringLiteral(obfuscated, ast.TokenFlagsNone),
		state.factory.NewLiteralTypeNode(literal),
	)
	return expressionTransformResult{
		expression:    state.factory.NewElementAccessExpression(receiverResult.expression, node.QuestionDotToken(), index, node.Flags),
		prerequisites: receiverResult.prerequisites,
		imports:       receiverResult.imports,
	}, nil
}

func flameworkAccessName(node *ast.Node) (string, bool) {
	if ast.IsPropertyAccessExpression(node) {
		return node.Name().Text(), true
	}
	argument := node.AsElementAccessExpression().ArgumentExpression
	if ast.IsStringLiteral(argument) {
		return argument.Text(), true
	}
	return "", false
}
