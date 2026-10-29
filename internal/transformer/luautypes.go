package transformer

import (
	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// The shared TS-to-Luau type builder: one mapping from checker types to the
// typed Luau AST (internal/luau/types.go), used by the experimental luauTypes
// mode at full fidelity and, through a typeSpelling, by the `//!native`
// annotations. The builder never inspects rendered text; the renderer owns
// parenthesization. Where Luau has no spelling for a TS construct the builder
// substitutes the closest type and records a fidelityLoss, so tests can assert
// what was lost and declaration-level losses can become comments.
//
// This file holds the context, loss records, spelling hooks, and the alias
// registry the builder needs; luautypes_builder.go holds the mapping itself.

// typePosition is where a type is being spelled. Luau's compiler tags only a
// function's top-level parameter and return annotations, and a return is the
// one place a type pack may appear.
type typePosition uint8

const (
	// positionNested is any untagged position: a table member, an element, a
	// key or value, a type argument, a function type's parameters and
	// return. Unspellable types widen to `any` here.
	positionNested typePosition = iota
	// positionParameter is a function's top-level parameter annotation.
	positionParameter
	// positionReturn is a function's declared return, the only position where
	// `()` and `(A, B)` packs are legal.
	positionReturn
	// positionLocal is a `local x: T` annotation.
	positionLocal
	// positionAliasBody is the top of a `type Name = T` body: the alias being
	// declared is spelled structurally there instead of by its own name.
	positionAliasBody
)

// typeContext is what the builder knows about the annotation site.
type typeContext struct {
	Position typePosition
	// Receiver is what `self` means for METHOD members of the object being
	// spelled (an interface alias reference, a class instance reference).
	// nil makes a method's `self` parameter `any` and records a loss. It
	// applies to the table at this position only: nested anonymous objects
	// have no name to refer to, so nested contexts drop it.
	Receiver luau.TypeNode
	// This is what the TS polymorphic `this` type means at this site. Unlike
	// Receiver it survives into nested positions (`clone(): this` inside a
	// method signature still means the interface).
	This luau.TypeNode
	// Owner is the declaration the annotation belongs to, for loss
	// attribution.
	Owner *ast.Node
	// ClassInstance marks the table of a class instance alias (WP7):
	// `readonly` members are spelled without `read` because constructors
	// write through `self`, and private/protected members are exposed; both
	// record one loss per table.
	ClassInstance bool
}

// nested is the context for a child slot: untagged, with no receiver of its
// own.
func (ctx typeContext) nested() typeContext {
	return typeContext{Position: positionNested, This: ctx.This, Owner: ctx.Owner}
}

// fidelityLossKind names a TS construct Luau could not spell exactly.
type fidelityLossKind uint8

const (
	lossNumberLiteral fidelityLossKind = iota
	lossTuple
	lossTemplateLiteral
	lossSymbolKey
	lossBigInt
	lossConstraintDropped
	lossIndexerCollapsed
	lossRecursiveAliasArgs
	lossLuaTupleOutsideReturn
	lossOptionalPack
	lossClassReadonlyDropped
	lossPrivateExposed
	lossObjectLiteralSelf
	lossMappedType
	lossConditionalType
	lossInferType
	lossKeyRemap
	lossThisType
	lossDepthExceeded
	lossUnspellable
)

var fidelityLossKindNames = [...]string{
	lossNumberLiteral:         "NumberLiteral",
	lossTuple:                 "Tuple",
	lossTemplateLiteral:       "TemplateLiteral",
	lossSymbolKey:             "SymbolKey",
	lossBigInt:                "BigInt",
	lossConstraintDropped:     "ConstraintDropped",
	lossIndexerCollapsed:      "IndexerCollapsed",
	lossRecursiveAliasArgs:    "RecursiveAliasArgs",
	lossLuaTupleOutsideReturn: "LuaTupleOutsideReturn",
	lossOptionalPack:          "OptionalPack",
	lossClassReadonlyDropped:  "ClassReadonlyDropped",
	lossPrivateExposed:        "PrivateExposed",
	lossObjectLiteralSelf:     "ObjectLiteralSelf",
	lossMappedType:            "MappedType",
	lossConditionalType:       "ConditionalType",
	lossInferType:             "InferType",
	lossKeyRemap:              "KeyRemap",
	lossThisType:              "ThisType",
	lossDepthExceeded:         "DepthExceeded",
	lossUnspellable:           "Unspellable",
}

func (kind fidelityLossKind) String() string {
	if int(kind) < len(fidelityLossKindNames) {
		return fidelityLossKindNames[kind]
	}
	return "Unknown"
}

// fidelityLoss records one substitution: the TS type as the checker prints it,
// the Luau text that replaced it, why, and the declaration it belongs to.
type fidelityLoss struct {
	Kind   fidelityLossKind
	TSText string
	Luau   string
	Reason string
	Node   *ast.Node
}

// builtType is the builder's answer for one type.
type builtType struct {
	// Node is never nil: `any` at worst.
	Node luau.TypeNode
	// Kind is the top-level classification the nativeTypes policy reuses:
	// what the annotation would tag, read off the built node (and, for
	// references, off Origins).
	Kind nativeTypeKind
	// OuterGeneric answers rule R1: the type names an ENCLOSING function's
	// generic in a parameter position.
	OuterGeneric bool
	// Lossy is set when building this type recorded at least one loss.
	Lossy bool
}

// typeSpelling carries the decisions a post-pass over the built tree cannot
// reverse. luauTypes uses defaultSpelling (full fidelity); nativeTypes plugs in
// its own spelling (WP3). The three Spell hooks return nil to take the
// default; the alias hooks decide which named types get a `type` statement,
// what it is called, and whether its body is kept (false unregisters the alias
// and the reference is spelled inline).
type typeSpelling interface {
	// SpellTypeParameter spells an in-scope type parameter; nil keeps the
	// bare `T` reference.
	SpellTypeParameter(b *typeBuilder, t *checker.Type, name string) luau.TypeNode
	// SpellEnumMember spells one enum member literal; nil keeps the singleton
	// (`number` for a numeric member).
	SpellEnumMember(b *typeBuilder, t *checker.Type) luau.TypeNode
	// SpellClassValue spells a class value (`typeof Foo`, a construct
	// signature); nil keeps the structural `{ new: ..., statics }` table.
	SpellClassValue(b *typeBuilder, t *checker.Type) luau.TypeNode
	ShouldAlias(state *State, symbol *ast.Symbol, declaration *ast.Node) bool
	AliasName(state *State, symbol *ast.Symbol) string
	FinishAliasBody(state *State, entry *typeAliasEntry, body luau.TypeNode) (luau.TypeNode, bool)
}

// defaultSpelling is full fidelity: every named interface, type alias and
// enum declared outside a library file gets a `type` statement under its own
// name, and nothing is widened. The alias registry (naming, exports,
// placement, imported types) is refined by WP5 through these same hooks.
type defaultSpelling struct{}

func (defaultSpelling) SpellTypeParameter(*typeBuilder, *checker.Type, string) luau.TypeNode {
	return nil
}

func (defaultSpelling) SpellEnumMember(*typeBuilder, *checker.Type) luau.TypeNode { return nil }

func (defaultSpelling) SpellClassValue(*typeBuilder, *checker.Type) luau.TypeNode { return nil }

// ShouldAlias accepts every interface, type alias and enum that has a
// declaration, library types included: a library interface (`Promise<T>`)
// spelled structurally at each use re-instantiates itself without bound, so
// it is mirrored once as an alias (WP5 names and hoists it). Two kinds are
// declined: a LuaTuple alias, because a pack is only spellable as a function
// return, and a generic alias whose body is a mapped, conditional or other
// type-level computation, whose instantiations are spelled eagerly from the
// resolved type until WP8 translates the body into a type function.
func (defaultSpelling) ShouldAlias(state *State, symbol *ast.Symbol, declaration *ast.Node) bool {
	if declaration == nil {
		return false
	}
	if symbol.Flags&ast.SymbolFlagsTypeAlias != 0 {
		declared := state.Checker.GetDeclaredTypeOfSymbol(symbol)
		if declared == nil || IsLuaTupleType(state).Check(declared) {
			return false
		}
		if len(declaredTypeParameters(declaration)) > 0 && isTypeLevelComputation(declared) {
			return false
		}
	}
	return true
}

// isTypeLevelComputation reports a type the checker evaluates per
// instantiation (a mapped type, a conditional, an indexed access, `keyof`, a
// template literal) rather than a shape.
func isTypeLevelComputation(t *checker.Type) bool {
	if t.Flags()&checker.TypeFlagsInstantiable != 0 {
		return true
	}
	return t.Flags()&checker.TypeFlagsObject != 0 && t.ObjectFlags()&checker.ObjectFlagsMapped != 0
}

// AliasName is the TS name when Luau can spell it. Uniqueness against other
// aliases, builtins and type-function names is WP5's job.
func (defaultSpelling) AliasName(_ *State, symbol *ast.Symbol) string {
	if luau.IsValidIdentifier(symbol.Name) {
		return symbol.Name
	}
	return "Alias"
}

func (defaultSpelling) FinishAliasBody(_ *State, _ *typeAliasEntry, body luau.TypeNode) (luau.TypeNode, bool) {
	return body, true
}

// typeAliasEntry is one registered `type` statement. The statement is
// registered before its body is built, which is what lets a self reference
// resolve to the name. WP5 extends the entry with export, placement and
// type-function data.
type typeAliasEntry struct {
	Symbol    *ast.Symbol
	Statement *luau.TypeAlias
	Name      string
	Generics  []luau.TypeGeneric
	// Kind is what the alias names at runtime (a table, a function, a
	// string...), read off the built body.
	Kind     nativeTypeKind
	Callable bool
	// Params are the alias's own type parameter types, so a recursive
	// reference can be checked for identical arguments.
	Params []*checker.Type
	// building is set while the body is under construction: a reference at
	// the top of the body is the alias itself and is spelled structurally.
	building bool
}

// typeBuilder maps checker types to Luau type nodes for one file.
type typeBuilder struct {
	state    *State
	spelling typeSpelling
	// stack holds the anonymous object types being spelled, for cycle
	// detection; named types recurse through their alias instead.
	stack  []*checker.Type
	losses []fidelityLoss
	// Origins maps every node spelled from a TS symbol (a type parameter,
	// class, enum, interface or alias reference, a Roblox type, and the
	// table built for an interface body, enum object or class value) to that
	// symbol, so a policy pass can classify by origin.
	Origins map[luau.TypeNode]*ast.Symbol
	// depth counts nested Build calls for the hard recursion guard.
	depth int
	// aliases is the in-file alias registry (nil entries mark symbols the
	// spelling declined); aliasOrder keeps registration order for placement.
	aliases    map[*ast.Symbol]*typeAliasEntry
	aliasOrder []*typeAliasEntry
	// constraintLosses dedupes the once-per-declaration constraint loss.
	constraintLosses map[*ast.Symbol]struct{}
}

func newTypeBuilder(state *State, spelling typeSpelling) *typeBuilder {
	return &typeBuilder{
		state:            state,
		spelling:         spelling,
		Origins:          make(map[luau.TypeNode]*ast.Symbol),
		aliases:          make(map[*ast.Symbol]*typeAliasEntry),
		constraintLosses: make(map[*ast.Symbol]struct{}),
	}
}

// spellingFor picks the spelling for a file: luauTypes and plain files use
// full fidelity. A `//!native` file without luauTypes keeps the native
// string emitter until WP3 lands nativeSpelling here.
func spellingFor(_ *State) typeSpelling {
	return defaultSpelling{}
}

// types is the file's builder, created on first use.
func (state *State) types() *typeBuilder {
	if state.luauTypeBuilder == nil {
		state.luauTypeBuilder = newTypeBuilder(state, spellingFor(state))
	}
	return state.luauTypeBuilder
}

// LuauTypeLosses lists every fidelity loss recorded while this file's types
// were built, in order.
func (state *State) LuauTypeLosses() []fidelityLoss {
	return state.types().losses
}

// typeAliasFor registers (on first use) and returns the alias for a symbol
// the spelling agreed to alias, or nil when it declined or the body was
// rejected and references are to be spelled inline.
func (state *State) typeAliasFor(symbol *ast.Symbol) *typeAliasEntry {
	return state.types().aliasFor(symbol)
}

// Aliases lists the registered alias entries in registration order.
func (b *typeBuilder) Aliases() []*typeAliasEntry {
	return b.aliasOrder
}

// lose records that t was spelled as replacement and returns the
// replacement, which must not be nil (use record when nothing stands in for
// the lost construct).
func (b *typeBuilder) lose(kind fidelityLossKind, t *checker.Type, replacement luau.TypeNode, ctx typeContext, reason string) luau.TypeNode {
	b.record(kind, b.typeText(t), replacement, ctx, reason)
	return replacement
}

// loseText is lose for a construct that has no checker type of its own (a
// member name, an index signature pair).
func (b *typeBuilder) loseText(kind fidelityLossKind, tsText string, replacement luau.TypeNode, ctx typeContext, reason string) luau.TypeNode {
	b.record(kind, tsText, replacement, ctx, reason)
	return replacement
}

// record appends one loss. replacement may be nil when the construct was
// skipped rather than replaced.
func (b *typeBuilder) record(kind fidelityLossKind, tsText string, replacement luau.TypeNode, ctx typeContext, reason string) {
	loss := fidelityLoss{Kind: kind, TSText: tsText, Reason: reason, Node: ctx.Owner}
	if replacement != nil {
		loss.Luau = renderTypeText(replacement)
	}
	b.losses = append(b.losses, loss)
}

// typeText is the checker's spelling of t, "" for no type.
func (b *typeBuilder) typeText(t *checker.Type) string {
	if t == nil {
		return ""
	}
	return b.state.Checker.TypeToString(t)
}

// renderTypeText renders a type node on one line. The builder uses it only
// for loss records and for structural deduplication of union members, never
// to decide syntax.
func renderTypeText(node luau.TypeNode) string {
	return render.Render(render.NewRenderState(), node)
}

// cloneType makes an occurrence-specific copy of a node that is reused at
// several sites (a receiver spelled into every method), so each parent adopts
// its own copy.
func cloneType(node luau.TypeNode) luau.TypeNode {
	clone, ok := luau.WithOrigin(node, node.Origin()).(luau.TypeNode)
	if !ok {
		panic("transformer: cloneType: clone is not a type node")
	}
	return clone
}

// anyType, anyTable and anyFunction are the widest spellings of a type, a
// table, and a callable.
func anyType() luau.TypeNode { return luau.TypeName(nativeTypeAny) }

func anyTable() luau.TypeNode {
	return luau.NewTypeTable(luau.NewList[luau.TypeTableField](
		luau.NewTypeTableIndexer(luau.TypeAccessNone, anyType(), anyType())))
}

func anyFunction() luau.TypeNode {
	return luau.NewTypeFunction(nil, nil, luau.NewTypeVariadic(anyType()), luau.NewTypeVariadic(anyType()))
}

// isAnyType reports a bare `any` reference.
func isAnyType(node luau.TypeNode) bool {
	reference, ok := node.(*luau.TypeReference)
	return ok && reference.Module == "" && reference.Name == nativeTypeAny &&
		(reference.Arguments == nil || reference.Arguments.IsEmpty())
}
