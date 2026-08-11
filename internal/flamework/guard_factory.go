package flamework

import (
	"fmt"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/scanner"
)

type guardProperty struct {
	name  string
	guard *ast.Node
}

func guardField(factory *ast.NodeFactory, object, field string) *ast.Node {
	return factory.NewElementAccessExpression(
		factory.NewIdentifier(object),
		nil,
		factory.NewStringLiteral(field, ast.TokenFlagsNone),
		ast.NodeFlagsNone,
	)
}

func guardCall(factory *ast.NodeFactory, field string, arguments ...*ast.Node) *ast.Node {
	return factory.NewCallExpression(guardField(factory, "t", field), nil, nil, factory.NewNodeList(arguments), ast.NodeFlagsNone)
}

func guardListCall(factory *ast.NodeFactory, field string, arguments []*ast.Node) *ast.Node {
	if len(arguments) <= 2 {
		return guardCall(factory, field, arguments...)
	}
	list := factory.NewArrayLiteralExpression(factory.NewNodeList(arguments), false)
	return guardCall(factory, field+"List", list)
}

func guardLiteral(factory *ast.NodeFactory, value any) (*ast.Node, error) {
	switch literal := value.(type) {
	case string:
		return factory.NewStringLiteral(literal, ast.TokenFlagsNone), nil
	case bool:
		if literal {
			return factory.NewToken(ast.KindTrueKeyword), nil
		}
		return factory.NewToken(ast.KindFalseKeyword), nil
	case nil:
		return factory.NewToken(ast.KindNullKeyword), nil
	default:
		return factory.NewNumericLiteral(fmt.Sprint(literal), ast.TokenFlagsNone), nil
	}
}

func guardObject(factory *ast.NodeFactory, properties []guardProperty) *ast.Node {
	nodes := make([]*ast.Node, 0, len(properties))
	for _, property := range properties {
		nodes = append(nodes, factory.NewPropertyAssignment(
			nil,
			factory.NewStringLiteral(property.name, ast.TokenFlagsNone),
			nil,
			nil,
			property.guard,
		))
	}
	return factory.NewObjectLiteralExpression(factory.NewNodeList(nodes), false)
}

func guardVariable(factory *ast.NodeFactory, name string, initializer *ast.Node) *ast.Node {
	declaration := factory.NewVariableDeclaration(factory.NewIdentifier(name), nil, nil, initializer)
	declarations := factory.NewVariableDeclarationList(factory.NewNodeList([]*ast.Node{declaration}), ast.NodeFlagsConst)
	return factory.NewVariableStatement(nil, declarations)
}

func (g *guardGenerator) buildInstance(typeValue *checker.Type) (*ast.Node, error) {
	properties := g.state.checker.GetPropertiesOfType(typeValue)
	nominalNames := make([]string, 0)
	for _, property := range properties {
		if strings.HasPrefix(property.Name, "_nominal_") {
			nominalNames = append(nominalNames, strings.TrimPrefix(property.Name, "_nominal_"))
		}
	}
	specific := typeValue
	specificCount := 0
	for _, name := range nominalNames {
		symbol := g.state.checker.ResolveName(name, nil, ast.SymbolFlagsType, false)
		if symbol == nil {
			continue
		}
		if len(symbol.Declarations) == 0 {
			continue
		}
		candidate := g.state.checker.GetTypeAtLocation(symbol.Declarations[0])
		if candidate == nil {
			continue
		}
		candidateCount := len(g.nominalProperties(candidate))
		if candidateCount <= specificCount {
			continue
		}
		specific = candidate
		specificCount = candidateCount
	}
	if specific == nil || specific.Symbol() == nil {
		return nil, g.fail(typeValue, "could not resolve nominal Instance type")
	}
	for _, nominal := range g.nominalProperties(typeValue) {
		if g.state.checker.GetPropertyOfType(specific, nominal) == nil {
			return nil, g.fail(typeValue, "intersection between nominal types is forbidden")
		}
	}
	base := guardCall(g.state.factory, "instanceIsA", g.state.factory.NewStringLiteral(specific.Symbol().Name, ast.TokenFlagsNone))
	additional := make([]guardProperty, 0)
	for _, property := range properties {
		if g.state.checker.GetPropertyOfType(specific, property.Name) != nil {
			continue
		}
		propertyType := g.state.checker.GetTypeOfPropertyOfType(typeValue, property.Name)
		guard, err := g.build(propertyType)
		if err != nil {
			return nil, err
		}
		additional = append(additional, guardProperty{name: property.Name, guard: guard})
	}
	if len(additional) == 0 {
		return base, nil
	}
	children := guardCall(g.state.factory, "children", guardObject(g.state.factory, additional))
	return guardCall(g.state.factory, "intersection", base, children), nil
}

