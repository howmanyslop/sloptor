package transformer

import (
	"slices"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// nativeSpelling is the typeSpelling of a `//!native` file: the handful of
// decisions the native annotations take while the type is being built,
// because applyNativePolicy (nativetypes_policy.go) could not reverse them
// from the tree afterwards.
//
//   - A type parameter is spelled as its base constraint when the constraint
//     is itself annotatable at that site (`T extends number` is the perf type
//     `number`); the bare name is kept otherwise.
//   - An enum member keeps the primitive spelling (`string`, `number`): Luau
//     tags a string singleton as `string` anyway, and an enum type is a union
//     of members the policy would otherwise have to widen.
//   - A class value (`typeof Foo`, a construct signature) is the table
//     roblox-ts emits at runtime: `new` plus the statics, `prototype`
//     included, with no receiver on the statics.
//   - Aliases: non-class interfaces and non-generic object aliases only, named
//     after the TS symbol, hoisted or placed at their declaration by the
//     shared registry; an interface body the policy cannot spell is
//     `{ [any]: any }`, a type alias body it cannot spell unregisters the
//     alias and the type is spelled inline.
type nativeSpelling struct{}

// SpellTypeParameter spells a constrained type parameter as its base
// constraint when that constraint survives the native policy at this site:
// `<T extends number>(value: T)` annotates `value: number`. A constraint the
// position cannot tag (`boolean` at a parameter) keeps the bare name, which
// Luau tags `any` for an own generic and which rule R1 drops for an outer one.
func (nativeSpelling) SpellTypeParameter(b *typeBuilder, t *checker.Type, _ string, ctx typeContext) luau.TypeNode {
	constraint := b.state.Checker.GetBaseConstraintOfType(t)
	if constraint == nil || constraint == t || constraint.Flags()&checker.TypeFlagsAnyOrUnknown != 0 {
		return nil
	}
	built := b.Build(constraint, ctx)
	node, _, _ := applyNativePolicy(built.Node, b, nativePositionOf(ctx.Position), 0)
	return node
}

// SpellEnumMember keeps an enum member primitive: `string` for a string
// member, `number` otherwise.
func (nativeSpelling) SpellEnumMember(_ *typeBuilder, t *checker.Type) luau.TypeNode {
	if t.Flags()&checker.TypeFlagsStringLike != 0 {
		return luau.TypeName(nativeTypeString)
	}
	return luau.TypeName(nativeTypeNumber)
}

// SpellClassValue spells a value with construct signatures as the plain table
// it is at runtime: the `new` member (an overload set keeps its intersection
// on the member, which preserves the table tag) and every static, `prototype`
// included, called with `.` and so without a receiver. A value already being
// spelled (a static typed as its own class) is `{ [any]: any }`, as is one
// with nothing to list.
func (nativeSpelling) SpellClassValue(b *typeBuilder, t *checker.Type) luau.TypeNode {
	if slices.Contains(b.stack, t) {
		return anyTable()
	}
	b.stack = append(b.stack, t)
	defer func() { b.stack = b.stack[:len(b.stack)-1] }()

	ctx := typeContext{Position: positionNested}
	fields := luau.NewList[luau.TypeTableField]()
	if construct := b.constructMember(t, ctx); construct != nil {
		fields.Push(construct)
	}
	for _, property := range b.state.Checker.GetPropertiesOfType(t) {
		if isSymbolKeyedMember(property.Name) {
			continue
		}
		value := b.Build(b.state.Checker.GetTypeOfSymbol(property), ctx).Node
		if property.Flags&ast.SymbolFlagsOptional != 0 {
			value = luau.OptionalType(value)
		}
		fields.Push(luau.NewTypeTableProp(luau.TypeAccessNone, property.Name, value))
	}
	if indexer := b.BuildIndexer(t, ctx); indexer != nil {
		fields.Push(indexer)
	}
	if fields.IsEmpty() {
		return anyTable()
	}
	return luau.NewTypeTable(fields)
}

// ShouldAlias is the native alias policy (isNativeAliasableSymbol).
func (nativeSpelling) ShouldAlias(state *State, symbol *ast.Symbol, declaration *ast.Node) bool {
	return isNativeAliasableSymbol(state, symbol, declaration)
}

// AliasName is the TS name; the registry makes it unique (`Node_2`) and
// steers it clear of the names Luau reads as something else (`table_2`).
func (nativeSpelling) AliasName(_ *State, symbol *ast.Symbol) string {
	return symbol.Name
}

// FinishAliasBody runs the native policy over the body (a nested position:
// alias bodies are never tagged, and depth counting restarts at the alias)
// and drops the generic defaults, which the native annotations never spell.
// An interface is a table however little of it can be written, so an
// unspellable body becomes `{ [any]: any }`; a type alias is kept only when
// its body came out as a table or a function type, and is otherwise
// unregistered so references spell the type inline.
func (nativeSpelling) FinishAliasBody(b *typeBuilder, entry *typeAliasEntry, body luau.TypeNode) (luau.TypeNode, bool) {
	node, kind, _ := applyNativePolicy(body, b, nativePositionNested, 0)
	for index := range entry.Statement.Generics {
		entry.Statement.Generics[index].Default = nil
	}
	if entry.Symbol.Flags&ast.SymbolFlagsInterface != 0 {
		if kind != nativeKindTable {
			node = anyTable()
		}
		entry.Kind = nativeKindTable
		return node, true
	}
	if kind != nativeKindTable && kind != nativeKindFunction {
		return nil, false
	}
	entry.Kind = kind
	return node, true
}
