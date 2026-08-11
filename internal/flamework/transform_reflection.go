package flamework

import "rotor/tsgo/ast"

func analyzeConstructorParameters(state *TransformState, plan FilePlan, node *ast.Node) ([]string, error) {
	if !metadataForClass(state, node).Requested("flamework:parameters") {
		return nil, nil
	}
	for _, member := range node.AsClassDeclaration().Members.Nodes {
		if !ast.IsConstructorDeclaration(member) {
			continue
		}
		return reflectedParameterIDs(state, member.Parameters())
	}
	return nil, nil
}

func analyzeImplementedTypes(state *TransformState, plan FilePlan, node *ast.Node) ([]string, error) {
	if !metadataForClass(state, node).Requested("flamework:implements") {
		return nil, nil
	}
	var identifiers []string
	clauses := node.AsClassDeclaration().HeritageClauses
	if clauses == nil {
		return identifiers, nil
	}
	for _, clauseNode := range clauses.Nodes {
		clause := clauseNode.AsHeritageClause()
		if clause.Token != ast.KindImplementsKeyword {
			continue
		}
		for _, implemented := range clause.Types.Nodes {
			identifier, err := nodeUID(state, implemented)
			if err != nil {
				return nil, err
			}
			identifiers = append(identifiers, identifier)
		}
	}
	return identifiers, nil
}

type metadataCall struct {
	className string
	key       string
	value     *ast.Node
}

func newMetadataStatement(factory *ast.NodeFactory, call metadataCall) *ast.Node {
	return newReflectCall(factory, "defineMetadata", []*ast.Node{
		factory.NewIdentifier(call.className), factory.NewStringLiteral(call.key, ast.TokenFlagsNone), call.value,
	})
}

func newReflectCall(factory *ast.NodeFactory, method string, arguments []*ast.Node) *ast.Node {
	access := factory.NewElementAccessExpression(
		factory.NewIdentifier("Reflect"), nil, factory.NewStringLiteral(method, ast.TokenFlagsNone), ast.NodeFlagsNone,
	)
	call := factory.NewCallExpression(access, nil, nil, factory.NewNodeList(arguments), ast.NodeFlagsNone)
	return factory.NewExpressionStatement(call)
}

func stringArray(factory *ast.NodeFactory, values []string) *ast.Node {
	elements := make([]*ast.Node, len(values))
	for index, value := range values {
		elements[index] = factory.NewStringLiteral(value, ast.TokenFlagsNone)
	}
	return expressionArray(factory, elements)
}

func expressionArray(factory *ast.NodeFactory, values []*ast.Node) *ast.Node {
	return factory.NewArrayLiteralExpression(factory.NewNodeList(values), false)
}
