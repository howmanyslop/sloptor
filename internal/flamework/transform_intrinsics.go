package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func hashUserMacro(state *TransformState, trace *ast.Node, hashType *checker.Type) (userMacro, bool, error) {
	textType := state.checker.GetTypeOfPropertyOfType(hashType, "0")
	contextType := state.checker.GetTypeOfPropertyOfType(hashType, "1")
	obfuscationType := state.checker.GetTypeOfPropertyOfType(hashType, "2")
	if textType == nil || !textType.IsStringLiteral() || contextType == nil {
		return userMacro{}, false, invalidMacro(trace, "Flamework encountered invalid hash metadata")
	}
	context := DefaultHashContext
	if contextType.IsStringLiteral() {
		context = stringLiteralValue(contextType)
	}
	text := stringLiteralValue(textType)
	var hash string
	var err error
	if obfuscationType != nil {
		hash, err = hashText(state, text, context, false)
	} else {
		hash, err = state.project.HashString(text, context)
	}
	if err != nil {
		return userMacro{}, false, err
	}
	return userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindStringLiteral, text: hash}}, true, nil
}

func tupleLabelsMacro(state *TransformState, target *checker.Type) userMacro {
	if !checker.IsTupleType(target) {
		return userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindIdentifier}}
	}
	infos := target.TargetTupleType().ElementInfos()
	items := make([]userMacro, len(infos))
	for index, info := range infos {
		name := ""
		if declaration := info.LabeledDeclaration(); declaration != nil && declaration.Name() != nil {
			name = declaration.Name().Text()
		}
		items[index] = userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindStringLiteral, text: name}}
	}
	return userMacro{kind: macroMany, items: items}
}

func buildManyMacro(state *TransformState, trace *ast.Node, macro userMacro, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	prerequisites := make([]*ast.Node, 0)
	if macro.items != nil {
		elements := make([]*ast.Node, 0, len(macro.items))
		for _, item := range macro.items {
			expression, statements, err := buildUserMacro(state, trace, item, runtime)
			if err != nil {
				return nil, nil, err
			}
			elements = append(elements, expression)
			prerequisites = append(prerequisites, statements...)
		}
		return state.factory.NewArrayLiteralExpression(state.factory.NewNodeList(elements), true), prerequisites, nil
	}
	properties := make([]*ast.Node, 0, len(macro.properties))
	for _, property := range macro.properties {
		expression, statements, err := buildUserMacro(state, trace, property.macro, runtime)
		if err != nil {
			return nil, nil, err
		}
		if isUndefinedAsNever(expression) {
			continue
		}
		properties = append(properties, state.factory.NewPropertyAssignment(nil, state.factory.NewStringLiteral(property.name, ast.TokenFlagsNone), nil, nil, expression))
		prerequisites = append(prerequisites, statements...)
	}
	return state.factory.NewObjectLiteralExpression(state.factory.NewNodeList(properties), false), prerequisites, nil
}

func buildIntrinsicMacro(state *TransformState, trace *ast.Node, macro userMacro, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	switch macro.intrinsic {
	case "pathglob":
		if len(macro.inputs) != 1 {
			return nil, nil, invalidMacro(trace, "Invalid pathglob intrinsic usage")
		}
		expression, err := buildPathGlobIntrinsic(state, trace, macro.inputs[0])
		return expression, nil, err
	case "path":
		if len(macro.inputs) != 1 {
			return nil, nil, invalidMacro(trace, "Invalid path intrinsic usage")
		}
		expression, err := buildPathIntrinsic(state, trace, macro.inputs[0])
		return expression, nil, err
	case "obfuscate-obj":
		return buildObfuscatedObjectIntrinsic(state, trace, macro.inputs, runtime)
	case "shuffle-array":
		return buildShuffleArrayIntrinsic(state, trace, macro.inputs, runtime)
	case "tuple-guards":
		if len(macro.inputs) != 1 || runtime.BuildGuard == nil {
			return nil, nil, ErrGuardBuilderAbsent
		}
		return buildTupleGuardsIntrinsic(state, trace, macro.inputs[0], runtime)
	case "declaration-uid":
		expression, err := buildDeclarationUIDIntrinsic(state, trace)
		return expression, nil, err
	case "symbol-id":
		if len(macro.inputs) != 1 || !ast.IsCallExpression(trace) {
			return nil, nil, invalidMacro(trace, "Invalid symbol-id intrinsic usage")
		}
		expression, err := buildSymbolIDIntrinsic(state, trace, macro.inputs[0])
		return expression, nil, err
	default:
		return nil, nil, invalidMacro(trace, "Unexpected intrinsic ID %q with %d inputs", macro.intrinsic, len(macro.inputs))
	}
}

func buildTupleGuardsIntrinsic(state *TransformState, trace *ast.Node, target *checker.Type, runtime MacroRuntime) (*ast.Node, []*ast.Node, error) {
	if state.checker.IsArrayLikeType(target) && !checker.IsTupleType(target) {
		typeArguments := state.checker.GetTypeArguments(target)
		if len(typeArguments) != 1 {
			return nil, nil, invalidMacro(trace, "Intrinsic encountered invalid array type: %s", state.checker.TypeToString(target))
		}
		result, err := runtime.BuildGuard(state, trace, typeArguments[0])
		if err != nil {
			return nil, nil, err
		}
		fixed := state.factory.NewArrayLiteralExpression(state.factory.NewNodeList(nil), true)
		return state.factory.NewArrayLiteralExpression(state.factory.NewNodeList([]*ast.Node{fixed, result.Expression}), true), result.Statements, nil
	}
	if !checker.IsTupleType(target) {
		return nil, nil, invalidMacro(trace, "Intrinsic encountered non-tuple type: %s", state.checker.TypeToString(target))
	}
	fixedGuards := make([]*ast.Node, 0)
	statements := make([]*ast.Node, 0)
	restGuard := state.factory.NewIdentifier("undefined")
	elementInfos := target.TargetTupleType().ElementInfos()
	for index, itemType := range state.checker.GetTypeArguments(target) {
		guardTrace := trace
		if index < len(elementInfos) && elementInfos[index].LabeledDeclaration() != nil {
			guardTrace = elementInfos[index].LabeledDeclaration()
		}
		result, err := runtime.BuildGuard(state, guardTrace, itemType)
		if err != nil {
			return nil, nil, err
		}
		if index < len(elementInfos) && elementInfos[index].TupleElementFlags()&checker.ElementFlagsRest != 0 {
			restGuard = result.Expression
		} else {
			fixedGuards = append(fixedGuards, result.Expression)
		}
		statements = append(statements, result.Statements...)
	}
	fixed := state.factory.NewArrayLiteralExpression(state.factory.NewNodeList(fixedGuards), true)
	return state.factory.NewArrayLiteralExpression(state.factory.NewNodeList([]*ast.Node{fixed, restGuard}), true), statements, nil
}

func hashText(state *TransformState, text, context string, force bool) (string, error) {
	if !force && !state.project.config.Obfuscation {
		return text, nil
	}
	hash, err := state.project.HashString(text, context)
	if err != nil {
		return "", fmt.Errorf("hash Flamework text: %w", err)
	}
	return hash, nil
}

func isUndefinedAsNever(expression *ast.Node) bool {
	return ast.IsAsExpression(expression) && isUndefinedExpression(expression.Expression())
}
