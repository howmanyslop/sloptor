package transformer

import (
	"slices"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/scanner"
)

// Native-codegen type annotations. Under `--!native` Luau's compiler turns
// parameter annotations into bytecode type tags (Compiler/src/Types.cpp), and
// the native code generator uses them to drop tag checks and fallback paths.
// Only `number`, `vector`/`Vector3` and table types buy a real speedup;
// `string` is a marginal win. Anything else is left unannotated, because every
// annotated parameter adds an entry guard and an unrecognised name is tagged
// as userdata.
//
// Tables must be spelled as a table type literal. A bare `table` is not a
// primitive for that compiler, so it would be tagged userdata and the entry
// guard would fail on every call. Arrays, tuples, Map, Set, Records and object
// types are therefore spelled structurally, and only a shape that cannot be
// written out (a self-referential one, or one nested too deeply) falls back to
// `{ [any]: any }`.
const (
	nativeTypeNumber   = "number"
	nativeTypeString   = "string"
	nativeTypeBoolean  = "boolean"
	nativeTypeAny      = "any"
	nativeTypeAnyTable = "{ [any]: any }"

	// nativeTypeMaxDepth bounds how far nested table shapes are followed.
	nativeTypeMaxDepth = 4
)

// hasNativeDirective reports whether the file's leading comments contain
// `//!native`. Luau honours the hot comment anywhere before the first token,
// so every leading comment is considered.
func hasNativeDirective(s *State) bool {
	if s.RemoveComments() {
		return false
	}
	var factory ast.NodeFactory
	for commentRange := range scanner.GetLeadingCommentRanges(&factory, s.SourceFileText, 0) {
		end := commentRange.End()
		if commentRange.Kind != ast.KindSingleLineCommentTrivia {
			end -= 2
		}
		if strings.TrimSpace(s.SourceFileText[commentRange.Pos()+2:end]) == "!native" {
			return true
		}
	}
	return false
}

// nativeLuauType maps a TS type to the Luau annotation worth emitting for it,
// or nil when the value should stay unannotated. `undefined` members become
// the optional marker; table members are spelled structurally; every other
// member must agree on one primitive Luau type. `optional` marks a parameter
// that callers may omit, so a union knows it needs parentheses before the `?`.
func nativeLuauType(s *State, t *checker.Type, optional bool) *luau.Type {
	t = nativeTypeConstraint(s, t)
	members := []*checker.Type{t}
	if t.IsUnion() {
		members = t.Types()
	}

	name := ""
	var tables []string
	for _, member := range members {
		if isUndefinedTypeCheck(member) {
			optional = true
			continue
		}
		// Primitives first: their apparent types (`Number`, `String`) have
		// properties, so a table check would claim them.
		if memberName := nativeScalarTypeName(s, member); memberName != "" {
			if name != "" && name != memberName {
				return nil
			}
			name = memberName
			continue
		}
		if !isNativeTableType(s, nativeTypeConstraint(s, member)) {
			return nil
		}
		table := nativeTableType(s, member)
		if table == "" {
			table = nativeTypeAnyTable
		}
		if !slices.Contains(tables, table) {
			tables = append(tables, table)
		}
	}
	if len(tables) > 0 {
		// Luau tags every table member of the union as TABLE, but a union that
		// mixes a table with anything else has no single useful tag.
		if name != "" {
			return nil
		}
		name = strings.Join(tables, " | ")
		if optional && len(tables) > 1 {
			name = "(" + name + ")"
		}
	}
	if name == "" {
		return nil
	}
	return &luau.Type{Name: name, Optional: optional}
}

// nativeTypeConstraint resolves a type variable to its base constraint; other
// types come back unchanged.
func nativeTypeConstraint(s *State, t *checker.Type) *checker.Type {
	for t.Flags()&checker.TypeFlagsTypeVariable != 0 {
		constraint := s.Checker.GetBaseConstraintOfType(t)
		if constraint == nil || constraint == t {
			return t
		}
		t = constraint
	}
	return t
}

// nativeScalarTypeName is the annotation for a non-table type: a number, a
// string, or one of the two Roblox vector types. Everything else is "".
func nativeScalarTypeName(s *State, t *checker.Type) string {
	t = nativeTypeConstraint(s, t)
	switch {
	case isNumberTypeCheck(t):
		return nativeTypeNumber
	case isStringTypeCheck(t):
		return nativeTypeString
	case t.Flags()&checker.TypeFlagsObject == 0:
		return ""
	case IsRobloxType(s).Check(t):
		if symbol := t.Symbol(); symbol != nil && (symbol.Name == "vector" || symbol.Name == "Vector3") {
			return symbol.Name
		}
		return ""
	}
	return ""
}

