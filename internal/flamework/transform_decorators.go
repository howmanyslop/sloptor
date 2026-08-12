package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
)

type decoratorTarget struct {
	className    string
	propertyName string
	plan         ClassPlan
	static       bool
	member       bool
}

func analyzeClassDecorators(state *TransformState, plan ClassPlan, node *ast.Node) ([]classDecoratorAnalysis, Metadata, error) {
	decorators, err := analyzeNodeDecorators(state, plan, node)
	if err != nil {
		return nil, Metadata{}, err
	}
	metadata := metadataForNode(state, node)
	for _, decorator := range decorators {
		metadata = mergeMetadata(metadata, metadataForDecorator(state, decorator.expression))
	}
	return decorators, metadata, nil
}

func analyzeNodeDecorators(state *TransformState, plan ClassPlan, node *ast.Node) ([]classDecoratorAnalysis, error) {
	decorators := make([]classDecoratorAnalysis, 0)
	modifiers := node.Modifiers()
	if modifiers == nil {
		return decorators, nil
	}
	for _, modifier := range modifiers.Nodes {
		if !ast.IsDecorator(modifier) || !isFlameworkDecorator(state, modifier) {
			continue
		}
		expression := modifier.AsDecorator().Expression
		identifier := expression
		var arguments []*ast.Node
		if ast.IsCallExpression(expression) {
			call := expression.AsCallExpression()
			identifier = call.Expression
			arguments = append(arguments, call.Arguments.Nodes...)
		}
		name, ok := decoratorReferenceName(identifier)
		if !ok {
			return nil, fmt.Errorf("%w: decorator expression on %s", ErrInvalidDecorator, plan.InternalID)
		}
		decoratorPlan, found := plannedDecoratorByName(plan, name)
		if !found {
			decoratorPlan = DecoratorPlan{Name: name, InternalID: localInternalID(plan.InternalID, name)}
		}
		decorators = append(decorators, classDecoratorAnalysis{
			name: name, internalID: decoratorPlan.InternalID, expression: identifier, arguments: arguments,
		})
	}
	for index := len(decorators) - 1; index >= 0; index-- {
		decorator := &decorators[index]
		transformedArguments, prerequisites, imports, transformErr := transformDecoratorConfig(
			state, node, decorator.expression, decorator.arguments,
		)
		if transformErr != nil {
			return nil, transformErr
		}
		decorator.arguments = transformedArguments
		decorator.prerequisites = prerequisites
		decorator.imports = imports
	}
	return decorators, nil
}

func decoratorReferenceName(node *ast.Node) (string, bool) {
	switch {
	case ast.IsIdentifier(node):
		return node.Text(), true
	case ast.IsPropertyAccessExpression(node):
		return node.Name().Text(), true
	case ast.IsElementAccessExpression(node) && ast.IsStringLiteral(node.AsElementAccessExpression().ArgumentExpression):
		return node.AsElementAccessExpression().ArgumentExpression.Text(), true
	default:
		return "", false
	}
}

func transformDecoratorConfig(
	state *TransformState,
	declaration *ast.Node,
	identifier *ast.Node,
	arguments []*ast.Node,
) ([]*ast.Node, []*ast.Node, []MacroImport, error) {
	if metadataForDecorator(state, decoratorSymbolNode(identifier)).Requested("intrinsic-component-decorator") {
		if !ast.IsClassDeclaration(declaration) {
			placement := declaration.Kind.String()
			if declaration.Name() != nil {
				placement = declaration.Name().Text()
			}
			return nil, nil, nil, fmt.Errorf("%w: component decorator requires a class declaration, got %s", ErrInvalidDecorator, placement)
		}
		if len(arguments) > 1 || (len(arguments) == 1 && !ast.IsObjectLiteralExpression(arguments[0])) {
			return nil, nil, nil, fmt.Errorf("%w: component decorator config must be an object literal", ErrInvalidDecorator)
		}
		base := state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(nil), true)
		if len(arguments) == 1 {
			base = arguments[0]
		}
		object := base.AsObjectLiteralExpression()
		transformedProperties := make([]*ast.Node, len(object.Properties.Nodes))
		prerequisites := make([]*ast.Node, 0)
		imports := make([]MacroImport, 0)
		for index, property := range object.Properties.Nodes {
			updated, err := transformFlameworkExpressionChildrenWithRuntime(state, property, defaultFlameworkMacroRuntime())
			if err != nil {
				return nil, nil, nil, fmt.Errorf("transform component decorator property %d: %w", index, err)
			}
			transformedProperties[index] = updated.expression
			prerequisites = append(prerequisites, updated.prerequisites...)
			imports = append(imports, updated.imports...)
		}
		properties, err := updateFlameworkComponentConfig(state, declaration, transformedProperties)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("transform component decorator config: %w", err)
		}
		transformed := []*ast.Node{state.factory.UpdateObjectLiteralExpression(object, state.factory.NewNodeList(properties), true)}
		return transformed, prerequisites, decoratorRuntimeImports(transformed, prerequisites, imports), nil
	}
	transformed := make([]*ast.Node, len(arguments))
	prerequisites := make([]*ast.Node, 0)
	imports := make([]MacroImport, 0)
	for index, argument := range arguments {
		updated, err := transformDecoratorArgument(state, argument)
		if err != nil {
			return nil, nil, nil, fmt.Errorf("transform decorator argument %d: %w", index, err)
		}
		transformed[index] = updated.expression
		prerequisites = append(prerequisites, updated.prerequisites...)
		imports = append(imports, updated.imports...)
	}
	return transformed, prerequisites, decoratorRuntimeImports(transformed, prerequisites, imports), nil
}

