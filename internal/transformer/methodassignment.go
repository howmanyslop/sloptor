package transformer

import (
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// This file ports TSTransformer/util/validateMethodAssignment.ts: when a
// function value is assigned where a contextual (or inherited) type expects
// the opposite method-ness, the call-site convention (`:` vs `.`) would
// silently disagree — upstream raises expectedMethodGotFunction /
// expectedFunctionGotMethod instead.

// hasCallSignatures ports validateMethodAssignment.ts hasCallSignatures
// (L8-14).
func hasCallSignatures(s *State, t *checker.Type) bool {
	result := false
	WalkTypes(s, t, func(t *checker.Type) {
		result = result || len(s.Checker.GetCallSignatures(t)) > 0
	})
	return result
}

// validateTypes ports validateMethodAssignment.ts validateTypes (L16-27):
// only when BOTH types are callable does method-ness get compared.
func validateTypes(s *State, node *ast.Node, baseType, assignmentType *checker.Type) {
	if hasCallSignatures(s, baseType) && hasCallSignatures(s, assignmentType) {
		assignmentIsMethod := isMethodFromType(s, node, assignmentType)
		if isMethodFromType(s, node, baseType) != assignmentIsMethod {
			if assignmentIsMethod {
				s.Diags.Add(DiagExpectedMethodGotFunction(node))
			} else {
				s.Diags.Add(DiagExpectedFunctionGotMethod(node))
			}
		}
	}
}

func validateMethodExpression(s *State, node *ast.Node) {
	parent := SkipUpwards(node).Parent
	if parent == nil || ast.IsPropertyAssignment(parent) || ast.IsShorthandPropertyAssignment(parent) {
		return
	}
	t := s.GetType(node)
	if !hasCallSignatures(s, t) {
		return
	}
	contextualType := s.Checker.GetContextualType(node, checker.ContextFlagsNone)
	if contextualType == nil || contextualType == t {
		return
	}
	expression := SkipDownwards(node)
	if ast.IsFunctionExpression(expression) && isMethod(s, expression) == isMethodFromType(s, node, contextualType) {
		return
	}
	isAccess := ast.IsPropertyAccessExpression(expression) || ast.IsElementAccessExpression(expression)
	if isAccess && !isValidMethodIndexWithoutCall(s, SkipUpwards(node)) && isMethodFromType(s, node, t) {
		return
	}
	validateTypes(s, node, t, contextualType)
}

// validateObjectLiteralElement ports validateMethodAssignment.ts
// validateObjectLiteralElement (L29-35): compare the element's own type
// against its contextual type, when one exists and differs.
func validateObjectLiteralElement(s *State, node *ast.Node) {
	t := s.GetType(node)
	contextualType := s.Checker.GetContextualTypeForObjectLiteralElement(node, checker.ContextFlagsNone)
	if contextualType != nil && contextualType != t {
		validateTypes(s, node, t, contextualType)
	}
}

// validateSpread ports validateMethodAssignment.ts validateSpread (L48-61):
// per property of the spread expression's type, compare against the
// contextual type's same-named property.
func validateSpread(s *State, node *ast.Node) {
	expression := node.AsSpreadAssignment().Expression
	t := s.GetType(expression)
	contextualType := s.Checker.GetContextualType(expression, checker.ContextFlagsNone)
	if contextualType == nil {
		return
	}

	for _, property := range s.Checker.GetPropertiesOfType(t) {
		basePropertyType := s.Checker.GetTypeOfPropertyOfType(t, property.Name)
		assignmentPropertyType := s.Checker.GetTypeOfPropertyOfType(contextualType, property.Name)
		if basePropertyType == nil || assignmentPropertyType == nil {
			continue
		}
		validateTypes(s, node, basePropertyType, assignmentPropertyType)
	}
}

// validateMethodAssignment ports validateMethodAssignment.ts (L63-77).
func validateMethodAssignment(s *State, node *ast.Node) {
	// Checker-free test states (parse-only literal tests) skip validation —
	// upstream always has a checker (same pattern as State.AmbientSymbol).
	if s.Checker == nil {
		return
	}
	if ast.IsClassElement(node) && node.Parent != nil && ast.IsClassLike(node.Parent) && node.Name() != nil {
		for _, typeNode := range getAllSuperTypeNodes(node.Parent) {
			validateHeritageClause(s, node, typeNode)
		}
		return
	}
	if ast.IsObjectLiteralElement(node) {
		if ast.IsSpreadAssignment(node) {
			if !ast.IsObjectLiteralExpression(node.AsSpreadAssignment().Expression) {
				validateSpread(s, node)
			}
		} else {
			validateObjectLiteralElement(s, node)
		}
	}
}
