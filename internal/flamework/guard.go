package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

type guardGenerator struct {
	state      *TransformState
	source     *ast.Node
	requires   map[*checker.Type]bool
	dedupIDs   map[*checker.Type]string
	statements []*ast.Node
	active     map[*checker.Type]bool
	tracking   []guardTracking
}

type guardTracking struct {
	node      *ast.Node
	typeValue *checker.Type
}

func buildFlameworkGuard(state *TransformState, typeValue *checker.Type, source *ast.Node) (*ast.Node, error) {
	result, err := buildFlameworkGuardWithDedup(state, typeValue, source, nil)
	return result.Expression, err
}

func buildFlameworkGuardWithDedup(
	state *TransformState,
	typeValue *checker.Type,
	source *ast.Node,
	dedupLimit *int,
) (GuardBuildResult, error) {
	if state == nil || state.checker == nil || state.factory == nil || typeValue == nil || source == nil {
		return GuardBuildResult{}, fmt.Errorf("%w: guard generator requires state, checker, factory, type, and source", ErrInvalidTransformInput)
	}
	generator := &guardGenerator{
		state:      state,
		source:     source,
		requires:   make(map[*checker.Type]bool),
		dedupIDs:   make(map[*checker.Type]string),
		active:     make(map[*checker.Type]bool),
		statements: make([]*ast.Node, 0),
	}
	if dedupLimit != nil {
		limit := max(*dedupLimit, 1)
		generator.requires = guardTypesRequiringDedup(typeValue, state.checker, limit)
	}
	guard, err := generator.build(typeValue)
	if err != nil {
		return GuardBuildResult{}, err
	}
	return GuardBuildResult{Expression: guard, Statements: generator.statements}, nil
}

func buildConfiguredFlameworkGuard(state *TransformState, typeValue *checker.Type, source *ast.Node) (GuardBuildResult, error) {
	var limit *int
	if state != nil && state.project != nil {
		limit = state.project.config.Optimizations.GuardGenerationDedupLimit
	}
	return buildFlameworkGuardWithDedup(state, typeValue, source, limit)
}

func buildFlameworkGuardForMacro(state *TransformState, source *ast.Node, typeValue *checker.Type) (GuardBuildResult, error) {
	return buildConfiguredFlameworkGuard(state, typeValue, source)
}

func (g *guardGenerator) build(typeValue *checker.Type) (*ast.Node, error) {
	if id, ok := g.dedupIDs[typeValue]; ok {
		return g.state.factory.NewIdentifier(id), nil
	}
	if g.active[typeValue] {
		return nil, g.fail(typeValue, "recursive types cannot be represented without a previously emitted guard")
	}
	if declaration := guardDeclaration(typeValue); declaration != nil {
		g.tracking = append(g.tracking, guardTracking{node: declaration, typeValue: typeValue})
		defer func() { g.tracking = g.tracking[:len(g.tracking)-1] }()
	}
	g.active[typeValue] = true
	defer delete(g.active, typeValue)

	guard, err := g.buildInner(typeValue)
	if err != nil {
		return nil, err
	}
	if g.requires[typeValue] {
		name := "dedup"
		if alias := typeValue.Alias(); alias != nil && alias.Symbol() != nil {
			name = alias.Symbol().Name
		}
		name = g.uniqueDedupName(name)
		g.dedupIDs[typeValue] = name
		g.statements = append(g.statements, guardVariable(g.state.factory, name, guard))
		return g.state.factory.NewIdentifier(name), nil
	}
	return guard, nil
}

