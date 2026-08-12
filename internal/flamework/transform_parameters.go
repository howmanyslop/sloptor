package flamework

import (
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func validateParameterConstIntrinsic(node *ast.Node, signature *checker.Signature, parameters []*ast.Symbol) error {
	arguments := node.Arguments()
	for _, parameter := range parameters {
		index := parameterIndex(signature, parameter)
		if index < 0 || index >= len(arguments) {
			continue
		}
		argument := arguments[index]
		if ast.IsLiteralExpression(argument) || argument.Kind == ast.KindTrueKeyword || argument.Kind == ast.KindFalseKeyword || argument.Kind == ast.KindNullKeyword {
			continue
		}
		var elements []*ast.Node
		switch argument.Kind {
		case ast.KindObjectLiteralExpression:
			elements = argument.AsObjectLiteralExpression().Properties.Nodes
		case ast.KindArrayLiteralExpression:
			elements = argument.AsArrayLiteralExpression().Elements.Nodes
		default:
			return invalidConstParameterMacro(argument, parameter, "Flamework expected this argument to be a literal expression.")
		}
		for _, element := range elements {
			if ast.IsSpreadElement(element) || ast.IsSpreadAssignment(element) {
				return invalidConstParameterMacro(element, parameter, "Flamework does not support spread expressions in this location.")
			}
		}
	}
	return nil
}

func invalidConstParameterMacro(node *ast.Node, parameter *ast.Symbol, message string) error {
	relatedNode := node
	if parameter != nil && parameter.ValueDeclaration != nil {
		relatedNode = parameter.ValueDeclaration
	}
	return invalidMacroWithRelated(node, message, MacroRelatedInformation{
		Node:    relatedNode,
		Message: "Required because this parameter must be known at compile-time.",
	})
}

func parameterIndex(signature *checker.Signature, target *ast.Symbol) int {
	for index, parameter := range signature.Parameters() {
		if parameter == target || parameter.ValueDeclaration != nil && target.ValueDeclaration != nil && parameter.ValueDeclaration.Symbol() == target.ValueDeclaration.Symbol() {
			return index
		}
	}
	return -1
}

func macroParameterCount(state *TransformState, signature *checker.Signature) int {
	count := len(signature.Parameters())
	if !signature.HasRestParameter() || count == 0 {
		return count
	}
	restType := state.checker.GetTypeOfSymbol(signature.Parameters()[count-1])
	if !checker.IsTupleType(restType) {
		return count
	}
	tuple := restType.TargetTupleType()
	count += tuple.FixedLength() - 1
	for _, flags := range tuple.ElementFlags() {
		if flags&checker.ElementFlagsRest != 0 {
			count++
			break
		}
	}
	return count
}
