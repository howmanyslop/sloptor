package flamework

import (
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

type memberReflectionInput struct {
	plan      FilePlan
	className string
	member    *ast.Node
}

type reflectionField struct {
	name  string
	value *ast.Node
}

type reflectionTarget struct {
	className    string
	propertyName string
	member       *ast.Node
	metadata     Metadata
}

type reflectionEmission struct {
	className    string
	propertyName string
	fields       []reflectionField
}

func constructorReflectionStatements(state *TransformState, className string, constructor *ast.Node) ([]*ast.Node, error) {
	if constructor == nil || !ast.IsConstructorDeclaration(constructor) {
		return nil, nil
	}
	return methodReflectionStatements(state, reflectionTarget{
		className: className,
		member:    constructor,
		metadata:  metadataForClass(state, constructor.Parent),
	})
}

func memberReflectionStatements(state *TransformState, input memberReflectionInput) ([]*ast.Node, error) {
	member := input.member
	if member.Name() == nil || (!ast.IsMethodDeclaration(member) && !ast.IsPropertyDeclaration(member)) {
		return nil, nil
	}
	propertyName := ast.GetPropertyNameForPropertyNameNode(member.Name())
	metadata := metadataForMember(state, member)
	target := reflectionTarget{input.className, propertyName, member, metadata}
	if ast.IsPropertyDeclaration(member) {
		return fieldReflectionStatements(state, target)
	}
	return methodReflectionStatements(state, target)
}

func fieldReflectionStatements(state *TransformState, target reflectionTarget) ([]*ast.Node, error) {
	fields := make([]reflectionField, 0, 2)
	property := target.member.AsPropertyDeclaration()
	if target.metadata.Requested("flamework:type") {
		identifier, err := reflectedNodeTypeID(state, property.Type, target.member)
		if err != nil {
			return nil, err
		}
		fields = append(fields, reflectionField{"flamework:type", state.factory.NewStringLiteral(identifier, ast.TokenFlagsNone)})
	}
	if target.metadata.Requested("flamework:guard") {
		guard, err := buildFlameworkGuard(
			state,
			state.checker.GetTypeAtLocation(target.member),
			reflectedTypeSource(property.Type, target.member),
		)
		if err != nil {
			return nil, err
		}
		fields = append(fields, reflectionField{"flamework:guard", guard})
	}
	return reflectionStatements(state.factory, reflectionEmission{target.className, target.propertyName, fields}), nil
}

func methodReflectionStatements(state *TransformState, target reflectionTarget) ([]*ast.Node, error) {
	signature := state.checker.GetSignatureFromDeclaration(target.member)
	if signature == nil {
		return nil, nil
	}
	fields := make([]reflectionField, 0, 5)
	methodType := target.member.Type()
	returnType := state.checker.GetReturnTypeOfSignature(signature)
	if target.metadata.Requested("flamework:return_type") {
		identifier, err := reflectedReturnTypeID(state, target.member, returnType)
		if err != nil {
			return nil, err
		}
		fields = append(fields, reflectionField{"flamework:return_type", state.factory.NewStringLiteral(identifier, ast.TokenFlagsNone)})
	}
	if target.metadata.Requested("flamework:return_guard") {
		guard, err := buildFlameworkGuard(state, returnType, reflectedTypeSource(methodType, target.member))
		if err != nil {
			return nil, err
		}
		fields = append(fields, reflectionField{"flamework:return_guard", guard})
	}
	if target.metadata.Requested("flamework:parameters") {
		identifiers, err := reflectedParameterIDs(state, target.member.Parameters())
		if err != nil {
			return nil, err
		}
		if len(identifiers) > 0 {
			fields = append(fields, reflectionField{"flamework:parameters", stringArray(state.factory, identifiers)})
		}
	}
	if target.metadata.Requested("flamework:parameter_names") {
		names := make([]string, 0, len(target.member.Parameters()))
		for _, parameter := range target.member.Parameters() {
			if ast.IsIdentifier(parameter.Name()) {
				names = append(names, parameter.Name().Text())
			} else {
				names = append(names, "_binding_")
			}
		}
		if len(names) > 0 {
			fields = append(fields, reflectionField{"flamework:parameter_names", stringArray(state.factory, names)})
		}
	}
	if target.metadata.Requested("flamework:parameter_guards") {
		guards := make([]*ast.Node, 0, len(target.member.Parameters()))
		for _, parameter := range target.member.Parameters() {
			guard, err := buildFlameworkGuard(state, state.checker.GetTypeAtLocation(parameter), parameter)
			if err != nil {
				return nil, err
			}
			guards = append(guards, guard)
		}
		if len(guards) > 0 {
			fields = append(fields, reflectionField{"flamework:parameter_guards", expressionArray(state.factory, guards)})
		}
	}
	return reflectionStatements(state.factory, reflectionEmission{target.className, target.propertyName, fields}), nil
}

func reflectedParameterIDs(state *TransformState, parameters []*ast.Node) ([]string, error) {
	identifiers := make([]string, 0, len(parameters))
	for _, parameter := range parameters {
		identifier, err := reflectedNodeTypeID(state, parameter.AsParameterDeclaration().Type, parameter)
		if err != nil {
			return nil, err
		}
		identifiers = append(identifiers, identifier)
	}
	return identifiers, nil
}

func reflectedNodeTypeID(state *TransformState, typeNode, location *ast.Node) (string, error) {
	if typeNode != nil {
		return nodeUID(state, typeNode)
	}
	return typeUID(state, state.checker.GetTypeAtLocation(location), location)
}

func reflectedReturnTypeID(state *TransformState, method *ast.Node, inferredType *checker.Type) (string, error) {
	if method.Type() != nil {
		return nodeUID(state, method.Type())
	}
	return typeUID(state, inferredType, method)
}

func reflectedTypeSource(typeNode, fallback *ast.Node) *ast.Node {
	if typeNode != nil {
		return typeNode
	}
	return fallback
}

func reflectionStatements(factory *ast.NodeFactory, emission reflectionEmission) []*ast.Node {
	statements := make([]*ast.Node, 0, len(emission.fields))
	for _, field := range emission.fields {
		arguments := []*ast.Node{
			factory.NewIdentifier(emission.className),
			factory.NewStringLiteral(field.name, ast.TokenFlagsNone),
			field.value,
		}
		if emission.propertyName != "" {
			arguments = append(arguments, factory.NewStringLiteral(emission.propertyName, ast.TokenFlagsNone))
		}
		statements = append(statements, newReflectCall(factory, "defineMetadata", arguments))
	}
	return statements
}
