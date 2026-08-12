package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func updateFlameworkComponentConfig(
	state *TransformState,
	classDeclaration *ast.Node,
	properties []*ast.Node,
) ([]*ast.Node, error) {
	updated, err := updateFlameworkAttributeGuards(state, classDeclaration, properties)
	if err != nil {
		return nil, err
	}
	return updateFlameworkInstanceGuard(state, classDeclaration, updated)
}

func updateFlameworkAttributeGuards(
	state *TransformState,
	classDeclaration *ast.Node,
	properties []*ast.Node,
) ([]*ast.Node, error) {
	classType := state.checker.GetTypeAtLocation(classDeclaration)
	attributeSymbol := state.checker.GetPropertyOfType(classType, "attributes")
	if attributeSymbol == nil || !flameworkSymbolRequestsMetadata(state, attributeSymbol, componentAttributesMetadata) {
		return properties, nil
	}
	attributeType := state.checker.GetTypeOfSymbolAtLocation(attributeSymbol, classDeclaration)
	customIndex, customAttributes := namedProperty(properties, "attributes")
	omitted := omittedFlameworkAttributeGuards(state, classDeclaration, attributeType, customAttributes)
	guards := make([]*ast.Node, 0)
	for _, property := range state.checker.GetPropertiesOfType(attributeType) {
		if omitted[property.Name] {
			continue
		}
		propertyType := state.checker.GetTypeOfPropertyOfType(attributeType, property.Name)
		guard, err := buildFlameworkGuard(state, propertyType, classDeclaration)
		if err != nil {
			return nil, fmt.Errorf("generate component attribute guard %q: %w", property.Name, err)
		}
		guards = append(guards, state.factory.NewPropertyAssignment(
			nil,
			state.factory.NewStringLiteral(property.Name, ast.TokenFlagsNone),
			nil,
			nil,
			guard,
		))
	}
	if customIndex >= 0 && ast.IsPropertyAssignment(customAttributes) && ast.IsObjectLiteralExpression(customAttributes.Initializer()) {
		initializer := customAttributes.Initializer().AsObjectLiteralExpression()
		merged := append([]*ast.Node(nil), initializer.Properties.Nodes...)
		merged = append(merged, guards...)
		updatedProperty := state.factory.UpdatePropertyAssignment(
			customAttributes.AsPropertyAssignment(),
			customAttributes.Modifiers(),
			customAttributes.Name(),
			customAttributes.PostfixToken(),
			customAttributes.Type(),
			state.factory.UpdateObjectLiteralExpression(initializer, state.factory.NewNodeList(merged), true),
		)
		updated := append([]*ast.Node(nil), properties...)
		updated[customIndex] = updatedProperty
		return updated, nil
	}
	attributeGuards := state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(guards), true)
	return append(properties, state.factory.NewPropertyAssignment(
		nil,
		state.factory.NewStringLiteral("attributes", ast.TokenFlagsNone),
		nil,
		nil,
		attributeGuards,
	)), nil
}

func omittedFlameworkAttributeGuards(
	state *TransformState,
	classDeclaration *ast.Node,
	attributeType *checker.Type,
	customAttributes *ast.Node,
) map[string]bool {
	omitted := objectPropertyNames(customAttributes)
	superClass := flameworkDirectSuperclass(state, classDeclaration)
	if superClass == nil {
		return omitted
	}
	superType := state.checker.GetTypeAtLocation(superClass)
	superAttributes := state.checker.GetPropertyOfType(superType, "attributes")
	if superAttributes == nil {
		return omitted
	}
	superAttributeType := state.checker.GetTypeOfSymbolAtLocation(superAttributes, superClass)
	for _, property := range state.checker.GetPropertiesOfType(superAttributeType) {
		propertyType := state.checker.GetTypeOfPropertyOfType(attributeType, property.Name)
		superPropertyType := state.checker.GetTypeOfPropertyOfType(superAttributeType, property.Name)
		if propertyType != nil && propertyType == superPropertyType {
			omitted[property.Name] = true
		}
	}
	return omitted
}

func updateFlameworkInstanceGuard(
	state *TransformState,
	classDeclaration *ast.Node,
	properties []*ast.Node,
) ([]*ast.Node, error) {
	classType := state.checker.GetTypeAtLocation(classDeclaration)
	instanceSymbol := state.checker.GetPropertyOfType(classType, "instance")
	if instanceSymbol == nil || !flameworkSymbolRequestsMetadata(state, instanceSymbol, componentInstanceMetadata) {
		return properties, nil
	}
	if index, _ := namedProperty(properties, "instanceGuard"); index >= 0 {
		return properties, nil
	}
	superClass := flameworkDirectSuperclass(state, classDeclaration)
	if superClass == nil {
		return properties, nil
	}
	superType := state.checker.GetTypeAtLocation(superClass)
	superSymbol := state.checker.GetPropertyOfType(superType, "instance")
	if superSymbol == nil {
		return properties, nil
	}
	instanceType := state.checker.GetTypeOfSymbolAtLocation(instanceSymbol, classDeclaration)
	superInstanceType := state.checker.GetTypeOfSymbolAtLocation(superSymbol, superClass)
	if superInstanceType.Flags()&checker.TypeFlagsTypeParameter != 0 {
		if defaultType := state.checker.GetDefaultFromTypeParameter(superInstanceType); defaultType == instanceType {
			return properties, nil
		}
	} else if state.checker.IsTypeAssignableTo(superInstanceType, instanceType) {
		return properties, nil
	}
	guard, err := buildFlameworkGuard(state, instanceType, classDeclaration)
	if err != nil {
		return nil, fmt.Errorf("generate component instance guard: %w", err)
	}
	return append(properties, state.factory.NewPropertyAssignment(
		nil,
		state.factory.NewStringLiteral("instanceGuard", ast.TokenFlagsNone),
		nil,
		nil,
		guard,
	)), nil
}

func flameworkDirectSuperclass(state *TransformState, classDeclaration *ast.Node) *ast.Node {
	extends := ast.GetExtendsHeritageClauseElement(classDeclaration)
	if extends == nil {
		return nil
	}
	symbol := state.checker.GetSymbolAtLocation(extends.Expression())
	if symbol == nil {
		return nil
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	for _, declaration := range symbol.Declarations {
		if ast.IsClassDeclaration(declaration) {
			return declaration
		}
	}
	return nil
}

func namedProperty(properties []*ast.Node, name string) (int, *ast.Node) {
	for index, property := range properties {
		if property.Name() != nil && property.Name().Text() == name {
			return index, property
		}
	}
	return -1, nil
}

func objectPropertyNames(property *ast.Node) map[string]bool {
	names := make(map[string]bool)
	if property == nil || !ast.IsPropertyAssignment(property) || !ast.IsObjectLiteralExpression(property.Initializer()) {
		return names
	}
	for _, child := range property.Initializer().AsObjectLiteralExpression().Properties.Nodes {
		if child.Name() != nil && (ast.IsIdentifier(child.Name()) || ast.IsStringLiteral(child.Name())) {
			names[child.Name().Text()] = true
		}
	}
	return names
}