func (g *guardGenerator) buildInner(typeValue *checker.Type) (*ast.Node, error) {
	typeChecker := g.state.checker
	factory := g.state.factory
	if typeValue.IsUnion() {
		return g.buildUnion(typeValue)
	}
	if typeChecker.GetPropertyOfType(typeValue, "_nominal_Instance") != nil {
		return g.buildInstance(typeValue)
	}
	if typeValue.IsIntersection() {
		return g.buildIntersection(typeValue)
	}
	if typeValue.Flags()&checker.TypeFlagsConditional != 0 {
		alias := typeValue.Alias()
		if alias == nil || alias.Symbol() == nil || len(alias.Symbol().Declarations) != 1 {
			return nil, g.fail(typeValue, "could not find conditional type declaration")
		}
		declaration := alias.Symbol().Declarations[0]
		if !ast.IsTypeAliasDeclaration(declaration) || !ast.IsConditionalTypeNode(declaration.AsTypeAliasDeclaration().Type.AsNode()) {
			return nil, g.fail(typeValue, "could not find conditional type branches")
		}
		conditional := declaration.AsTypeAliasDeclaration().Type.AsNode().AsConditionalTypeNode()
		for _, branch := range []*ast.TypeNode{conditional.TrueType, conditional.FalseType} {
			branchNode := branch.AsNode()
			if !ast.IsTypeReferenceNode(branchNode) {
				continue
			}
			symbol := typeChecker.GetSymbolAtLocation(branchNode.AsTypeReferenceNode().TypeName.AsNode())
			if symbol == nil || symbol.Flags&ast.SymbolFlagsTypeParameter == 0 || len(symbol.Declarations) != 1 {
				continue
			}
			declaration := symbol.Declarations[0]
			if ast.IsTypeParameterDeclaration(declaration) && declaration.AsTypeParameterDeclaration().Constraint == nil {
				return nil, g.fail(typeValue, "could not find constraint of type parameter")
			}
		}
		trueGuard, err := g.build(typeChecker.GetTypeFromTypeNode(conditional.TrueType.AsNode()))
		if err != nil {
			return nil, err
		}
		falseGuard, err := g.build(typeChecker.GetTypeFromTypeNode(conditional.FalseType.AsNode()))
		if err != nil {
			return nil, err
		}
		return guardListCall(factory, "union", []*ast.Node{trueGuard, falseGuard}), nil
	}
	if typeValue.Flags()&checker.TypeFlagsTypeVariable != 0 {
		constraint := typeChecker.GetBaseConstraintOfType(typeValue)
		if constraint == nil {
			return nil, g.fail(typeValue, "could not find constraint of type parameter")
		}
		return g.build(constraint)
	}
	if literal, ok, err := g.literal(typeValue); ok || err != nil {
		return literal, err
	}
	if checker.IsTupleType(typeValue) {
		arguments := guardTypeArguments(typeValue, typeChecker)
		guards, err := g.buildAll(arguments)
		if err != nil {
			return nil, err
		}
		return guardCall(factory, "strictArray", guards...), nil
	}
	if g.isNamedType(typeValue, "Array", "ReadonlyArray") {
		arguments := guardTypeArguments(typeValue, typeChecker)
		elementGuard := guardField(factory, "t", "any")
		if len(arguments) > 0 {
			var err error
			elementGuard, err = g.build(arguments[0])
			if err != nil {
				return nil, err
			}
		}
		return guardCall(factory, "array", elementGuard), nil
	}
	if len(typeChecker.GetSignaturesOfType(typeValue, checker.SignatureKindCall)) > 0 {
		return guardField(factory, "t", "callback"), nil
	}
	switch typeValue {
	case typeChecker.GetVoidType(), typeChecker.GetUndefinedType():
		return guardField(factory, "t", "none"), nil
	case typeChecker.GetAnyType():
		return guardField(factory, "t", "any"), nil
	case typeChecker.GetStringType():
		return guardField(factory, "t", "string"), nil
	case typeChecker.GetNumberType():
		return guardField(factory, "t", "number"), nil
	case typeChecker.GetBooleanType():
		return guardField(factory, "t", "boolean"), nil
	}
	if typeValue.Flags()&checker.TypeFlagsUnknown != 0 {
		return guardCall(factory, "union", guardField(factory, "t", "any"), guardField(factory, "t", "none")), nil
	}
	if typeValue.Flags()&checker.TypeFlagsTemplateLiteral != 0 {
		return nil, g.fail(typeValue, "template literal types are unsupported")
	}
	if typeValue.Symbol() == nil {
		return nil, g.fail(typeValue, "unknown type has no symbol")
	}
	if g.isNamedType(typeValue, "Map", "ReadonlyMap", "WeakMap") {
		return g.buildMap(typeValue)
	}
	if g.isNamedType(typeValue, "Set", "ReadonlySet") {
		return g.buildSet(typeValue)
	}
	if g.isNamedType(typeValue, "Promise") {
		return guardField(factory, "Promise", "is"), nil
	}
	for _, name := range robloxGuardTypes {
		if !g.isNamedType(typeValue, name) {
			continue
		}
		if name == "buffer" {
			return guardCall(factory, "typeof", factory.NewStringLiteral(name, ast.TokenFlagsNone)), nil
		}
		return guardField(factory, "t", name), nil
	}
	if typeValue.IsClass() {
		return nil, g.fail(typeValue, fmt.Sprintf("class %q is unsupported", typeValue.Symbol().Name))
	}
	if typeValue.Flags()&checker.TypeFlagsObject != 0 {
		return g.buildObject(typeValue)
	}
	return nil, g.fail(typeValue, "unknown type was encountered")
}
