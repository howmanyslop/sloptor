package transformer

import (
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
// guard would fail on every call.
const (
	nativeTypeNumber = "number"
	nativeTypeString = "string"
	nativeTypeTable  = "{ [any]: any }"
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
// the optional marker; every other member must agree on one Luau type.
func nativeLuauType(s *State, t *checker.Type) *luau.Type {
	members := []*checker.Type{t}
	if t.IsUnion() {
		members = t.Types()
	}

	optional := false
	name := ""
	for _, member := range members {
		if isUndefinedTypeCheck(member) {
			optional = true
			continue
		}
		memberName := nativeLuauTypeName(s, member)
		if memberName == "" || (name != "" && name != memberName) {
			return nil
		}
		name = memberName
	}
	if name == "" {
		return nil
	}
	return &luau.Type{Name: name, Optional: optional}
}

func nativeLuauTypeName(s *State, t *checker.Type) string {
	switch {
	case t.Flags()&checker.TypeFlagsTypeVariable != 0:
		constraint := s.Checker.GetBaseConstraintOfType(t)
		if constraint == nil || constraint == t {
			return ""
		}
		if resolved := nativeLuauType(s, constraint); resolved != nil && !resolved.Optional {
			return resolved.Name
		}
		return ""
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
	case isNativeTableType(s, t):
		return nativeTypeTable
	}
	return ""
}

// isNativeTableType accepts object types that are always plain Lua tables.
// Classes are excluded because their instances carry a metatable, which
// Luau's analyzer will not accept for a table type literal. Types with no
// members (`{}`, `defined`) are excluded because they also admit primitives.
func isNativeTableType(s *State, t *checker.Type) bool {
	if symbol := t.Symbol(); symbol != nil && symbol.Flags&ast.SymbolFlagsClass != 0 {
		return false
	}
	if symbolIsAmbient(s, t, "Object", "Function") {
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
	annotation := nativeLuauType(s, s.Checker.GetTypeAtLocation(parameter.Name()))
	if annotation != nil && (parameter.QuestionToken != nil || parameter.Initializer != nil) {
		annotation.Optional = true
	}
	return annotation
}

// nativeReturnType is the annotation for a function's return, nil outside
// `--!native` files. Async and
// generator functions return a wrapper, so they get none.
func nativeReturnType(s *State, node *ast.Node) *luau.Type {
	if !s.IsNative || ast.HasSyntacticModifier(node, ast.ModifierFlagsAsync) || isGeneratorFunction(node) {
		return nil
	}
	signature := s.Checker.GetSignatureFromDeclaration(node)
	if signature == nil {
		return nil
	}
	return nativeLuauType(s, s.Checker.GetReturnTypeOfSignature(signature))
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
