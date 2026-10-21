package compile

import (
	"unicode/utf8"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/scanner"
)

// TransformerRebase lets the sidecar rebuild, for a file native jecs rewrote,
// the tree the JavaScript jecs transformer would have handed the next plugin:
// the printed rewrite's nodes, each carrying the range it was written at in the
// file on disk, or none when jecs made it up. A plugin that maps its own edits
// back through those ranges, as the React compiler applier does, then reports
// the positions it reported under the JavaScript transformer, and so does the
// trace the sidecar prints. Reprinted text alone would leave every node
// pointing into the rewrite, which composing the two traces only approximates.
type TransformerRebase struct {
	// Text is the file on disk the ranges index.
	Text string `json:"text"`
	// Ranges holds five numbers per node of the printed rewrite, in preorder:
	// its start and end in the printed text, its pos and end in Text (-1 for a
	// node jecs synthesized), and 1 when the node is not from the parse of
	// Text. Offsets count UTF-16 code units, as TypeScript's do.
	Ranges []int `json:"ranges"`
}

// newJecsRebase pairs the tree native jecs printed with the parse of what it
// printed. It returns nil when the two do not line up node for node, and the
// sidecar then works from the printed text alone, as it always has.
func newJecsRebase(original, transformed *ast.SourceFile, printed string) *TransformerRebase {
	parsed := parser.ParseSourceFile(original.ParseOptions(), printed, core.GetScriptKindFromFileName(original.FileName()))
	fromOriginal := map[*ast.Node]struct{}{}
	var collect func(node *ast.Node) bool
	collect = func(node *ast.Node) bool {
		fromOriginal[node] = struct{}{}
		node.ForEachChild(collect)
		return false
	}
	original.AsNode().ForEachChild(collect)

	printedOffset := utf16Offsets(printed)
	originalOffset := utf16Offsets(original.Text())
	ranges := []int{}
	var pair func(node, reparsed *ast.Node) bool
	pair = func(node, reparsed *ast.Node) bool {
		start := printedOffset[scanner.GetTokenPosOfNode(reparsed, parsed, false)]
		// The printer parenthesizes expressions and types where precedence
		// demands it; the tree it printed has no node there.
		if (reparsed.Kind == ast.KindParenthesizedExpression || reparsed.Kind == ast.KindParenthesizedType) && node.Kind != reparsed.Kind {
			ranges = append(ranges, start, printedOffset[reparsed.End()], -1, -1, 1)
			return pair(node, childNodes(reparsed)[0])
		}
		if node.Kind != reparsed.Kind {
			return false
		}
		pos, end := -1, -1
		if node.Pos() >= 0 && node.End() >= node.Pos() && node.End() < len(originalOffset) {
			pos, end = originalOffset[node.Pos()], originalOffset[node.End()]
		}
		synthesized := 1
		if _, ok := fromOriginal[node]; ok {
			synthesized = 0
		}
		ranges = append(ranges, start, printedOffset[reparsed.End()], pos, end, synthesized)
		children, reparsedChildren := childNodes(node), childNodes(reparsed)
		if len(children) != len(reparsedChildren) {
			return false
		}
		for index, child := range children {
			if !pair(child, reparsedChildren[index]) {
				return false
			}
		}
		return true
	}
	statements, reparsedStatements := transformed.Statements.Nodes, parsed.Statements.Nodes
	if len(statements) != len(reparsedStatements) {
		return nil
	}
	for index, statement := range statements {
		if !pair(statement, reparsedStatements[index]) {
			return nil
		}
	}
	return &TransformerRebase{Text: original.Text(), Ranges: ranges}
}

func childNodes(node *ast.Node) []*ast.Node {
	var children []*ast.Node
	node.ForEachChild(func(child *ast.Node) bool {
		children = append(children, child)
		return false
	})
	return children
}

// utf16Offsets maps every byte offset of text, and its end, to the UTF-16
// offset TypeScript gives the same position.
func utf16Offsets(text string) []int {
	offsets := make([]int, len(text)+1)
	units := 0
	for index := 0; index < len(text); {
		r, size := utf8.DecodeRuneInString(text[index:])
		for byteIndex := index; byteIndex < index+size; byteIndex++ {
			offsets[byteIndex] = units
		}
		units++
		if r > 0xFFFF {
			units++
		}
		index += size
	}
	offsets[len(text)] = units
	return offsets
}
