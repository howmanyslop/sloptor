package transformer

import (
	"slices"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// This file mirrors a TS type as Luau type text. Positions matter:
//
//   - a NESTED position (alias body, table member, element, key/value, function
//     type parameter or return) is never type-tagged, so it is mirrored in full
//     and widens to `any` when it cannot be written;
//   - a RETURN type is not tagged either, but a shape that cannot be mirrored
//     stays unannotated rather than claiming `any`;
//   - a top-level PARAMETER annotation becomes a runtime tag, so only shapes
//     Luau understands are emitted (`number`, `string`, vectors, tables,
//     functions, own generics) and anything else is left unannotated.

// getNativeType mirrors t as Luau type text for the given position. It also
// reports the rendered kind and whether the text used an ENCLOSING function's
// generic in a tagged position (rule R1), in which case a parameter annotation
// must be dropped.
func getNativeType(state *State, checkerType *checker.Type, nativePosition nativePosition, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	if checkerType == nil {
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	}
	if checkerType.IsUnion() {
		return getNativeUnionType(state, checkerType, nativePosition, depth, seen)
	}
	if checkerType.IsIntersection() {
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	}
	if checkerType.Flags()&checker.TypeFlagsTypeVariable != 0 {
		return getNativeTypeVariable(state, checkerType, nativePosition, depth, seen)
	}

	switch {
	case isNumberTypeCheck(checkerType):
		return nativeTypeNumber, nativeKindNumber, false
	case isStringTypeCheck(checkerType):
		return nativeTypeString, nativeKindString, false
	case isBooleanTypeCheck(checkerType):
		if nativePosition == nativePositionParameter {
			// Booleans are tagged correctly already; the annotation would
			// only add an entry guard.
			return "", nativeKindBoolean, false
		}
		return nativeTypeBoolean, nativeKindBoolean, false
	case checkerType.Flags()&(checker.TypeFlagsUndefined|checker.TypeFlagsVoid) != 0:
		switch nativePosition {
		case nativePositionReturn:
			return nativeTypeUnit, nativeKindNil, false
		case nativePositionNested:
			return nativeTypeNil, nativeKindNil, false
		}
		return "", nativeKindUnspellable, false
	case checkerType.Flags()&(checker.TypeFlagsAnyOrUnknown|checker.TypeFlagsNever) != 0:
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	case checkerType.Flags()&checker.TypeFlagsObject == 0:
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	}
	return getNativeObjectType(state, checkerType, nativePosition, depth, seen)
}

// getNativeUnspellable is what a position emits for a type it cannot mirror.
func getNativeUnspellable(nativePosition nativePosition) string {
	if nativePosition == nativePositionNested {
		return nativeTypeAny
	}
	return ""
}

// getNativeUnionType mirrors every union member. `undefined` becomes the optional
// marker; a member that cannot be mirrored widens the whole nested union to
// `any` and disqualifies a tagged one.
func getNativeUnionType(state *State, checkerType *checker.Type, nativePosition nativePosition, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	var texts []string
	var kinds []nativeTypeKind
	optional := false
	outerGeneric := false
	unspellable := false

	for _, member := range checkerType.Types() {
		if isUndefinedTypeCheck(member) {
			optional = true
			continue
		}
		text, kind, outer := getNativeType(state, member, nativePosition, depth, seen)
		if text == "" {
			unspellable = true
			continue
		}
		outerGeneric = outerGeneric || outer
		if !slices.Contains(texts, text) {
			texts = append(texts, text)
			kinds = append(kinds, kind)
		}
	}
	if unspellable || len(texts) == 0 {
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, outerGeneric
	}
	if len(texts) == 1 {
		text := texts[0]
		if optional {
			text = getNativeOptionalType(text)
		}
		return text, kinds[0], outerGeneric
	}
	if nativePosition == nativePositionParameter {
		// Luau tags every table member of a union as TABLE, but a union that
		// mixes a table with another kind has no single useful tag.
		for _, kind := range kinds {
			if kind != nativeKindTable {
				return "", nativeKindUnspellable, outerGeneric
			}
		}
	}
	for index, text := range texts {
		if kinds[index] == nativeKindFunction && len(texts) > 1 {
			texts[index] = "(" + text + ")"
		}
	}
	text := strings.Join(texts, " | ")
	if optional {
		text = getNativeOptionalType(text)
	}
	return text, kinds[0], outerGeneric
}

// getNativeTypeVariable mirrors a type variable: its constraint when that yields a
// usable type, otherwise its name when the generic is in scope, otherwise
// nothing.
func getNativeTypeVariable(state *State, checkerType *checker.Type, nativePosition nativePosition, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	name, own, ok := nativeTypeParameterName(state, checkerType)
	if !ok {
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	}
	constraint := state.Checker.GetBaseConstraintOfType(checkerType)
	if constraint != nil && constraint != checkerType && constraint.Flags()&checker.TypeFlagsAnyOrUnknown == 0 {
		if text, kind, outer := getNativeType(state, constraint, nativePosition, depth, seen); text != "" {
			return text, kind, outer
		}
	}
	return name, nativeKindGeneric, nativePosition == nativePositionParameter && !own
}

// getNativeObjectType mirrors an object type: a named alias, a Roblox datatype by
// name, a function type, or a structural table.
func getNativeObjectType(state *State, checkerType *checker.Type, nativePosition nativePosition, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	if reference, kind, ok := getNativeAliasReference(state, checkerType, depth, seen); ok {
		return reference, kind, false
	}
	if name := getNativeRobloxTypeName(state, checkerType); name != "" {
		if nativePosition == nativePositionParameter && !isNativeVectorTypeName(name) {
			return "", nativeKindUnspellable, false
		}
		return name, nativeKindVector, false
	}
	if len(state.Checker.GetConstructSignatures(checkerType)) > 0 {
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	}
	if len(state.Checker.GetCallSignatures(checkerType)) > 0 {
		text, kind, _ := getNativeFunctionType(state, checkerType, false, depth, seen)
		if text == "" {
			return getNativeUnspellable(nativePosition), kind, false
		}
		return text, kind, false
	}
	if symbol := checkerType.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsEnum != 0 {
		return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
	}
	if isNativeTableType(state, checkerType) {
		text := getNativeTableTypeAt(state, checkerType, depth, seen)
		if text == "" {
			// Recursive or too deeply nested: still a table, just not
			// spellable.
			text = nativeTypeAnyTable
		}
		return text, nativeKindTable, false
	}
	return getNativeUnspellable(nativePosition), nativeKindUnspellable, false
}

// getNativeFunctionType mirrors a single-call-signature object as a Luau function
// type. Overloads, construct signatures and LuaTuple results have no faithful
// spelling and fall back to `(...any) -> ...any`. receiver marks an interface
// METHOD member, whose Luau call form (`obj:method()`) passes the object as an
// implicit first argument.
func getNativeFunctionType(state *State, checkerType *checker.Type, receiver bool, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	signatures := state.Checker.GetCallSignatures(checkerType)
	if len(signatures) != 1 {
		return nativeTypeAnyFunction, nativeKindFunction, false
	}
	signature := signatures[0]
	if IsLuaTupleType(state).Check(state.Checker.GetReturnTypeOfSignature(signature)) {
		return nativeTypeAnyFunction, nativeKindFunction, false
	}

	var parts []string
	// A method signature carries its receiver, which Luau passes as the
	// implicit first argument of a `obj:method()` call.
	if receiver || signature.ThisParameter() != nil {
		parts = append(parts, "self: "+nativeTypeAny)
	}
	parameters := signature.Parameters()
	minArgumentCount := signature.MinArgumentCount()
	hasRest := signature.HasRestParameter()
	for index, parameter := range parameters {
		parameterType := state.Checker.GetTypeOfSymbol(parameter)
		if hasRest && index == len(parameters)-1 {
			element := state.Checker.GetElementTypeOfArrayType(parameterType)
			if element == nil {
				return nativeTypeAnyFunction, nativeKindFunction, false
			}
			text, _, _ := getNativeType(state, element, nativePositionNested, depth+1, seen)
			if text == "" {
				text = nativeTypeAny
			}
			parts = append(parts, "..."+text)
			continue
		}
		text, _, _ := getNativeType(state, parameterType, nativePositionNested, depth+1, seen)
		if text == "" {
			text = nativeTypeAny
		}
		if index >= minArgumentCount {
			text = getNativeOptionalType(text)
		}
		if luau.IsValidIdentifier(parameter.Name) {
			parts = append(parts, parameter.Name+": "+text)
		} else {
			parts = append(parts, text)
		}
	}

	returnType := state.Checker.GetReturnTypeOfSignature(signature)
	var result string
	if isUndefinedTypeCheck(returnType) {
		result = nativeTypeUnit
	} else {
		result, _, _ = getNativeType(state, returnType, nativePositionNested, depth+1, seen)
		if result == "" {
			result = nativeTypeAny
		}
		if doesNativeHaveTopLevelOperator(result, "|") {
			result = "(" + result + ")"
		}
	}
	return "(" + strings.Join(parts, ", ") + ") -> " + result, nativeKindFunction, false
}

// getNativeTableTypeAt spells one table level as a Luau table type literal, or ""
// when its shape cannot be written out. `depth` bounds nesting and `seen` stops
// a self-referential interface from recursing forever.
func getNativeTableTypeAt(state *State, checkerType *checker.Type, depth int, seen map[*checker.Type]bool) string {
	checkerType = getNativeTypeConstraint(state, checkerType)
	if depth > nativeTypeMaxDepth || seen[checkerType] {
		return ""
	}
	if seen == nil {
		seen = map[*checker.Type]bool{}
	}
	seen[checkerType] = true
	defer delete(seen, checkerType)

	switch {
	case checkerType.IsTupleType():
		var names []string
		for _, element := range getTypeArguments(state, checkerType) {
			name, _, _ := getNativeType(state, element, nativePositionNested, depth+1, seen)
			if name == "" {
				name = nativeTypeAny
			}
			if !slices.Contains(names, name) {
				names = append(names, name)
			}
		}
		if len(names) == 0 {
			return ""
		}
		return "{ " + strings.Join(names, " | ") + " }"
	case IsArrayType(state).Check(checkerType):
		element := state.Checker.GetElementTypeOfArrayType(checkerType)
		if element == nil {
			return ""
		}
		name, _, _ := getNativeType(state, element, nativePositionNested, depth+1, seen)
		if name == "" {
			name = nativeTypeAny
		}
		return "{ " + name + " }"
	case IsMapType(state).Check(checkerType):
		arguments := getTypeArguments(state, checkerType)
		if len(arguments) < 2 {
			return ""
		}
		key, _, _ := getNativeType(state, arguments[0], nativePositionNested, depth+1, seen)
		value, _, _ := getNativeType(state, arguments[1], nativePositionNested, depth+1, seen)
		if key == "" {
			key = nativeTypeAny
		}
		if value == "" {
			value = nativeTypeAny
		}
		return "{ [" + key + "]: " + value + " }"
	case IsSetType(state).Check(checkerType):
		arguments := getTypeArguments(state, checkerType)
		if len(arguments) < 1 {
			return ""
		}
		name, _, _ := getNativeType(state, arguments[0], nativePositionNested, depth+1, seen)
		if name == "" {
			name = nativeTypeAny
		}
		return "{ [" + name + "]: true }"
	}

	var parts []string
	for _, property := range state.Checker.GetPropertiesOfType(checkerType) {
		name := property.Name
		if strings.HasPrefix(name, "__@") {
			// Computed/ES-symbol member: no Luau spelling.
			continue
		}
		value := getNativePropertyType(state, property, depth, seen)
		if property.Flags&ast.SymbolFlagsOptional != 0 {
			value = getNativeOptionalType(value)
		}
		if luau.IsValidIdentifier(name) {
			parts = append(parts, name+": "+value)
		} else {
			parts = append(parts, `["`+getNativeQuotedKey(name)+`"]: `+value)
		}
	}
	// Luau allows at most one table indexer, so two TS index signatures have to
	// collapse into one. When their value types differ the property is still a
	// table, so the imprecise `any` value stays correct.
	stringIndex := state.Checker.GetStringIndexType(checkerType)
	numberIndex := state.Checker.GetNumberIndexType(checkerType)
	switch {
	case stringIndex != nil && numberIndex != nil:
		stringValue, _, _ := getNativeType(state, stringIndex, nativePositionNested, depth+1, seen)
		numberValue, _, _ := getNativeType(state, numberIndex, nativePositionNested, depth+1, seen)
		value := nativeTypeAny
		if stringValue != "" && stringValue == numberValue {
			value = stringValue
		}
		parts = append(parts, "["+nativeTypeAny+"]: "+value)
	case stringIndex != nil:
		value, _, _ := getNativeType(state, stringIndex, nativePositionNested, depth+1, seen)
		if value == "" {
			value = nativeTypeAny
		}
		parts = append(parts, "["+nativeTypeString+"]: "+value)
	case numberIndex != nil:
		value, _, _ := getNativeType(state, numberIndex, nativePositionNested, depth+1, seen)
		if value == "" {
			value = nativeTypeAny
		}
		parts = append(parts, "["+nativeTypeNumber+"]: "+value)
	}
	if len(parts) == 0 {
		return ""
	}
	return "{ " + strings.Join(parts, ", ") + " }"
}

// getNativePropertyType mirrors one table member. A METHOD member (`run(): T`)
// takes its receiver as the implicit first argument, so its Luau function type
// spells that parameter; a function-typed property does not.
func getNativePropertyType(state *State, property *ast.Symbol, depth int, seen map[*checker.Type]bool) string {
	propertyType := state.Checker.GetTypeOfSymbol(property)
	if property.Flags&ast.SymbolFlagsMethod != 0 && len(state.Checker.GetCallSignatures(propertyType)) > 0 {
		if text, kind, _ := getNativeFunctionType(state, propertyType, true, depth+1, seen); text != "" && kind == nativeKindFunction {
			return text
		}
	}
	text, _, _ := getNativeType(state, propertyType, nativePositionNested, depth+1, seen)
	if text == "" {
		text = nativeTypeAny
	}
	return text
}

// getNativeTypeConstraint resolves a type variable to its base constraint; other
// types come back unchanged.
func getNativeTypeConstraint(state *State, checkerType *checker.Type) *checker.Type {
	for checkerType.Flags()&checker.TypeFlagsTypeVariable != 0 {
		constraint := state.Checker.GetBaseConstraintOfType(checkerType)
		if constraint == nil || constraint == checkerType {
			return checkerType
		}
		checkerType = constraint
	}
	return checkerType
}

// getNativeOptionalType appends the optional marker, parenthesising a union or a
// function type so the `?` covers the whole type.
func getNativeOptionalType(text string) string {
	if text == "" || text == nativeTypeAny || text == nativeTypeNil || strings.HasSuffix(text, "?") {
		return text
	}
	if doesNativeHaveTopLevelOperator(text, "|") || doesNativeHaveTopLevelOperator(text, "->") {
		return "(" + text + ")"
	}
	return text + "?"
}

// doesNativeHaveTopLevelOperator reports whether op appears outside every bracket.
// `->` is an arrow, not a closing `>`.
func doesNativeHaveTopLevelOperator(text, operator string) bool {
	depth := 0
	for index := 0; index < len(text); index += 1 {
		if depth == 0 && strings.HasPrefix(text[index:], operator) {
			return true
		}
		switch text[index] {
		case '(', '[', '{', '<':
			depth++
		case ')', ']', '}':
			depth--
		case '>':
			if index == 0 || text[index-1] != '-' {
				depth--
			}
		}
	}
	return false
}

// getNativeQuotedKey escapes a property name for a quoted Luau key.
func getNativeQuotedKey(name string) string {
	return strings.NewReplacer(`\`, `\\`, `"`, `\"`, "\n", `\n`, "\r", `\r`).Replace(name)
}

// isNativeTableType accepts object types that are always plain Lua tables.
// Classes are excluded because their instances carry a metatable, which
// Luau's analyzer will not accept for a table type literal. Roblox types are
// excluded because their members are an implementation detail of the datatype,
// not fields of the Lua table (`vector`/`Vector3` are annotated by name
// instead). Types with no members (`{}`, `defined`) are excluded because they
// also admit primitives.
func isNativeTableType(state *State, checkerType *checker.Type) bool {
	// Primitives have apparent types with properties (`Number`, `Boolean`), so
	// only object types can be tables.
	if checkerType.Flags()&checker.TypeFlagsObject == 0 {
		return false
	}
	if symbol := checkerType.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsClass != 0 {
		return false
	}
	if IsRobloxType(state).Check(checkerType) || symbolIsAmbient(state, checkerType, "Object", "Function") {
		return false
	}
	if len(state.Checker.GetCallSignatures(checkerType)) > 0 || len(state.Checker.GetConstructSignatures(checkerType)) > 0 {
		return false
	}
	return len(state.Checker.GetPropertiesOfType(checkerType)) > 0 || len(state.Checker.GetIndexInfosOfType(checkerType)) > 0
}
