package flamework

import (
	"strings"

	"rotor/tsgo/ast"
)

func metadataForNode(state *TransformState, node *ast.Node) Metadata {
	metadata := NewMetadata(nil)
	file := ast.GetSourceFileOfNode(node)
	for _, jsdoc := range node.JSDoc(file) {
		if jsdoc.AsJSDoc().Tags == nil {
			continue
		}
		for _, tag := range jsdoc.AsJSDoc().Tags.Nodes {
			if tag.Kind != ast.KindJSDocUnknownTag || tag.TagName().Text() != "metadata" {
				continue
			}
			var text strings.Builder
			for _, comment := range tag.Comments() {
				text.WriteString(comment.Text())
			}
			metadata = mergeMetadata(metadata, ParseMetadataText(text.String()))
		}
	}
	return metadata
}

func metadataForDecorator(state *TransformState, identifier *ast.Node) Metadata {
	symbol := state.checker.GetSymbolAtLocation(identifier)
	metadata := NewMetadata(nil)
	if symbol == nil {
		return metadata
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	for _, declaration := range symbol.Declarations {
		for current := declaration; current != nil && !ast.IsSourceFile(current); current = current.Parent {
			metadata = mergeMetadata(metadata, metadataForNode(state, current))
			if ast.IsStatement(current) {
				break
			}
		}
	}
	return metadata
}

func mergeMetadata(left, right Metadata) Metadata {
	return NewMetadata(append(left.Tokens(), right.Tokens()...))
}

func metadataForClass(state *TransformState, node *ast.Node) Metadata {
	metadata := metadataForDecoratedNode(state, node)
	for _, implemented := range ast.GetImplementsTypeNodes(node) {
		symbol := state.checker.GetSymbolAtLocation(implemented.AsExpressionWithTypeArguments().Expression)
		if symbol == nil {
			continue
		}
		if symbol.Flags&ast.SymbolFlagsAlias != 0 {
			symbol = state.checker.GetAliasedSymbol(symbol)
		}
		for _, declaration := range symbol.Declarations {
			metadata = mergeMetadata(metadata, metadataForNode(state, declaration))
		}
	}
	return metadata
}

func metadataForMember(state *TransformState, member *ast.Node) Metadata {
	metadata := metadataForDecoratedNode(state, member)
	if member.Name() == nil || member.Parent == nil || !ast.IsClassLike(member.Parent) {
		return metadata
	}
	name := ast.GetPropertyNameForPropertyNameNode(member.Name())
	for _, implemented := range ast.GetImplementsTypeNodes(member.Parent) {
		symbol := state.checker.GetSymbolAtLocation(implemented.AsExpressionWithTypeArguments().Expression)
		if symbol == nil {
			continue
		}
		if symbol.Flags&ast.SymbolFlagsAlias != 0 {
			symbol = state.checker.GetAliasedSymbol(symbol)
		}
		interfaceMember := symbol.Members[name]
		if interfaceMember == nil {
			continue
		}
		for _, declaration := range interfaceMember.Declarations {
			metadata = mergeMetadata(metadata, metadataForNode(state, declaration))
		}
	}
	return metadata
}

func metadataForDecoratedNode(state *TransformState, node *ast.Node) Metadata {
	metadata := metadataForNode(state, node)
	if modifiers := node.Modifiers(); modifiers != nil {
		for _, modifier := range modifiers.Nodes {
			if !ast.IsDecorator(modifier) || !isFlameworkDecorator(state, modifier) {
				continue
			}
			expression := modifier.AsDecorator().Expression
			if ast.IsCallExpression(expression) {
				expression = expression.AsCallExpression().Expression
			}
			metadata = mergeMetadata(metadata, metadataForDecorator(state, expression))
		}
	}
	return metadata
}
