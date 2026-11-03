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
// `string` is a marginal win. Everything else is left unannotated, because every
// annotated parameter adds an entry guard and an unrecognised name is tagged
// as userdata.
//
// A bare generic name is only understood when it belongs to the annotated
// function itself; an outer function's generic is tagged userdata, which makes
// the native entry guard fail on every call. That rule (R1) is why a parameter
// annotation is dropped when it would contain a bare outer generic.
//
// The annotations are built in two steps: the shared TS-to-Luau builder
// (luautypes_builder.go) spells the type at full fidelity, consulting
// nativeSpelling (nativetypes_spelling.go) for the few decisions a post-pass
// cannot reverse, and applyNativePolicy (nativetypes_policy.go) then widens or
// drops what would hurt native codegen. The alias predicates live in
// nativetypes_alias.go.
const (
	nativeTypeNumber  = "number"
	nativeTypeString  = "string"
	nativeTypeBoolean = "boolean"
	nativeTypeAny     = "any"
	nativeTypeNil     = "nil"

	// nativeTypeMaxDepth bounds how far nested table shapes are followed.
	nativeTypeMaxDepth = 4
)

// nativePosition is where a type is being spelled. Only a function's top-level
// parameter and return types are type-tagged by Luau's compiler, so those
// positions refuse to spell a shape instead of widening it.
type nativePosition uint8

const (
	// nativePositionNested is any position that is not tagged: alias bodies,
	// table members, elements, keys/values, and function type parameters and
	// returns. An unspellable nested type widens to `any`.
	nativePositionNested nativePosition = iota
	// nativePositionReturn is a function's declared return type.
	nativePositionReturn
	// nativePositionParameter is a function's top-level parameter type.
	nativePositionParameter
)

// nativePositionOf maps the builder's position onto the three the native
// policy distinguishes.
func nativePositionOf(position typePosition) nativePosition {
	switch position {
	case positionParameter:
		return nativePositionParameter
	case positionReturn:
		return nativePositionReturn
	}
	return nativePositionNested
}

// nativeTypeKind classifies a spelled type so union handling can decide
// whether a top-level parameter annotation is worth emitting.
type nativeTypeKind uint8

const (
	nativeKindUnspellable nativeTypeKind = iota
	nativeKindNil
	nativeKindNumber
	nativeKindString
	nativeKindBoolean
	nativeKindVector
	nativeKindTable
	nativeKindFunction
	nativeKindGeneric
)

// hasNativeDirective reports whether the file's leading comments contain
// `//!native`. Luau honors the hot comment anywhere before the first token,
// so every leading comment is considered.
func hasNativeDirective(state *State) bool {
	if state.RemoveComments() {
		return false
	}
	var factory ast.NodeFactory
	for commentRange := range scanner.GetLeadingCommentRanges(&factory, state.SourceFileText, 0) {
		end := commentRange.End()
		if commentRange.Kind != ast.KindSingleLineCommentTrivia {
			end -= 2
		}
		if strings.TrimSpace(state.SourceFileText[commentRange.Pos()+2:end]) == "!native" {
			return true
		}
	}
	return false
}

// getNativeParameterType is the annotation for one plain-identifier parameter, or
// nil when the parameter must stay unannotated: the built type under the
// parameter policy, optional when the parameter has `?` or an initializer.
func getNativeParameterType(state *State, parameterNode *ast.Node) luau.TypeNode {
	parameter := parameterNode.AsParameterDeclaration()
	b := state.nativeTypes()
	built := b.Build(state.Checker.GetTypeAtLocation(parameter.Name()), typeContext{Position: positionParameter, Owner: parameterNode})
	node, _, _ := applyNativePolicy(built.Node, b, nativePositionParameter, 0)
	if node == nil {
		return nil
	}
	if parameter.QuestionToken != nil || parameter.Initializer != nil {
		node = luau.OptionalType(node)
	}
	return node
}

// getNativeReturnType is the annotation for a function's return, nil outside
// `--!native` files. Async and generator functions return a wrapper, so they
// get none; a return the policy cannot mirror stays unannotated rather than
// claiming `any`, because the analyzer infers a better pack from the body.
func getNativeReturnType(state *State, node *ast.Node) luau.TypeNode {
	if !state.IsNative || ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync) || isGeneratorFunction(node) {
		return nil
	}
	signature := state.Checker.GetSignatureFromDeclaration(node)
	if signature == nil {
		return nil
	}
	b := state.nativeTypes()
	pack := b.BuildReturnPack(state.Checker.GetReturnTypeOfSignature(signature), typeContext{Position: positionReturn, Owner: node})
	annotation, _, _ := applyNativePolicy(pack, b, nativePositionReturn, 0)
	return annotation
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

// getNativeFunctionGenerics returns the generic parameters a function
// declares, ready for `function f<K, V>`. Constraints are dropped: Luau has no
// `extends` clause on type parameters. A parameter Luau cannot name is
// skipped; references to it then widen to `any`.
func getNativeFunctionGenerics(state *State, node *ast.Node) []luau.TypeGeneric {
	if !state.IsNative {
		return nil
	}
	return state.nativeTypes().Generics(node)
}

// ---------------------------------------------------------------------------
// Generic scope
// ---------------------------------------------------------------------------

// typeGenericScope is one level of generic names visible to the annotation
// currently being rendered, shared by the native annotations and the luauTypes
// builder. barrier marks a type alias body: names declared by the enclosing
// function are not in scope inside a file-level alias.
type typeGenericScope struct {
	names   []string
	barrier bool
}

// PushTypeGenerics enters the generic scope of a function (or alias body)
// while its signature and body are transformed. Every function pushes a scope,
// even one that declares no generics, so "the current function's own generic"
// is always the innermost entry.
func (state *State) PushTypeGenerics(names []string, barrier bool) {
	state.typeScopes = append(state.typeScopes, typeGenericScope{names: names, barrier: barrier})
}

// PopTypeGenerics leaves the innermost generic scope.
func (state *State) PopTypeGenerics() {
	if n := len(state.typeScopes); n > 0 {
		state.typeScopes = state.typeScopes[:n-1]
	}
}

// typeGenericInScope reports whether name is a generic visible from the
// current annotation, and whether it belongs to the innermost function (rule
// R1: only those may appear bare in a tagged position). R1 stays a query: the
// builder reports it through builtType.OuterGeneric, and the native policy
// asks it again (typeBuilder.ownGeneric) for the bare generics left at the
// top of a parameter annotation.
func (state *State) typeGenericInScope(name string) (inScope, own bool) {
	for index := len(state.typeScopes) - 1; index >= 0; index-- {
		scope := state.typeScopes[index]
		if slices.Contains(scope.names, name) {
			return true, index == len(state.typeScopes)-1 && !scope.barrier
		}
		if scope.barrier {
			return false, false
		}
	}
	return false, false
}

// getNativeTypeParameterName resolves a type variable to its Luau generic name
// when that generic is in scope. Class type parameters (and any other variable
// Luau cannot declare) resolve to nothing and widen to `any`.
func getNativeTypeParameterName(state *State, checkerType *checker.Type) (name string, own, ok bool) {
	symbol := checkerType.Symbol()
	if symbol == nil || symbol.Flags&ast.SymbolFlagsTypeParameter == 0 {
		return "", false, false
	}
	name = symbol.Name
	if !luau.IsValidIdentifier(name) {
		return "", false, false
	}
	inScope, own := state.typeGenericInScope(name)
	if !inScope {
		return "", false, false
	}
	return name, own, true
}