// nativeElementType is the Luau type for one nested value: an array element, a
// tuple member, a map key or value, or a table property. A type with no useful
// spelling widens to `any`, which the surrounding table literal still accepts.
func nativeElementType(s *State, t *checker.Type, depth int, seen map[*checker.Type]bool) string {
	if depth > nativeTypeMaxDepth {
		return nativeTypeAny
	}
	t = nativeTypeConstraint(s, t)
	if t.IsUnion() {
		optional := false
		var names []string
		for _, member := range t.Types() {
			if isUndefinedTypeCheck(member) {
				optional = true
				continue
			}
			name := nativeElementType(s, member, depth, seen)
			// `any` already covers the other members, and `any?` reads badly.
			if name == nativeTypeAny {
				return name
			}
			if !slices.Contains(names, name) {
				names = append(names, name)
			}
		}
		if len(names) == 0 {
			return nativeTypeAny
		}
		name := strings.Join(names, " | ")
		if optional {
			return nativeOptionalType(name)
		}
		return name
	}
	symbol := t.Symbol()
	switch {
	case isNumberTypeCheck(t):
		return nativeTypeNumber
	case isStringTypeCheck(t):
		return nativeTypeString
	case isBooleanTypeCheck(t):
		return nativeTypeBoolean
	case t.Flags()&checker.TypeFlagsObject == 0:
		return nativeTypeAny
	case IsRobloxType(s).Check(t):
		if symbol != nil && (symbol.Name == "vector" || symbol.Name == "Vector3") {
			return symbol.Name
		}
		return nativeTypeAny
	case symbol != nil && symbol.Flags&ast.SymbolFlagsEnum != 0:
		return nativeTypeAny
	case isNativeTableType(s, t):
		if table := nativeTableTypeAt(s, t, depth, seen); table != "" {
			return table
		}
		// Recursive or too deeply nested: still a table, just not spellable.
		return nativeTypeAnyTable
	}
	return nativeTypeAny
}

// nativeTableType spells a table type as a Luau table type literal, or "" when
// its shape cannot be written out.
func nativeTableType(s *State, t *checker.Type) string {
	return nativeTableTypeAt(s, nativeTypeConstraint(s, t), 0, map[*checker.Type]bool{})
}

// nativeTableTypeAt spells one table level. `depth` bounds nesting and `seen`
// stops a self-referential interface from recursing forever; either guard
// yields "" so the caller can widen to `any`.
func nativeTableTypeAt(s *State, t *checker.Type, depth int, seen map[*checker.Type]bool) string {
	if depth > nativeTypeMaxDepth || seen[t] {
		return ""
	}
	seen[t] = true
	defer delete(seen, t)

	switch {
	case t.IsTupleType():
		var names []string
		for _, element := range getTypeArguments(s, t) {
			name := nativeElementType(s, element, depth+1, seen)
			if !slices.Contains(names, name) {
				names = append(names, name)
			}
		}
		if len(names) == 0 {
			return ""
		}
		return "{ " + strings.Join(names, " | ") + " }"
	case IsArrayType(s).Check(t):
		element := s.Checker.GetElementTypeOfArrayType(t)
		if element == nil {
			return ""
		}
		return "{ " + nativeElementType(s, element, depth+1, seen) + " }"
	case IsMapType(s).Check(t):
		arguments := getTypeArguments(s, t)
		if len(arguments) < 2 {
			return ""
		}
		key := nativeElementType(s, arguments[0], depth+1, seen)
		value := nativeElementType(s, arguments[1], depth+1, seen)
		return "{ [" + key + "]: " + value + " }"
	case IsSetType(s).Check(t):
		arguments := getTypeArguments(s, t)
		if len(arguments) < 1 {
			return ""
		}
		return "{ [" + nativeElementType(s, arguments[0], depth+1, seen) + "]: true }"
	}

	var parts []string
	for _, property := range s.Checker.GetPropertiesOfType(t) {
		name := property.Name
		if strings.HasPrefix(name, "__@") {
			// Computed/ES-symbol member: no Luau spelling.
			continue
		}
		value := nativeElementType(s, s.Checker.GetTypeOfSymbol(property), depth+1, seen)
		if property.Flags&ast.SymbolFlagsOptional != 0 {
			value = nativeOptionalType(value)
		}
		if luau.IsValidIdentifier(name) {
			parts = append(parts, name+": "+value)
		} else {
			parts = append(parts, `["`+nativeQuotedKey(name)+`"]: `+value)
		}
	}
	// Luau allows at most one table indexer, so two TS index signatures have to
	// collapse into one. When their value types differ the property is still a
	// table, so the imprecise `any` value stays correct.
	stringIndex := s.Checker.GetStringIndexType(t)
	numberIndex := s.Checker.GetNumberIndexType(t)
	switch {
	case stringIndex != nil && numberIndex != nil:
		stringValue := nativeElementType(s, stringIndex, depth+1, seen)
		numberValue := nativeElementType(s, numberIndex, depth+1, seen)
		value := nativeTypeAny
		if stringValue == numberValue {
			value = stringValue
		}
		parts = append(parts, "["+nativeTypeAny+"]: "+value)
	case stringIndex != nil:
		parts = append(parts, "["+nativeTypeString+"]: "+nativeElementType(s, stringIndex, depth+1, seen))
	case numberIndex != nil:
		parts = append(parts, "["+nativeTypeNumber+"]: "+nativeElementType(s, numberIndex, depth+1, seen))
	}
	if len(parts) == 0 {
		return ""
	}
	return "{ " + strings.Join(parts, ", ") + " }"
}

