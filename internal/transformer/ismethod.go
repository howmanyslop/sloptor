package transformer

import (
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func isMethodDeclarationNode(node *ast.Node) bool {
	if ast.IsMethodDeclaration(node) || ast.IsMethodSignatureDeclaration(node) {
		return true
	}
	if ast.IsFunctionExpression(node) {
		parent := SkipUpwards(node).Parent
		return parent != nil && ast.IsPropertyAssignment(parent)
	}
	return false
}

// isMethodInner ports isMethod.ts isMethodInner (L51-77): scan the call
// signatures — an explicit `this` parameter typed void marks a callback, any
// other `this` type marks a method; signatures without a thisParameter symbol
// fall back to their declaration's shape. Mixing both on one type is a
// noMixedTypeCall error.
func isMethodInner(s *State, node *ast.Node, t *checker.Type) bool {
	hasMethodDefinition := false
	hasCallbackDefinition := false

	for _, callSignature := range s.Checker.GetCallSignatures(t) {
		if thisParameter := callSignature.ThisParameter(); thisParameter != nil {
			thisType := s.Checker.GetTypeOfSymbolAtLocation(thisParameter, node)
			if canChangeReceiverConvention(s, thisType) {
				AddDiagnosticWithCache(s.Diags, node, DiagNoUnstableThisType(node), s.Multi.IsReportedByNoUnstableThisType)
			}
			if thisType.Flags()&checker.TypeFlagsVoid == 0 {
				hasMethodDefinition = true
			} else {
				hasCallbackDefinition = true
			}
		} else if declaration := callSignature.Declaration(); declaration != nil {
			if isMethodDeclarationNode(declaration) {
				hasMethodDefinition = true
			} else {
				hasCallbackDefinition = true
			}
		}
	}

	if hasMethodDefinition && hasCallbackDefinition {
		s.Diags.Add(DiagNoMixedTypeCall(node))
	}

	return hasMethodDefinition
}

// isMethodFromType ports isMethod.ts isMethodFromType (L79-91): walk the
// type's union/intersection members and constraints; each leaf with a symbol
// consults the cache for that instantiated type.
func isMethodFromType(s *State, node *ast.Node, t *checker.Type) bool {
	result := false
	WalkTypes(s, t, func(t *checker.Type) {
		// NOTE upstream `result ||= getOrSetDefault(...)` short-circuits: once
		// result is true, later leaves are neither checked nor cached (and
		// cannot add noMixedTypeCall diagnostics) — preserved exactly.
		if result {
			return
		}
		if symbol := t.Symbol(); symbol != nil {
			cached, ok := s.Multi.IsMethodCache[t]
			if !ok {
				cached = isMethodInner(s, node, t)
				s.Multi.IsMethodCache[t] = cached
			}
			result = result || cached
		}
	})
	return result
}

// isMethod ports isMethod.ts isMethod (L93-98).
func isMethod(s *State, node *ast.Node) bool {
	t := s.GetType(node)
	if isMethodFromType(s, node, t) {
		return true
	}
	if s.Checker != nil && ast.IsFunctionExpression(node) {
		parameters := node.Parameters()
		if len(parameters) > 0 && ast.IsThisIdentifier(parameters[0].Name()) {
			return false
		}
		if contextualType := s.Checker.GetContextualType(node, checker.ContextFlagsNone); contextualType != nil && contextualType != t {
			return isMethodFromType(s, node, contextualType)
		}
	}
	return false
}
