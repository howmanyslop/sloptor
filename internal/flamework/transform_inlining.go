package flamework

import (
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func inlineMacroIntrinsic(state *TransformState, signature *checker.Signature, arguments []*ast.Node, parameter *ast.Symbol) (*ast.Node, error) {
	index := parameterIndex(signature, parameter)
	if index < 0 || index >= len(arguments) {
		return nil, invalidMacro(signature.Declaration(), "Flamework inline parameter was not supplied")
	}
	typeNode := signature.Declaration().Type()
	if typeNode == nil {
		return nil, invalidMacro(signature.Declaration(), "Flamework inline signature has no return type")
	}
	return state.factory.NewAsExpression(arguments[index], typeNode), nil
}