// nativeOptionalType appends the optional marker, parenthesising a union so
// `A | B` does not become `A | B?`.
func nativeOptionalType(name string) string {
	if name == nativeTypeAny || strings.HasSuffix(name, "?") {
		return name
	}
	if strings.Contains(name, " | ") {
		return "(" + name + ")?"
	}
	return name + "?"
}

// nativeQuotedKey escapes a property name for a quoted Luau key.
func nativeQuotedKey(name string) string {
	return strings.NewReplacer(`\`, `\\`, `"`, `\"`, "\n", `\n`, "\r", `\r`).Replace(name)
}

// isNativeTableType accepts object types that are always plain Lua tables.
// Classes are excluded because their instances carry a metatable, which
// Luau's analyzer will not accept for a table type literal. Roblox types are
// excluded because their members are an implementation detail of the datatype,
// not fields of the Lua table (`vector`/`Vector3` are annotated by name
// instead). Types with no members (`{}`, `defined`) are excluded because they
// also admit primitives.
func isNativeTableType(s *State, t *checker.Type) bool {
	// Primitives have apparent types with properties (`Number`, `Boolean`), so
	// only object types can be tables.
	if t.Flags()&checker.TypeFlagsObject == 0 {
		return false
	}
	if symbol := t.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsClass != 0 {
		return false
	}
	if IsRobloxType(s).Check(t) || symbolIsAmbient(s, t, "Object", "Function") {
		return false
	}
	if len(s.Checker.GetCallSignatures(t)) > 0 || len(s.Checker.GetConstructSignatures(t)) > 0 {
		return false
	}
	return len(s.Checker.GetPropertiesOfType(t)) > 0 || len(s.Checker.GetIndexInfosOfType(t)) > 0
}

// nativeParameterType is the annotation for one plain-identifier parameter.
// A parameter that callers may omit (`?`, default value) is optional.
func nativeParameterType(s *State, parameterNode *ast.Node) *luau.Type {
	parameter := parameterNode.AsParameterDeclaration()
	optional := parameter.QuestionToken != nil || parameter.Initializer != nil
	return nativeLuauType(s, s.Checker.GetTypeAtLocation(parameter.Name()), optional)
}

// nativeReturnType is the annotation for a function's return, nil outside
// `--!native` files. Async and generator functions return a wrapper, so they
// get none.
func nativeReturnType(s *State, node *ast.Node) *luau.Type {
	if !s.IsNative || ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync) || isGeneratorFunction(node) {
		return nil
	}
	signature := s.Checker.GetSignatureFromDeclaration(node)
	if signature == nil {
		return nil
	}
	return nativeLuauType(s, s.Checker.GetReturnTypeOfSignature(signature), false)
}

func isGeneratorFunction(node *ast.Node) bool {
	switch node.Kind {
	case ast.KindFunctionDeclaration:
		return node.AsFunctionDeclaration().AsteriskToken != nil
	case ast.KindFunctionExpression:
		return node.AsFunctionExpression().AsteriskToken != nil
	case ast.KindMethodDeclaration:
		return node.AsMethodDeclaration().AsteriskToken != nil
	}
	return false
}
