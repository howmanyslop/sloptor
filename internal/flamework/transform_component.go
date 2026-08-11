package flamework

import "rotor/tsgo/ast"

const (
	componentAttributesMetadata = "intrinsic-component-attributes"
	componentInstanceMetadata   = "intrinsic-component-instance"
	attributeSetterIdentifier   = "SYMBOL_ATTRIBUTE_SETTER"
	attributeSetterModule       = "@flamework/components/out/baseComponent"
)

func isFlameworkAttributesAccess(state *TransformState, node *ast.Node) bool {
	if !ast.IsPropertyAccessExpression(node) && !ast.IsElementAccessExpression(node) {
		return false
	}
	symbol := state.checker.GetSymbolAtLocation(node.Expression())
	if symbol == nil {
		return false
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	return flameworkSymbolRequestsMetadata(state, symbol, componentAttributesMetadata)
}

func flameworkSymbolRequestsMetadata(state *TransformState, symbol *ast.Symbol, requested string) bool {
	if symbol == nil || symbol.ValueDeclaration == nil {
		return false
	}
	return collectNodeMetadata(state, symbol.ValueDeclaration).Requested(requested)
}

func flameworkAccessIndex(state *TransformState, access *ast.Node) *ast.Node {
	if ast.IsPropertyAccessExpression(access) {
		return state.factory.NewStringLiteral(access.Name().Text(), ast.TokenFlagsNone)
	}
	return access.AsElementAccessExpression().ArgumentExpression
}

func flameworkAttributeReceiver(access *ast.Node) (*ast.Node, bool) {
	container := access.Expression()
	if !ast.IsPropertyAccessExpression(container) && !ast.IsElementAccessExpression(container) {
		return nil, false
	}
	return container.Expression(), true
}

func newFlameworkAttributeSetterCall(state *TransformState, access, value *ast.Node, postfix bool) (*ast.Node, error) {
	receiver, ok := flameworkAttributeReceiver(access)
	if !ok {
		return nil, invalidMacro(access, "assignments not supported with direct access")
	}
	setter := state.factory.NewElementAccessExpression(
		receiver,
		nil,
		state.factory.NewIdentifier(attributeSetterIdentifier),
		ast.NodeFlagsNone,
	)
	arguments := []*ast.Node{flameworkAccessIndex(state, access), value}
	if postfix {
		arguments = append(arguments, state.factory.NewToken(ast.KindTrueKeyword))
	}
	return state.factory.NewCallExpression(
		setter,
		nil,
		nil,
		state.factory.NewNodeList(arguments),
		ast.NodeFlagsNone,
	), nil
}

func expressionUsesAttributeSetter(node *ast.Node) bool {
	used := false
	var visit func(*ast.Node) bool
	visit = func(current *ast.Node) bool {
		if ast.IsIdentifier(current) && current.Text() == attributeSetterIdentifier {
			used = true
			return true
		}
		return current.ForEachChild(visit)
	}
	visit(node)
	return used
}

func newAttributeSetterImport(factory *ast.NodeFactory) *ast.Node {
	identifier := factory.NewIdentifier(attributeSetterIdentifier)
	specifier := factory.NewImportSpecifier(false, nil, identifier)
	namedImports := factory.NewNamedImports(factory.NewNodeList([]*ast.Node{specifier}))
	clause := factory.NewImportClause(ast.KindUnknown, nil, namedImports)
	return factory.NewImportDeclaration(
		nil,
		clause,
		factory.NewStringLiteral(attributeSetterModule, ast.TokenFlagsNone),
		nil,
	)
}