func transformDecoratorArgument(state *TransformState, argument *ast.Node) (expressionTransformResult, error) {
	if decoratorArgumentAlreadyTransformed(argument) {
		return expressionTransformResult{expression: argument}, nil
	}
	runtime := defaultFlameworkMacroRuntime()
	if ast.IsCallExpression(argument) || ast.IsNewExpression(argument) {
		result, err := transformFlameworkCall(state, argument, runtime)
		if err == nil && (result.Expression != argument || len(result.Prerequisites) > 0 || len(result.Imports) > 0) {
			return expressionTransformResult{result.Expression, result.Prerequisites, result.Imports}, nil
		}
		if err != nil && state.checker.GetResolvedSignature(argument) != nil {
			return expressionTransformResult{}, err
		}
	}
	return transformFlameworkExpressionWithRuntime(state, argument, runtime)
}

func decoratorArgumentAlreadyTransformed(argument *ast.Node) bool {
	for child := range argument.IterChildren() {
		if child.Pos() < 0 || decoratorArgumentAlreadyTransformed(child) {
			return true
		}
	}
	return false
}

func decoratorSymbolNode(identifier *ast.Node) *ast.Node {
	if ast.IsPropertyAccessExpression(identifier) {
		return identifier.Name()
	}
	if ast.IsElementAccessExpression(identifier) {
		return identifier.AsElementAccessExpression().ArgumentExpression
	}
	return identifier
}

func isFlameworkDecorator(state *TransformState, decorator *ast.Node) bool {
	typeAtLocation := state.checker.GetTypeAtLocation(decorator.AsDecorator().Expression)
	return state.checker.GetPropertyOfType(typeAtLocation, "_flamework_Decorator") != nil
}

func plannedDecoratorByName(plan ClassPlan, name string) (DecoratorPlan, bool) {
	for _, decorator := range plan.Decorators {
		if decorator.Name == name {
			return decorator, true
		}
	}
	return DecoratorPlan{}, false
}

func stripFlameworkDecorators(state *TransformState, modifiers *ast.ModifierList) *ast.ModifierList {
	if modifiers == nil {
		return nil
	}
	filtered := make([]*ast.Node, 0, len(modifiers.Nodes))
	for _, modifier := range modifiers.Nodes {
		if ast.IsDecorator(modifier) && isFlameworkDecorator(state, modifier) {
			continue
		}
		filtered = append(filtered, modifier)
	}
	return state.factory.NewModifierList(filtered)
}

func newDecoratorStatement(state *TransformState, className string, decorator classDecoratorAnalysis) (*ast.Node, error) {
	return newDecoratorStatementForTarget(state, decoratorTarget{className: className}, decorator)
}

func newDecoratorStatementForTarget(
	state *TransformState,
	target decoratorTarget,
	decorator classDecoratorAnalysis,
) (*ast.Node, error) {
	identifier, err := identifierForTransform(state, identifierTransformInput{
		internalID: decorator.internalID, name: decorator.name, node: decorator.expression,
	})
	if err != nil {
		return nil, err
	}
	arguments := state.factory.NewArrayLiteralExpression(state.factory.NewNodeList(decorator.arguments), false)
	callArguments := []*ast.Node{
		state.factory.NewIdentifier(target.className),
		state.factory.NewStringLiteral(identifier, ast.TokenFlagsNone),
		decorator.expression,
		arguments,
	}
	if target.member {
		callArguments = append(callArguments, state.factory.NewStringLiteral(target.propertyName, ast.TokenFlagsNone))
		keyword := ast.KindFalseKeyword
		if target.static {
			keyword = ast.KindTrueKeyword
		}
		callArguments = append(callArguments, state.factory.NewKeywordExpression(keyword))
	}
	return newReflectCallWithIdentifier(state.factory, reflectIdentifierForNode(decorator.expression), "decorate", callArguments), nil
}

func newReflectCallWithIdentifier(factory *ast.NodeFactory, identifier, method string, arguments []*ast.Node) *ast.Node {
	access := factory.NewElementAccessExpression(
		factory.NewIdentifier(identifier), nil, factory.NewStringLiteral(method, ast.TokenFlagsNone), ast.NodeFlagsNone,
	)
	call := factory.NewCallExpression(access, nil, nil, factory.NewNodeList(arguments), ast.NodeFlagsNone)
	return factory.NewExpressionStatement(call)
}
