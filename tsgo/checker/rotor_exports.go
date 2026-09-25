// ROTOR ADDITION — this file is NOT part of the typescript-go mirror.
//
// Generated from tools/mirror/overlay/checker/rotor_exports.go.tmpl; edit the
// overlay copy, not this file. tools/mirror regenerates ./tsgo from the pinned
// upstream commit (deleting everything first) and then re-applies the overlay
// shims automatically. A missing shim is caught immediately:
// rotor/internal/transformer fails to build without it.

package checker

import "rotor/tsgo/ast"

// GetTypeOfAssignmentPattern exposes getTypeOfAssignmentPattern (services.go)
// for rotor's destructuring transforms: the type of an
// ArrayLiteralExpression/ObjectLiteralExpression used as a destructuring
// assignment LHS (`[a, b] = exp`, `({ a } = exp)`). Mirrors the TypeScript
// checker API of the same name that roblox-ts consumes
// (transformArrayAssignmentPattern.ts L24, transformObjectAssignmentPattern.ts
// L25/L55), including strada's `|| errorType` fallback.
func (c *Checker) GetTypeOfAssignmentPattern(expr *ast.Node) *Type {
	if t := c.getTypeOfAssignmentPattern(expr); t != nil {
		return t
	}
	return c.errorType
}

// GetIndexTypeOfType exposes getIndexTypeOfType (checker.go) for rotor's
// ReadonlyArray.join macro: strada's public
// `typeChecker.getIndexTypeOfType(type, ts.IndexKind.Number)` resolves to
// `getIndexTypeOfType(type, numberType)` — the ELEMENT type of an array —
// which roblox-ts consumes at propertyCallMacros.ts L168-171. Pass
// c.GetNumberType() for IndexKind.Number. nil when no applicable index info
// exists (same as strada's undefined).
func (c *Checker) GetIndexTypeOfType(t *Type, keyType *Type) *Type {
	return c.getIndexTypeOfType(t, keyType)
}

func (t *ConditionalType) RootCheckType() *Type {
	return t.root.checkType
}

func (t *ConditionalType) RootNode() *ast.ConditionalTypeNode {
	return t.root.node
}
