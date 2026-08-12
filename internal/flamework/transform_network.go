package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func buildObfuscatedObjectIntrinsic(state *TransformState, trace *ast.Node, inputs []*checker.Type, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	if len(inputs) != 2 {
		return nil, nil, invalidMacro(trace, "Invalid obfuscate-obj intrinsic usage")
	}
	macro, ok, err := userMacroOfMany(state, trace, inputs[0])
	if err != nil || !ok {
		if err == nil {
			err = invalidMacro(trace, "Intrinsic obfuscate-obj received no inner macro")
		}
		return nil, nil, err
	}
	context := ""
	if inputs[1].IsStringLiteral() {
		context = stringLiteralValue(inputs[1])
	}
	if macro.properties != nil {
		properties, err := shuffledProperties(state, macro.properties, runtime)
		if err != nil {
			return nil, nil, err
		}
		for index := range properties {
			properties[index].name, err = hashText(state, properties[index].name, context, false)
			if err != nil {
				return nil, nil, err
			}
		}
		macro.properties = properties
	}
	return buildManyMacro(state, trace, macro, runtime)
}

func buildShuffleArrayIntrinsic(state *TransformState, trace *ast.Node, inputs []*checker.Type, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	if len(inputs) != 1 {
		return nil, nil, invalidMacro(trace, "Invalid shuffle-array intrinsic usage")
	}
	macro, ok, err := userMacroOfMany(state, trace, inputs[0])
	if err != nil || !ok {
		if err == nil {
			err = invalidMacro(trace, "Intrinsic shuffle-array received no inner macro")
		}
		return nil, nil, err
	}
	if macro.items != nil {
		macro.items, err = shuffledMacros(state, macro.items, runtime)
		if err != nil {
			return nil, nil, err
		}
	}
	return buildManyMacro(state, trace, macro, runtime)
}

func transformNetworkingMiddlewareIntrinsic(state *TransformState, signature *checker.Signature, arguments []*ast.Node, parameters []*ast.Symbol, runtime MacroRuntime) error {
	for _, parameter := range parameters {
		index := parameterIndex(signature, parameter)
		if index < 0 || index >= len(arguments) || !ast.IsObjectLiteralExpression(arguments[index]) {
			continue
		}
		object := arguments[index].AsObjectLiteralExpression()
		properties := append([]*ast.Node(nil), object.Properties.Nodes...)
		for propertyIndex, property := range properties {
			if !ast.IsPropertyAssignment(property) || property.Name() == nil || property.Name().Text() != "middleware" {
				continue
			}
			initializer := property.Initializer()
			if !ast.IsObjectLiteralExpression(initializer) {
				return invalidMacro(initializer, "Networking middleware must be an object.")
			}
			obfuscated, err := obfuscateMiddlewareObject(state, initializer, runtime)
			if err != nil {
				return err
			}
			assignment := property.AsPropertyAssignment()
			properties[propertyIndex] = state.factory.UpdatePropertyAssignment(assignment, assignment.Modifiers(), assignment.Name(), assignment.PostfixToken, assignment.Type, obfuscated)
		}
		arguments[index] = state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(properties), true)
	}
	return nil
}

func obfuscateMiddlewareObject(state *TransformState, node *ast.Node, runtime MacroRuntime) (*ast.Node, error) {
	properties := append([]*ast.Node(nil), node.AsObjectLiteralExpression().Properties.Nodes...)
	var err error
	properties, err = shuffledNodes(state, properties, runtime)
	if err != nil {
		return nil, err
	}
	for index, property := range properties {
		if !ast.IsPropertyAssignment(property) || property.Name() == nil || property.Name().Text() == "" {
			continue
		}
		assignment := property.AsPropertyAssignment()
		initializer := assignment.Initializer
		if ast.IsObjectLiteralExpression(initializer) {
			initializer, err = obfuscateMiddlewareObject(state, initializer, runtime)
			if err != nil {
				return nil, err
			}
		}
		original := property.Name().Text()
		hash, err := hashText(state, original, "remotes", false)
		if err != nil {
			return nil, err
		}
		literal := state.factory.NewStringLiteral(original, ast.TokenFlagsNone)
		computed := state.factory.NewComputedPropertyName(state.factory.NewAsExpression(state.factory.NewStringLiteral(hash, ast.TokenFlagsNone), state.factory.NewLiteralTypeNode(literal)))
		properties[index] = state.factory.UpdatePropertyAssignment(assignment, assignment.Modifiers(), computed, assignment.PostfixToken, assignment.Type, initializer)
	}
	return state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(properties), node.AsObjectLiteralExpression().MultiLine), nil
}

func shuffledProperties(state *TransformState, values []macroProperty, runtime MacroRuntime) ([]macroProperty, error) {
	indexes, err := shuffledIndexes(state, len(values), runtime)
	if err != nil {
		return nil, err
	}
	result := make([]macroProperty, len(values))
	for index, source := range indexes {
		result[index] = values[source]
	}
	return result, nil
}

func shuffledMacros(state *TransformState, values []userMacro, runtime MacroRuntime) ([]userMacro, error) {
	indexes, err := shuffledIndexes(state, len(values), runtime)
	if err != nil {
		return nil, err
	}
	result := make([]userMacro, len(values))
	for index, source := range indexes {
		result[index] = values[source]
	}
	return result, nil
}

func shuffledNodes(state *TransformState, values []*ast.Node, runtime MacroRuntime) ([]*ast.Node, error) {
	indexes, err := shuffledIndexes(state, len(values), runtime)
	if err != nil {
		return nil, err
	}
	result := make([]*ast.Node, len(values))
	for index, source := range indexes {
		result[index] = values[source]
	}
	return result, nil
}

func shuffledIndexes(state *TransformState, length int, runtime MacroRuntime) ([]int, error) {
	indexes := make([]int, length)
	for index := range indexes {
		indexes[index] = index
	}
	if !state.project.config.Obfuscation {
		return indexes, nil
	}
	if runtime.RandomIndex == nil {
		return nil, invalidMacro(nil, "Flamework random-index runtime is not configured")
	}
	for index := length - 1; index > 0; index-- {
		target, err := runtime.RandomIndex(index + 1)
		if err != nil {
			return nil, fmt.Errorf("shuffle Flamework metadata: %w", err)
		}
		if target < 0 || target > index {
			return nil, invalidMacro(nil, "Flamework random index %d is outside [0, %d)", target, index+1)
		}
		indexes[index], indexes[target] = indexes[target], indexes[index]
	}
	return indexes, nil
}
