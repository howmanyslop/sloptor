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
		if kinds[index] == nativeKindFunction || doesNativeHaveTopLevelOperator(text, "&") {
			// A function type needs parentheses before `|`, and mixing `|`
			// and `&` unparenthesised is a syntax error.
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
	name, own, ok := getNativeTypeParameterName(state, checkerType)
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
		// A value with construct signatures is a class factory: a plain table
		// at runtime (`new` plus the statics). Class INSTANCES stay excluded
		// by isNativeTableType.
		text := getNativeTableTypeAt(state, checkerType, depth, seen)
		if text == "" {
			text = nativeTypeAnyTable
		}
		return text, nativeKindTable, false
	}
	if len(state.Checker.GetCallSignatures(checkerType)) > 0 {
		text, kind, _ := getNativeFunctionType(state, checkerType, false, nativePosition, depth, seen)
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

// getNativeFunctionType mirrors a callable object as a Luau function type. An
// overload set becomes an intersection of function types (`((A) -> B) & ((C) ->
// D)`), which is what Luau's own tests and real projects use; only at a TAGGED
// parameter position does the set fall back, because an intersection is tagged
// `any` and is stricter than TS, while `(...any) -> ...any` keeps a real
// `function` tag. receiver marks an interface METHOD member, whose Luau call
// form (`obj:method()`) passes the object as an implicit first argument.
func getNativeFunctionType(state *State, checkerType *checker.Type, receiver bool, nativePosition nativePosition, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	signatures := state.Checker.GetCallSignatures(checkerType)
	switch {
	case len(signatures) == 1:
		return getNativeSignatureType(state, signatures[0], receiver, depth, seen), nativeKindFunction, false
	case len(signatures) == 0 || nativePosition == nativePositionParameter:
		return nativeTypeAnyFunction, nativeKindFunction, false
	}
	parts := make([]string, len(signatures))
	for index, signature := range signatures {
		parts[index] = "(" + getNativeSignatureType(state, signature, receiver, depth, seen) + ")"
	}
	return strings.Join(parts, " & "), nativeKindFunction, false
}

// getNativeSignatureType spells one call or construct signature. Its own type
// parameters are declared and put in scope, so `T` resolves by name inside the
// parameter and return types; constraints are dropped.
func getNativeSignatureType(state *State, signature *checker.Signature, receiver bool, depth int, seen map[*checker.Type]bool) string {
	generics := getNativeSignatureGenerics(signature)
	state.PushNativeGenerics(generics, false)
	defer state.PopNativeGenerics()

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
			// A rest parameter Luau cannot unwrap still keeps the exact
			// parameters before it.
			text := nativeTypeAny
			if element := state.Checker.GetElementTypeOfArrayType(parameterType); element != nil {
				if rendered, _, _ := getNativeType(state, element, nativePositionNested, depth+1, seen); rendered != "" {
					text = rendered
				}
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

	result := getNativeReturnTypeOfSignature(state, state.Checker.GetReturnTypeOfSignature(signature), nativePositionNested, depth+1, seen)
	if result == "" {
		result = nativeTypeAny
	}
	return luau.GenericsString(generics) + "(" + strings.Join(parts, ", ") + ") -> " + result
}

// getNativeReturnTypeOfSignature spells what follows `->`. A LuaTuple becomes a
// Luau return pack (`(A, B)`, `(A, B?)`, `(A, ...C)`, `()`); everything else is
// mirrored as one type. "" means the return cannot be written.
func getNativeReturnTypeOfSignature(state *State, checkerType *checker.Type, nativePosition nativePosition, depth int, seen map[*checker.Type]bool) string {
	if checkerType == nil {
		return ""
	}
	if isUndefinedTypeCheck(checkerType) {
		return nativeTypeUnit
	}
	if isNativeLuaTupleReturn(state, checkerType) {
		tuple := getNativeLuaTupleTuple(state, checkerType)
		if tuple == nil {
			// LuaTuple<Array<…>>, LuaTuple<T>, a union of tuples, or a
			// `LuaTuple | undefined`: keep the parameters, lose the pack.
			// `?` after a pack means "optional function", never "optional
			// return", so an optional LuaTuple has no pack spelling.
			return nativeTypeAnyPack
		}
		return getNativeReturnPack(state, tuple, depth, seen)
	}
	text, _, _ := getNativeType(state, checkerType, nativePosition, depth, seen)
	if text == "" {
		return ""
	}
	// Inside a function type an operator needs parentheses; a declared return
	// is the whole annotation, so it does not.
	if nativePosition != nativePositionReturn &&
		(doesNativeHaveTopLevelOperator(text, "|") || doesNativeHaveTopLevelOperator(text, "&")) {
		return "(" + text + ")"
	}
	return text
}

// isNativeLuaTupleReturn reports whether a return type is a LuaTuple, or a union
// with one as a member (`LuaTuple<[A, B]> | undefined`).
func isNativeLuaTupleReturn(state *State, checkerType *checker.Type) bool {
	if IsLuaTupleType(state).Check(checkerType) {
		return true
	}
	return checkerType.IsUnion() && slices.ContainsFunc(checkerType.Types(), IsLuaTupleType(state).Check)
}

// getNativeLuaTupleTuple is the tuple a LuaTuple<T> carries, or nil when T is
// not one fixed tuple.
func getNativeLuaTupleTuple(state *State, checkerType *checker.Type) *checker.Type {
	if alias := checkerType.Alias(); alias != nil {
		arguments := alias.TypeArguments()
		if len(arguments) == 1 && arguments[0].IsTupleType() {
			return arguments[0]
		}
	}
	if checkerType.IsUnion() || checkerType.IsIntersection() {
		for _, member := range checkerType.Types() {
			if member.IsTupleType() {
				return member
			}
		}
	}
	return nil
}

// getNativeReturnPack spells a tuple as the return pack of a function type. A
// rest element that is not last has no Luau spelling, and an element Luau
// cannot mirror widens to `any` without giving up the pack.
func getNativeReturnPack(state *State, tuple *checker.Type, depth int, seen map[*checker.Type]bool) string {
	infos := tuple.TargetTupleType().ElementInfos()
	arguments := getTypeArguments(state, tuple)
	if len(infos) == 0 {
		return "()"
	}
	if len(arguments) != len(infos) {
		return nativeTypeAnyPack
	}
	parts := make([]string, len(infos))
	for index, info := range infos {
		elementType := arguments[index]
		flags := info.TupleElementFlags()
		if flags&(checker.ElementFlagsRest|checker.ElementFlagsVariadic) != 0 {
			if index != len(infos)-1 {
				return nativeTypeAnyPack
			}
			// A rest element's type argument is already its element type in a
			// resolved tuple; an unresolved array still unwraps.
			if element := state.Checker.GetElementTypeOfArrayType(elementType); element != nil {
				elementType = element
			}
			text, _, _ := getNativeType(state, elementType, nativePositionNested, depth+1, seen)
			if text == "" {
				text = nativeTypeAny
			}
			// `...number | string` parses, but the parentheses read better.
			if doesNativeHaveTopLevelOperator(text, "|") {
				text = "(" + text + ")"
			}
			parts[index] = "..." + text
			continue
		}
		text, _, _ := getNativeType(state, elementType, nativePositionNested, depth+1, seen)
		if text == "" {
			text = nativeTypeAny
		}
		if flags&checker.ElementFlagsOptional != 0 {
			text = getNativeOptionalType(text)
		}
		parts[index] = text
	}
	return "(" + strings.Join(parts, ", ") + ")"
}

// getNativeConstructMember spells the `new` member of a value with construct
// signatures. An overloaded constructor keeps the intersection on the MEMBER,
// which is what preserves the table tag; `{…} & {…}` would not.
func getNativeConstructMember(state *State, checkerType *checker.Type, depth int, seen map[*checker.Type]bool) string {
	signatures := state.Checker.GetConstructSignatures(checkerType)
	switch len(signatures) {
	case 0:
		return ""
	case 1:
		return "new: " + getNativeSignatureType(state, signatures[0], false, depth, seen)
	}
	parts := make([]string, len(signatures))
	for index, signature := range signatures {
		parts[index] = "(" + getNativeSignatureType(state, signature, false, depth, seen) + ")"
	}
	return "new: " + strings.Join(parts, " & ")
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
	// A value with construct signatures is a class factory: its members are
	// statics, called with `.`, not methods that take a receiver.
	receivers := len(state.Checker.GetConstructSignatures(checkerType)) == 0
	if construct := getNativeConstructMember(state, checkerType, depth, seen); construct != "" {
		parts = append(parts, construct)
	}
	parts = append(parts, getNativePropertyMembers(state, checkerType, depth, seen, receivers)...)
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

// getNativePropertyMembers spells every declared member of checkerType as a
// table member. receivers makes a method member declare the `self` parameter an
// `obj:method()` call passes; a class's statics are called with `.` instead.
func getNativePropertyMembers(state *State, checkerType *checker.Type, depth int, seen map[*checker.Type]bool, receivers bool) []string {
	var parts []string
	for _, property := range state.Checker.GetPropertiesOfType(checkerType) {
		name := property.Name
		if strings.HasPrefix(name, "__@") {
			// Computed/ES-symbol member: no Luau spelling.
			continue
		}
		value := getNativePropertyType(state, property, receivers, depth, seen)
		if property.Flags&ast.SymbolFlagsOptional != 0 {
			value = getNativeOptionalType(value)
		}
		if luau.IsValidIdentifier(name) {
			parts = append(parts, name+": "+value)
		} else {
			parts = append(parts, `["`+getNativeQuotedKey(name)+`"]: `+value)
		}
	}
	return parts
}

// getNativePropertyType mirrors one table member. A METHOD member (`run(): T`)
// takes its receiver as the implicit first argument, so its Luau function type
// spells that parameter; a function-typed property does not.
func getNativePropertyType(state *State, property *ast.Symbol, receivers bool, depth int, seen map[*checker.Type]bool) string {
	propertyType := state.Checker.GetTypeOfSymbol(property)
	if receivers && property.Flags&ast.SymbolFlagsMethod != 0 && len(state.Checker.GetCallSignatures(propertyType)) > 0 {
		if text, kind, _ := getNativeFunctionType(state, propertyType, true, nativePositionNested, depth+1, seen); text != "" && kind == nativeKindFunction {
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
	if doesNativeHaveTopLevelOperator(text, "|") || doesNativeHaveTopLevelOperator(text, "&") || doesNativeHaveTopLevelOperator(text, "->") {
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
