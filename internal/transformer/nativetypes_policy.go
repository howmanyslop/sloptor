package transformer

import (
	"slices"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// The native policy: a post-pass over a full-fidelity type tree that widens
// or drops what would hurt Luau's native code generator. Positions matter:
//
//   - a NESTED position (alias body, table member, element, key/value, function
//     type parameter or return) is never type-tagged, so it is kept in full
//     and widens to `any` when it cannot be written;
//   - a RETURN type is not tagged either, but a shape that cannot be mirrored
//     stays unannotated rather than claiming `any`;
//   - a top-level PARAMETER annotation becomes a runtime tag, so only shapes
//     Luau understands are kept (`number`, `string`, vectors, tables,
//     functions, own generics) and anything else is left unannotated.
//
// Depth counts every child slot (table members, elements, type arguments,
// function type parameters and returns, pack elements) except union,
// intersection and optional members; a table deeper than nativeTypeMaxDepth
// is `{ [any]: any }`. The policy never inspects rendered text except to
// dedupe union members, and it is idempotent, so a tree may pass through it
// twice (nativeSpelling probes a constraint with it before the parameter pass).

// nativeOrigins is what the policy knows about a tree beyond its shape: the TS
// symbol a node was spelled from, what a named alias stands for at runtime,
// and whether a bare generic belongs to the function being annotated (rule
// R1). The file's typeBuilder implements it.
type nativeOrigins interface {
	originOf(node luau.TypeNode) *ast.Symbol
	aliasKindOf(symbol *ast.Symbol) (nativeTypeKind, bool)
	ownGeneric(name string) bool
	// transferOrigin records that to was rebuilt from from, so a later pass
	// classifies it the same way.
	transferOrigin(from, to luau.TypeNode)
}

func (b *typeBuilder) originOf(node luau.TypeNode) *ast.Symbol { return b.Origins[node] }

func (b *typeBuilder) aliasKindOf(symbol *ast.Symbol) (nativeTypeKind, bool) {
	entry := b.aliases[symbol]
	if entry == nil {
		return nativeKindUnspellable, false
	}
	return entry.Kind, true
}

// ownGeneric asks the generic scope at annotation time, which is the scope the
// parameter was built in: transformFunctionBody pushes the function's
// generics before its parameters are annotated.
func (b *typeBuilder) ownGeneric(name string) bool {
	if b.state == nil {
		return true
	}
	_, own := b.state.typeGenericInScope(name)
	return own
}

func (b *typeBuilder) transferOrigin(from, to luau.TypeNode) {
	if symbol, ok := b.Origins[from]; ok {
		b.Origins[to] = symbol
	}
}

// classifyNativeType is what a node would tag: references by builtin name or
// origin (a type parameter, a class or enum, a registered alias, a Roblox
// name), singletons by their primitive, unions and optionals by their first
// member, tables and functions by shape, an overload set as a function.
// `any`, `unknown`, `never`, qualified names, type-function calls, TS
// intersections, empty tables, packs and the class-instance and enum
// spellings have no tag.
func classifyNativeType(node luau.TypeNode, origins nativeOrigins) nativeTypeKind {
	switch n := node.(type) {
	case *luau.TypeReference:
		return classifyNativeReference(n, origins)
	case *luau.TypeStringSingleton:
		return nativeKindString
	case *luau.TypeBooleanSingleton:
		return nativeKindBoolean
	case *luau.TypeOptional:
		return classifyNativeType(n.Inner, origins)
	case *luau.TypeUnion:
		if n.Members.Head != nil {
			return classifyNativeType(n.Members.Head.Value, origins)
		}
	case *luau.TypeIntersection:
		if isOverloadSet(n) {
			return nativeKindFunction
		}
	case *luau.TypeTable:
		// An enum object and an empty table admit values that are not
		// tables.
		if symbol := origins.originOf(n); symbol != nil && symbol.Flags&ast.SymbolFlagsEnum != 0 {
			return nativeKindUnspellable
		}
		if n.Fields.IsNonEmpty() {
			return nativeKindTable
		}
	case *luau.TypeArray:
		return nativeKindTable
	case *luau.TypeFunction:
		return nativeKindFunction
	case *luau.TypePack:
		if n.Elements.IsEmpty() {
			return nativeKindNil
		}
	}
	return nativeKindUnspellable
}

func classifyNativeReference(n *luau.TypeReference, origins nativeOrigins) nativeTypeKind {
	if n.Module != "" {
		// `Enum.Material`: Luau names the enum, native codegen has no tag for
		// it.
		return nativeKindUnspellable
	}
	if symbol := origins.originOf(n); symbol != nil {
		switch {
		case symbol.Flags&ast.SymbolFlagsTypeParameter != 0:
			return nativeKindGeneric
		case symbol.Flags&(ast.SymbolFlagsClass|ast.SymbolFlagsEnum) != 0:
			// A class instance carries a metatable and an enum is a union of
			// members: neither is a table type.
			return nativeKindUnspellable
		}
		if kind, ok := origins.aliasKindOf(symbol); ok {
			return kind
		}
		if _, extra := luauRobloxExtraTypeNames[n.Name]; extra {
			return nativeKindUnspellable
		}
		// A Roblox datatype or Instance subclass by name.
		return nativeKindVector
	}
	switch n.Name {
	case nativeTypeNumber:
		return nativeKindNumber
	case nativeTypeString:
		return nativeKindString
	case nativeTypeBoolean:
		return nativeKindBoolean
	case nativeTypeNil:
		return nativeKindNil
	}
	if isNativeVectorTypeName(n.Name) {
		return nativeKindVector
	}
	// `any`, `unknown`, `never`, and a type-function call (`keyof<T>`).
	return nativeKindUnspellable
}

// isOverloadSet reports an intersection of function types, which is how the
// builder spells an overloaded callable. A TS intersection of named types or
// tables has no native spelling.
func isOverloadSet(n *luau.TypeIntersection) bool {
	overloads := true
	n.Members.ForEach(func(member luau.TypeNode) {
		if _, ok := member.(*luau.TypeFunction); !ok {
			overloads = false
		}
	})
	return overloads
}

// unspellableNative is what a position emits for a type it cannot mirror.
func unspellableNative(position nativePosition) luau.TypeNode {
	if position == nativePositionNested {
		return anyType()
	}
	return nil
}

// applyNativePolicy widens or drops node for the given position and depth.
// It returns the node to emit (nil when a parameter or return must stay
// unannotated), its kind, and whether a bare outer generic was met in a
// tagged position (rule R1). Nodes that need no change are returned as they
// are, so origins stay attached.
func applyNativePolicy(node luau.TypeNode, origins nativeOrigins, position nativePosition, depth int) (luau.TypeNode, nativeTypeKind, bool) {
	// Packs have no tag but are the shape of a return.
	switch n := node.(type) {
	case *luau.TypePack:
		elements, changed := applyNativeListPolicy(n.Elements, origins, depth+1)
		kind := classifyNativeType(n, origins)
		if !changed {
			return n, kind, false
		}
		return luau.NewTypePack(elements), kind, false
	case *luau.TypeVariadic:
		if position == nativePositionReturn {
			// `...any` as a declared return: the parameters stay exact and
			// the analyzer infers a better pack from the body.
			return nil, nativeKindUnspellable, false
		}
		inner, _, _ := applyNativePolicy(n.Inner, origins, nativePositionNested, depth)
		if inner == n.Inner {
			return n, nativeKindUnspellable, false
		}
		return luau.NewTypeVariadic(inner), nativeKindUnspellable, false
	case *luau.TypeGenericPack:
		// Only legal where a pack is, and a pack is never widened.
		return n, nativeKindUnspellable, false
	}

	kind := classifyNativeType(node, origins)
	if kind == nativeKindUnspellable {
		return unspellableNative(position), kind, false
	}
	switch n := node.(type) {
	case *luau.TypeReference:
		return applyNativeReferencePolicy(n, kind, origins, position, depth)
	case *luau.TypeBooleanSingleton:
		if position == nativePositionParameter {
			// Booleans are tagged correctly already; the annotation would
			// only add an entry guard.
			return nil, kind, false
		}
	case *luau.TypeOptional:
		inner, innerKind, outer := applyNativePolicy(n.Inner, origins, position, depth)
		switch inner {
		case nil:
			return nil, innerKind, outer
		case n.Inner:
			return n, innerKind, outer
		}
		return luau.OptionalType(inner), innerKind, outer
	case *luau.TypeUnion:
		return applyNativeUnionPolicy(n, origins, position, depth)
	case *luau.TypeIntersection:
		return applyNativeOverloadPolicy(n, origins, position, depth)
	case *luau.TypeTable:
		return applyNativeTablePolicy(n, origins, depth)
	case *luau.TypeArray:
		if depth > nativeTypeMaxDepth {
			return anyTable(), kind, false
		}
		element, _, _ := applyNativePolicy(n.Element, origins, nativePositionNested, depth+1)
		if element == n.Element && n.Access == luau.TypeAccessNone {
			return n, kind, false
		}
		return luau.NewTypeArray(element), kind, false
	case *luau.TypeFunction:
		return applyNativeFunctionPolicy(n, origins, depth)
	}
	return node, kind, false
}

// applyNativeReferencePolicy keeps the references Luau tags usefully. A
// parameter drops `boolean`, `nil`, a Roblox name other than the vectors, and
// an outer function's generic (R1); a return spells `nil` as `()`; the type
// arguments of an alias reference are nested slots.
func applyNativeReferencePolicy(n *luau.TypeReference, kind nativeTypeKind, origins nativeOrigins, position nativePosition, depth int) (luau.TypeNode, nativeTypeKind, bool) {
	switch kind {
	case nativeKindNil:
		switch position {
		case nativePositionReturn:
			return luau.NewTypePack(luau.NewList[luau.TypeNode]()), kind, false
		case nativePositionParameter:
			return nil, nativeKindUnspellable, false
		}
		return n, kind, false
	case nativeKindBoolean:
		if position == nativePositionParameter {
			return nil, kind, false
		}
		return n, kind, false
	case nativeKindGeneric:
		if position == nativePositionParameter && !origins.ownGeneric(n.Name) {
			return nil, kind, true
		}
		return n, kind, false
	case nativeKindVector:
		if position == nativePositionParameter && !isNativeVectorTypeName(n.Name) {
			// An Instance tag is userdata, not a speedup.
			return nil, nativeKindUnspellable, false
		}
		return n, kind, false
	}
	if n.Arguments == nil || n.Arguments.IsEmpty() {
		return n, kind, false
	}
	arguments, changed := applyNativeListPolicy(n.Arguments, origins, depth+1)
	if !changed {
		return n, kind, false
	}
	reference := luau.NewTypeReference(n.Name, arguments)
	origins.transferOrigin(n, reference)
	return reference, kind, false
}

// applyNativeUnionPolicy mirrors every member at the union's position. A
// member that cannot be tagged disqualifies a tagged union; a nested member
// that cannot be written widens the union to `any`. Luau tags every table
// member of a union as TABLE, both `vector` and `Vector3` as VECTOR, and every
// string singleton (like `string` itself) as STRING, but a parameter union
// that mixes kinds has no single useful tag and is dropped.
func applyNativeUnionPolicy(n *luau.TypeUnion, origins nativeOrigins, position nativePosition, depth int) (luau.TypeNode, nativeTypeKind, bool) {
	type member struct {
		node luau.TypeNode
		kind nativeTypeKind
		text string
	}
	var members []member
	outer, changed := false, false
	for current := n.Members.Head; current != nil; current = current.Next {
		node, kind, memberOuter := applyNativePolicy(current.Value, origins, position, depth)
		outer = outer || memberOuter
		if node == nil {
			return nil, nativeKindUnspellable, outer
		}
		if isAnyType(node) {
			return node, nativeKindUnspellable, outer
		}
		changed = changed || node != current.Value
		text := renderTypeText(node)
		if slices.ContainsFunc(members, func(existing member) bool { return existing.text == text }) {
			changed = true
			continue
		}
		members = append(members, member{node: node, kind: kind, text: text})
	}
	if len(members) == 0 {
		return unspellableNative(position), nativeKindUnspellable, outer
	}
	if len(members) == 1 {
		return members[0].node, members[0].kind, outer
	}
	if position == nativePositionParameter {
		for _, existing := range members {
			if existing.kind != members[0].kind ||
				(existing.kind != nativeKindTable && existing.kind != nativeKindVector && existing.kind != nativeKindString) {
				return nil, nativeKindUnspellable, outer
			}
		}
	}
	if !changed {
		return n, members[0].kind, outer
	}
	list := luau.NewList[luau.TypeNode]()
	for _, existing := range members {
		list.Push(existing.node)
	}
	return luau.NewTypeUnion(list), members[0].kind, outer
}

// applyNativeOverloadPolicy keeps an overload set everywhere but at a tagged
// parameter, where an intersection is tagged `any` and is stricter than TS
// while `(...any) -> ...any` keeps a real `function` tag.
func applyNativeOverloadPolicy(n *luau.TypeIntersection, origins nativeOrigins, position nativePosition, depth int) (luau.TypeNode, nativeTypeKind, bool) {
	if position == nativePositionParameter {
		return anyFunction(), nativeKindFunction, false
	}
	members, changed := applyNativeListPolicy(n.Members, origins, depth)
	if !changed {
		return n, nativeKindFunction, false
	}
	return luau.NewTypeIntersection(members), nativeKindFunction, false
}

// applyNativeTablePolicy mirrors a table's members as nested slots with
// `read`/`write` dropped (not legal on a native-annotated table and no tag
// either way). A table deeper than nativeTypeMaxDepth is still a table, just
// not spelled out.
func applyNativeTablePolicy(n *luau.TypeTable, origins nativeOrigins, depth int) (luau.TypeNode, nativeTypeKind, bool) {
	if depth > nativeTypeMaxDepth {
		return anyTable(), nativeKindTable, false
	}
	fields := luau.NewList[luau.TypeTableField]()
	changed := false
	n.Fields.ForEach(func(field luau.TypeTableField) {
		switch f := field.(type) {
		case *luau.TypeTableProp:
			value, _, _ := applyNativePolicy(f.Value, origins, nativePositionNested, depth+1)
			if value == f.Value && f.Access == luau.TypeAccessNone {
				fields.Push(f)
				return
			}
			changed = true
			fields.Push(luau.NewTypeTableProp(luau.TypeAccessNone, f.Name, value))
		case *luau.TypeTableIndexer:
			key, _, _ := applyNativePolicy(f.Key, origins, nativePositionNested, depth+1)
			value, _, _ := applyNativePolicy(f.Value, origins, nativePositionNested, depth+1)
			if key == f.Key && value == f.Value && f.Access == luau.TypeAccessNone {
				fields.Push(f)
				return
			}
			changed = true
			fields.Push(luau.NewTypeTableIndexer(luau.TypeAccessNone, key, value))
		}
	})
	if !changed {
		return n, nativeKindTable, false
	}
	table := luau.NewTypeTable(fields)
	origins.transferOrigin(n, table)
	return table, nativeKindTable, false
}

// applyNativeFunctionPolicy mirrors a function type's parameters, varargs and
// return as nested slots. The receiver of a method (`self`) is `any`: the
// receiver's own type is a class instance or an alias the table literal
// cannot name usefully, and an exact receiver would only add a guard.
func applyNativeFunctionPolicy(n *luau.TypeFunction, origins nativeOrigins, depth int) (luau.TypeNode, nativeTypeKind, bool) {
	changed := false
	parameters := make([]luau.TypeFunctionParameter, len(n.Parameters))
	for index, parameter := range n.Parameters {
		if index == 0 && parameter.Name == "self" {
			parameters[index] = parameter
			if !isAnyType(parameter.Type) {
				changed = true
				parameters[index].Type = anyType()
			}
			continue
		}
		parameterType, _, _ := applyNativePolicy(parameter.Type, origins, nativePositionNested, depth+1)
		changed = changed || parameterType != parameter.Type
		parameters[index] = luau.TypeFunctionParameter{Name: parameter.Name, Type: parameterType}
	}
	varArgs := n.VarArgs
	if varArgs != nil {
		varArgs, _, _ = applyNativePolicy(varArgs, origins, nativePositionNested, depth+1)
		changed = changed || varArgs != n.VarArgs
	}
	returnType, _, _ := applyNativePolicy(n.Return, origins, nativePositionNested, depth+1)
	changed = changed || returnType != n.Return
	if !changed {
		return n, nativeKindFunction, false
	}
	return luau.NewTypeFunction(n.Generics, parameters, varArgs, returnType), nativeKindFunction, false
}

// applyNativeListPolicy mirrors every element of a list at a nested slot of
// the given depth, reporting whether any element changed.
func applyNativeListPolicy(list *luau.List[luau.TypeNode], origins nativeOrigins, depth int) (*luau.List[luau.TypeNode], bool) {
	result := luau.NewList[luau.TypeNode]()
	changed := false
	list.ForEach(func(element luau.TypeNode) {
		node, _, _ := applyNativePolicy(element, origins, nativePositionNested, depth)
		changed = changed || node != element
		result.Push(node)
	})
	return result, changed
}
