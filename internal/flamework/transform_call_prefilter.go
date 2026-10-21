package flamework

import (
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// calleeMayResolveToFlameworkMacro is a sound filter before
// GetResolvedSignature, which costs most of the expression visit: overload
// resolution checks every argument and runs inference, and nearly every call
// in a Flamework game is admitted by callMayBeFlameworkMacro.
//
// A call is a macro only through the signature it resolves to: a parameter
// whose type carries a `_flamework` marker, or `@metadata` JSDoc on the
// signature's declaration (transformFlameworkCall). Resolution picks one of
// the callee type's own signatures, so when none of them has either, no
// instantiation of any of them can. A parameter typed by a type variable or a
// conditional type might gain a marker once instantiated, so those count as
// possible macros (see typeMayCarryFlameworkMarker), as does a callee with no
// signatures to inspect.
func calleeMayResolveToFlameworkMacro(state *TransformState, node *ast.Node) bool {
	kind := checker.SignatureKindCall
	if ast.IsNewExpression(node) {
		kind = checker.SignatureKindConstruct
	}
	calleeType := state.checker.GetTypeAtLocation(node.Expression())
	if calleeType == nil {
		return true
	}
	signatures := state.checker.GetSignaturesOfType(calleeType, kind)
	if len(signatures) == 0 {
		return true
	}
	for _, signature := range signatures {
		if signatureMayBeFlameworkMacro(state, signature) {
			return true
		}
	}
	return false
}

func signatureMayBeFlameworkMacro(state *TransformState, signature *checker.Signature) bool {
	if declaration := signature.Declaration(); declaration != nil && declarationHasMetadataTag(declaration) {
		return true
	}
	for index := range macroParameterCount(state, signature) {
		if typeMayCarryFlameworkMarker(state, state.checker.GetTypeAtPosition(signature, index)) {
			return true
		}
	}
	return false
}

func declarationHasMetadataTag(declaration *ast.Node) bool {
	for _, jsdoc := range declaration.JSDoc(nil) {
		tags := jsdoc.AsJSDoc().Tags
		if tags == nil {
			continue
		}
		for _, tag := range tags.Nodes {
			if tag.TagName().Text() == "metadata" {
				return true
			}
		}
	}
	return false
}

// typeMayCarryFlameworkMarker reports whether a parameter type has, or could
// gain through instantiation, a `_flamework` property. Property names only
// change under instantiation for a type variable, indexed access, conditional
// or substitution type, or a mapped type (`Partial<T>` over a marker type
// carries the marker); every other type already shows its names. Unions and
// intersections are as open as their members: `T | undefined` and `T & {}`
// are open through T.
func typeMayCarryFlameworkMarker(state *TransformState, target *checker.Type) bool {
	if target == nil {
		return true
	}
	if target.Flags()&checker.TypeFlagsInstantiable != 0 {
		return true
	}
	if target.Flags()&checker.TypeFlagsUnionOrIntersection != 0 {
		for _, member := range target.Types() {
			if typeMayCarryFlameworkMarker(state, member) {
				return true
			}
		}
	}
	if target.Flags()&checker.TypeFlagsObject != 0 && target.ObjectFlags()&(checker.ObjectFlagsMapped|checker.ObjectFlagsReverseMapped) != 0 {
		return true
	}
	for _, property := range state.checker.GetPropertiesOfType(target) {
		if strings.HasPrefix(property.Name, flameworkMarkerPrefix) {
			return true
		}
	}
	return false
}

// passThroughFlameworkCall is what transformFlameworkCall returns for a call
// whose signature turns out not to be a macro: the same callee and arguments
// in a fresh argument list. The skipped route builds the identical node, so
// the printed source does not depend on which route a call took.
func passThroughFlameworkCall(state *TransformState, node *ast.Node) MacroTransformResult {
	argumentList := state.factory.NewNodeList(append([]*ast.Node(nil), node.Arguments()...))
	if ast.IsCallExpression(node) {
		call := node.AsCallExpression()
		return MacroTransformResult{Expression: state.factory.UpdateCallExpression(call, call.Expression, call.QuestionDotToken, call.TypeArguments, argumentList, call.Flags)}
	}
	constructor := node.AsNewExpression()
	return MacroTransformResult{Expression: state.factory.UpdateNewExpression(constructor, constructor.Expression, constructor.TypeArguments, argumentList)}
}