func (g *guardGenerator) nominalProperties(typeValue *checker.Type) []string {
	properties := make([]string, 0)
	for _, property := range g.state.checker.GetPropertiesOfType(typeValue) {
		if strings.HasPrefix(property.Name, "_nominal_") {
			properties = append(properties, property.Name)
		}
	}
	return properties
}

func (g *guardGenerator) literal(typeValue *checker.Type) (*ast.Node, bool, error) {
	literal, ok, err := g.literalValue(typeValue)
	if !ok || err != nil {
		return nil, ok, err
	}
	return guardCall(g.state.factory, "literal", literal), true, nil
}

func (g *guardGenerator) literalValue(typeValue *checker.Type) (*ast.Node, bool, error) {
	if typeValue.Flags()&checker.TypeFlagsLiteral != 0 {
		literal, err := guardLiteral(g.state.factory, typeValue.AsLiteralType().Value())
		return literal, true, err
	}
	if typeValue.Flags()&checker.TypeFlagsEnum == 0 || typeValue.Symbol() == nil || len(typeValue.Symbol().Declarations) != 1 {
		return nil, false, nil
	}
	declaration := typeValue.Symbol().Declarations[0]
	if !ast.IsEnumDeclaration(declaration) {
		return nil, false, nil
	}
	values := make([]*ast.Node, 0, len(declaration.AsEnumDeclaration().Members.Nodes))
	for _, member := range declaration.AsEnumDeclaration().Members.Nodes {
		value := g.state.checker.GetConstantValue(member)
		if value == nil {
			return nil, false, nil
		}
		literal, err := guardLiteral(g.state.factory, value)
		if err != nil {
			return nil, false, err
		}
		values = append(values, literal)
	}
	return guardListCall(g.state.factory, "literal", values), true, nil
}

func (g *guardGenerator) buildAll(types []*checker.Type) ([]*ast.Node, error) {
	guards := make([]*ast.Node, 0, len(types))
	for _, typeValue := range types {
		guard, err := g.build(typeValue)
		if err != nil {
			return nil, err
		}
		guards = append(guards, guard)
	}
	return guards, nil
}

func (g *guardGenerator) isNamedType(typeValue *checker.Type, names ...string) bool {
	symbol := typeValue.Symbol()
	if symbol == nil {
		return false
	}
	for _, name := range names {
		resolved := g.state.checker.ResolveName(name, nil, ast.SymbolFlagsType, false)
		if symbol == resolved || resolved == nil && symbol.Name == name {
			return true
		}
	}
	return false
}

func (g *guardGenerator) uniqueDedupName(preferred string) string {
	preferred = strings.TrimSpace(preferred)
	if preferred == "" {
		preferred = "dedup"
	}
	return g.state.nextGeneratedName(ast.GetSourceFileOfNode(g.source).FileName(), preferred)
}

func (g *guardGenerator) fail(typeValue *checker.Type, reason string) error {
	source := g.source
	if reason == "intersection between nominal types is forbidden" {
		source = ast.GetSourceFileOfNode(source).AsNode()
	}
	sourceFile := ast.GetSourceFileOfNode(source)
	err := &GuardGenerationError{
		TypeName: g.state.checker.TypeToString(typeValue),
		Reason:   reason,
		FileName: sourceFile.FileName(),
		Start:    scanner.GetTokenPosOfNode(source, sourceFile, false),
		End:      source.End(),
	}
	var previousType *checker.Type
	for _, location := range g.tracking {
		if location.typeValue == previousType {
			continue
		}
		previousType = location.typeValue
		node := location.node
		if name := ast.GetNameOfDeclaration(node); name != nil {
			node = name
		}
		fileName := ast.GetSourceFileOfNode(node).FileName()
		typeName := g.state.checker.TypeToString(location.typeValue)
		err.Path = append(err.Path, fileName+":"+typeName)
		err.RelatedInformation = append(err.RelatedInformation, GuardRelatedInformation{
			TypeName: typeName,
			FileName: fileName,
			Start:    node.Pos(),
			End:      node.End(),
		})
	}
	return err
}
