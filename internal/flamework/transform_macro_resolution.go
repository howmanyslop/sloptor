package flamework

import (
	"sort"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func userMacroOfUnion(state *TransformState, trace *ast.Node, target *checker.Type) (userMacro, bool, error) {
	for _, constituent := range target.Distributed() {
		macro, ok, err := userMacroOfType(state, trace, constituent)
		if err != nil || ok {
			return macro, ok, err
		}
	}
	return userMacro{}, false, nil
}

func userMacroOfType(state *TransformState, trace *ast.Node, target *checker.Type) (userMacro, bool, error) {
	if many := state.checker.GetTypeOfPropertyOfType(target, "_flamework_macro_many"); many != nil {
		return userMacroOfMany(state, trace, many)
	}
	return basicUserMacro(state, trace, target)
}

func basicUserMacro(state *TransformState, trace *ast.Node, target *checker.Type) (userMacro, bool, error) {
	if generic := state.checker.GetTypeOfPropertyOfType(target, "_flamework_macro_generic"); generic != nil {
		macroTarget := state.checker.GetTypeOfPropertyOfType(generic, "0")
		metadata := state.checker.GetTypeOfPropertyOfType(generic, "1")
		if macroTarget == nil || metadata == nil {
			return userMacro{}, false, nil
		}
		if !metadata.IsStringLiteral() {
			return userMacro{}, false, invalidMacro(trace, "Flamework encountered invalid generic metadata %q", state.checker.TypeToString(metadata))
		}
		return userMacro{kind: macroGeneric, target: macroTarget, metadata: stringLiteralValue(metadata), order: metadata.Id()}, true, nil
	}
	if caller := state.checker.GetTypeOfPropertyOfType(target, "_flamework_macro_caller"); caller != nil {
		if !caller.IsStringLiteral() {
			return userMacro{}, false, invalidMacro(trace, "Flamework encountered invalid caller metadata %q", state.checker.TypeToString(caller))
		}
		return userMacro{kind: macroCaller, metadata: stringLiteralValue(caller), order: caller.Id()}, true, nil
	}
	if hash := state.checker.GetTypeOfPropertyOfType(target, "_flamework_macro_hash"); hash != nil {
		return hashUserMacro(state, trace, hash)
	}
	nonNullable := state.checker.GetNonNullableType(target)
	if labels := state.checker.GetTypeOfPropertyOfType(nonNullable, "_flamework_macro_tuple_labels"); labels != nil {
		return tupleLabelsMacro(state, labels), true, nil
	}
	if intrinsic := state.checker.GetTypeOfPropertyOfType(nonNullable, "_flamework_intrinsic"); intrinsic != nil && checker.IsTupleType(intrinsic) {
		inputs := state.checker.GetTypeArguments(intrinsic)
		if len(inputs) == 0 || !inputs[0].IsStringLiteral() {
			return userMacro{}, false, invalidMacro(trace, "Flamework encountered invalid intrinsic metadata")
		}
		return userMacro{kind: macroIntrinsic, intrinsic: stringLiteralValue(inputs[0]), inputs: inputs[1:]}, true, nil
	}
	return userMacro{}, false, nil
}

func userMacroOfMany(state *TransformState, trace *ast.Node, target *checker.Type) (userMacro, bool, error) {
	if macro, ok, err := basicUserMacro(state, trace, target); err != nil || ok {
		return macro, ok, err
	}
	if nested := state.checker.GetTypeOfPropertyOfType(target, "_flamework_macro_many"); nested != nil {
		return userMacroOfMany(state, trace, nested)
	}
	if checker.IsTupleType(target) {
		return manyItemsMacro(state, trace, state.checker.GetTypeArguments(target))
	}
	if isArrayMacroTarget(state, trace, target) {
		arguments := state.checker.GetTypeArguments(target)
		if len(arguments) != 1 {
			return userMacro{}, false, invalidMacro(trace, "Flamework array macro has %d type arguments", len(arguments))
		}
		return manyItemsMacro(state, trace, arguments[0].Distributed())
	}
	if target.Flags()&checker.TypeFlagsObject != 0 || target.IsIntersection() {
		properties := state.checker.GetPropertiesOfType(target)
		result := userMacro{kind: macroMany, properties: make([]macroProperty, 0, len(properties))}
		ordered := true
		for _, property := range properties {
			memberType := state.checker.GetTypeOfPropertyOfType(target, property.Name)
			member, ok, err := userMacroOfMany(state, trace, memberType)
			if err != nil {
				return userMacro{}, false, err
			}
			if !ok {
				return userMacro{}, false, nil
			}
			ordered = ordered && member.order != 0
			result.properties = append(result.properties, macroProperty{name: property.Name, macro: member})
		}
		if ordered {
			sort.SliceStable(result.properties, func(left, right int) bool {
				return result.properties[left].macro.order < result.properties[right].macro.order
			})
		}
		return result, true, nil
	}
	if target.IsStringLiteral() {
		return userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindStringLiteral, text: stringLiteralValue(target)}}, true, nil
	}
	if target.IsNumberLiteral() {
		return userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindNumericLiteral, number: target.AsLiteralType().String()}}, true, nil
	}
	if target.Flags()&checker.TypeFlagsBooleanLiteral != 0 {
		value, ok := target.AsLiteralType().Value().(bool)
		if !ok {
			return userMacro{}, false, invalidMacro(trace, "Flamework boolean literal has an invalid value")
		}
		return userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindTrueKeyword, boolean: value}}, true, nil
	}
	if target.Flags()&checker.TypeFlagsUndefined != 0 || target.Flags()&checker.TypeFlagsNever != 0 {
		return userMacro{kind: macroLiteral, literal: literalValue{kind: ast.KindIdentifier}}, true, nil
	}
	return userMacro{}, false, invalidMacro(trace, "Unknown type '%s' encountered", state.checker.TypeToString(target))
}

func isArrayMacroTarget(state *TransformState, trace *ast.Node, target *checker.Type) bool {
	if target.ObjectFlags()&checker.ObjectFlagsReference == 0 {
		return false
	}
	targetSymbol := target.Target().Symbol()
	if targetSymbol == nil {
		return false
	}
	for _, name := range []string{"Array", "ReadonlyArray"} {
		if state.checker.ResolveName(name, trace, ast.SymbolFlagsType, false) == targetSymbol {
			return true
		}
	}
	return false
}

func manyItemsMacro(state *TransformState, trace *ast.Node, types []*checker.Type) (userMacro, bool, error) {
	items := make([]userMacro, 0, len(types))
	for _, itemType := range types {
		if itemType.Flags()&checker.TypeFlagsNever != 0 {
			break
		}
		item, ok, err := userMacroOfMany(state, trace, itemType)
		if err != nil {
			return userMacro{}, false, err
		}
		if !ok {
			return userMacro{}, false, nil
		}
		items = append(items, item)
	}
	return userMacro{kind: macroMany, items: items}, true, nil
}
