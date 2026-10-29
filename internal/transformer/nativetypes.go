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
// The mirroring rules live in nativetypes_type.go, the named-type aliases in
// nativetypes_alias.go.
const (
	nativeTypeNumber   = "number"
	nativeTypeString   = "string"
	nativeTypeBoolean  = "boolean"
	nativeTypeAny      = "any"
	nativeTypeAnyTable = "{ [any]: any }"
	nativeTypeNil      = "nil"
	nativeTypeUnit     = "()"

	// nativeTypeAnyFunction is the fallback for a callable shape Luau cannot
	// spell (overloads, LuaTuple, construct signatures).
	nativeTypeAnyFunction = "(...any) -> ...any"

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

// nativeTypeKind classifies a rendered type so union handling can decide
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
// nil when the parameter must stay unannotated.
func getNativeParameterType(state *State, parameterNode *ast.Node) *luau.Type {
	parameter := parameterNode.AsParameterDeclaration()
	text, _, outerGeneric := getNativeType(state, state.Checker.GetTypeAtLocation(parameter.Name()), nativePositionParameter, 0, nil)
	if text == "" || outerGeneric {
		// R1: an outer generic (or any other unusable bare name) would tag
		// the argument as userdata.
		return nil
	}
	if parameter.QuestionToken != nil || parameter.Initializer != nil {
		text = getNativeOptionalType(text)
	}
	return &luau.Type{Name: text}
}

// getNativeReturnType is the annotation for a function's return, nil outside
// `--!native` files. Async and generator functions return a wrapper, so they
// get none.
func getNativeReturnType(state *State, node *ast.Node) *luau.Type {
	if !state.IsNative || ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync) || isGeneratorFunction(node) {
		return nil
	}
	signature := state.Checker.GetSignatureFromDeclaration(node)
	if signature == nil {
		return nil
	}
	text, _, _ := getNativeType(state, state.Checker.GetReturnTypeOfSignature(signature), nativePositionReturn, 0, nil)
	if text == "" {
		// A return that cannot be mirrored stays unannotated rather than
		// claiming `any`.
		return nil
	}
	return &luau.Type{Name: text}
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

// getNativeFunctionGenerics returns the generic parameter names a function
// declares, ready for `function f<K, V>`. Constraints are dropped: Luau has no
// `extends` clause on type parameters.
func getNativeFunctionGenerics(state *State, node *ast.Node) []string {
	if !state.IsNative {
		return nil
	}
	return getNativeDeclarationGenerics(node)
}

// getNativeDeclarationGenerics lists the Luau-spellable type parameter names of a
// function, interface, or type alias declaration. A parameter Luau cannot
// spell is skipped; references to it then widen to `any`.
func getNativeDeclarationGenerics(node *ast.Node) []string {
	var names []string
	for _, parameter := range node.TypeParameters() {
		name := parameter.Name()
		if name == nil {
			continue
		}
		if text := name.Text(); luau.IsValidIdentifier(text) {
			names = append(names, text)
		}
	}
	return names
}

// ---------------------------------------------------------------------------
// Generic scope
// ---------------------------------------------------------------------------

// nativeGenericScope is one level of generic names visible to the annotation
// currently being rendered. barrier marks a type alias body: names declared by
// the enclosing function are not in scope inside a file-level alias.
type nativeGenericScope struct {
	names   []string
	barrier bool
}

// PushNativeGenerics enters the generic scope of a function (or alias body)
// while its signature and body are transformed. Every function pushes a scope,
// even one that declares no generics, so "the current function's own generic"
// is always the innermost entry.
func (state *State) PushNativeGenerics(names []string, barrier bool) {
	state.nativeScopes = append(state.nativeScopes, nativeGenericScope{names: names, barrier: barrier})
}

// PopNativeGenerics leaves the innermost generic scope.
func (state *State) PopNativeGenerics() {
	if n := len(state.nativeScopes); n > 0 {
		state.nativeScopes = state.nativeScopes[:n-1]
	}
}

// getNativeGenericInScope reports whether name is a generic visible from the
// current annotation, and whether it belongs to the innermost function (rule
// R1: only those may appear bare in a tagged position).
func (state *State) getNativeGenericInScope(name string) (inScope, own bool) {
	for index := len(state.nativeScopes) - 1; index >= 0; index-- {
		scope := state.nativeScopes[index]
		if slices.Contains(scope.names, name) {
			return true, index == len(state.nativeScopes)-1 && !scope.barrier
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
	inScope, own := state.getNativeGenericInScope(name)
	if !inScope {
		return "", false, false
	}
	return name, own, true
}
