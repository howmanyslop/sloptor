package flamework

import (
	"sort"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
)

type linkedConstraint struct {
	target *checker.Type
	trace  *ast.Node
}

type classNodeMetadata struct {
	flags       map[string]struct{}
	constraints []linkedConstraint
}

func (m classNodeMetadata) Requested(name string) bool {
	if _, excluded := m.flags["~"+name]; excluded {
		return false
	}
	_, requested := m.flags[name]
	_, wildcard := m.flags["*"]
	return requested || wildcard
}

func collectNodeMetadata(state *TransformState, node *ast.Node) classNodeMetadata {
	metadata := classNodeMetadata{flags: make(map[string]struct{})}
	visited := make(map[*ast.Node]struct{})
	collectNodeMetadataInto(state, node, &metadata, visited)
	return metadata
}

func collectNodeMetadataInto(state *TransformState, node *ast.Node, metadata *classNodeMetadata, visited map[*ast.Node]struct{}) {
	if node == nil {
		return
	}
	if _, found := visited[node]; found {
		return
	}
	visited[node] = struct{}{}
	parseJSDocMetadata(state, node, metadata)
	for _, decorator := range node.Decorators() {
		expression := decorator.Expression()
		if ast.IsCallExpression(expression) {
			expression = expression.Expression()
		}
		collectNodeMetadataInto(state, declarationFromNode(state, expression), metadata, visited)
	}
	if ast.IsClassDeclaration(node) || ast.IsClassExpression(node) {
		for _, implemented := range ast.GetImplementsHeritageClauseElements(node) {
			collectNodeMetadataInto(state, declarationFromNode(state, implemented.Expression()), metadata, visited)
		}
	}
	if ast.IsClassElement(node) && node.Name() != nil && node.Parent != nil && (ast.IsClassDeclaration(node.Parent) || ast.IsClassExpression(node.Parent)) {
		for _, implemented := range ast.GetImplementsHeritageClauseElements(node.Parent) {
			symbol := state.checker.GetSymbolAtLocation(implemented.Expression())
			if symbol == nil {
				continue
			}
			if symbol.Flags&ast.SymbolFlagsAlias != 0 {
				symbol = state.checker.GetAliasedSymbol(symbol)
			}
			member := symbol.Members[node.Name().Text()]
			if member == nil {
				continue
			}
			for _, declaration := range member.Declarations {
				collectNodeMetadataInto(state, declaration, metadata, visited)
			}
		}
	}
}

func parseJSDocMetadata(state *TransformState, node *ast.Node, metadata *classNodeMetadata) {
	file := ast.GetSourceFileOfNode(node)
	for current := node; current != nil && !ast.IsSourceFile(current); current = current.Parent {
		parseDirectJSDocMetadata(state, current, file, metadata)
	}
}

func parseDirectJSDocMetadata(state *TransformState, node *ast.Node, file *ast.SourceFile, metadata *classNodeMetadata) {
	for _, jsdoc := range node.JSDoc(file) {
		if jsdoc.AsJSDoc().Tags == nil {
			continue
		}
		for _, tag := range jsdoc.AsJSDoc().Tags.Nodes {
			if tag.TagName() == nil || tag.TagName().Text() != "metadata" {
				continue
			}
			for _, comment := range tag.Comments() {
				if ast.IsJSDocLink(comment) || ast.IsJSDocLinkCode(comment) || ast.IsJSDocLinkPlain(comment) {
					parseLinkedMetadata(state, comment, metadata)
					continue
				}
				for _, token := range strings.Fields(comment.Text()) {
					metadata.flags[token] = struct{}{}
				}
			}
		}
	}
}

func parseLinkedMetadata(state *TransformState, link *ast.Node, metadata *classNodeMetadata) {
	if link.Name() == nil || strings.TrimSpace(link.Text()) != "constraint" {
		return
	}
	symbol := state.checker.GetSymbolAtLocation(link.Name())
	if symbol == nil {
		return
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	var target *checker.Type
	if symbol.Flags&(ast.SymbolFlagsTypeAlias|ast.SymbolFlagsInterface|ast.SymbolFlagsClass) != 0 {
		target = state.checker.GetDeclaredTypeOfSymbol(symbol)
	} else {
		target = state.checker.GetTypeAtLocation(link.Name())
	}
	metadata.constraints = append(metadata.constraints, linkedConstraint{target: target, trace: link})
}

func constraintDiagnostics(state *TransformState, class discoveredClassState) []*ast.Diagnostic {
	metadata := collectNodeMetadata(state, class.node)
	if len(metadata.constraints) == 0 || class.node.Name() == nil {
		return nil
	}
	symbol := state.checker.GetSymbolAtLocation(class.node.Name())
	if symbol == nil {
		return nil
	}
	source := state.checker.GetDeclaredTypeOfSymbol(symbol)
	result := make([]*ast.Diagnostic, 0, len(metadata.constraints))
	for _, constraint := range metadata.constraints {
		if state.checker.IsTypeAssignableTo(source, constraint.target) {
			continue
		}
		diagnostic := ast.NewDiagnostic(
			ast.GetSourceFileOfNode(class.node), nodeRange(class.node.Name()), diagnostics.Type_0_does_not_satisfy_the_constraint_1,
			state.checker.TypeToString(source), state.checker.TypeToString(constraint.target),
		)
		diagnostic.AddRelatedInfo(ast.NewDiagnostic(
			ast.GetSourceFileOfNode(constraint.trace), nodeRange(constraint.trace), diagnostics.X_0_is_declared_here, "The constraint",
		))
		result = append(result, diagnostic)
	}
	return result
}

func nodeRange(node *ast.Node) core.TextRange {
	return core.NewTextRange(node.Pos(), node.End())
}

func orderClassDiagnostics(input []*ast.Diagnostic) []*ast.Diagnostic {
	ordered := append([]*ast.Diagnostic(nil), input...)
	sort.SliceStable(ordered, func(left, right int) bool {
		leftFile, rightFile := diagnosticFileName(ordered[left]), diagnosticFileName(ordered[right])
		if leftFile != rightFile {
			return leftFile < rightFile
		}
		if ordered[left].Pos() != ordered[right].Pos() {
			return ordered[left].Pos() < ordered[right].Pos()
		}
		return ordered[left].String() < ordered[right].String()
	})
	return ordered
}
