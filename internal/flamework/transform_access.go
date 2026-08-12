package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
)

func transformFlameworkAccessExpression(state *TransformState, node *ast.Node) (*ast.Node, error) {
	if !sourceMayNeedFlameworkAccessRewrite(state, ast.GetSourceFileOfNode(node)) {
		return transformFlameworkExpressionChildren(state, node)
	}
	typeOfReceiver := state.checker.GetTypeAtLocation(node.Expression())
	hashType := state.checker.GetTypeOfPropertyOfType(typeOfReceiver, "_flamework_key_obfuscation")
	if hashType == nil || !hashType.IsStringLiteral() {
		return transformFlameworkExpressionChildren(state, node)
	}

	name, known := flameworkAccessName(node)
	if !known {
		if ast.IsElementAccessExpression(node) && ast.IsAsExpression(node.AsElementAccessExpression().ArgumentExpression) {
			return transformFlameworkExpressionChildren(state, node)
		}
		return nil, &MacroError{
			Node:    node,
			Message: "This object has key obfuscation enabled and must be accessed directly.",
			Cause:   ErrDynamicObfuscatedAccess,
		}
	}

	receiver, err := transformFlameworkExpression(state, node.Expression())
	if err != nil {
		return nil, err
	}
	context, ok := hashType.AsLiteralType().Value().(string)
	if !ok {
		return nil, fmt.Errorf("%w: obfuscation context is not text", ErrDynamicObfuscatedAccess)
	}
	obfuscated := name
	if state.project.config.Obfuscation {
		obfuscated, err = state.project.HashString(name, context)
		if err != nil {
			return nil, fmt.Errorf("obfuscate Flamework key %q: %w", name, err)
		}
	}
	literal := state.factory.NewStringLiteral(name, ast.TokenFlagsNone)
	index := state.factory.NewAsExpression(
		state.factory.NewStringLiteral(obfuscated, ast.TokenFlagsNone),
		state.factory.NewLiteralTypeNode(literal),
	)
	return state.factory.NewElementAccessExpression(receiver, node.QuestionDotToken(), index, node.Flags), nil
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
