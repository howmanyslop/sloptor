package render

import (
	"unicode/utf8"

	"rotor/internal/luau"
)

// DefaultTypeLineWidth is the column past which LayoutTypes breaks a table
// type or an alias-body union across lines, like StyLua's hanging layout.
const DefaultTypeLineWidth = 80

// LayoutTypes marks every table type and alias-body union in ast whose own
// single-line text is wider than width (in runes) as Multiline, children
// before parents, so a type that contains a too-wide type breaks too. It is
// opt-in: RenderAST never calls it, so output stays single-line unless a
// caller asks (the native annotation fixtures carry 200-column aliases). A
// width of zero or less clears every flag.
func LayoutTypes(ast luau.NodeOrList, width int) {
	measure := NewRenderState()
	measure.singleLine = true
	aliasBodies := map[luau.Node]struct{}{}
	visitNodeOrList(ast, &visitor{
		before: func(node luau.Node) {
			if alias, ok := node.(*luau.TypeAlias); ok && alias.Body != nil {
				aliasBodies[alias.Body] = struct{}{}
			}
		},
		after: func(node luau.Node) {
			switch n := node.(type) {
			case *luau.TypeTable:
				n.Multiline = measure.typeTooWide(n, width)
			case *luau.TypeUnion:
				_, isAliasBody := aliasBodies[n]
				n.Multiline = isAliasBody && measure.typeTooWide(n, width)
			}
		},
	})
}

// typeTooWide renders node on one line and compares the width to width.
func (s *RenderState) typeTooWide(node luau.TypeNode, width int) bool {
	if width <= 0 {
		return false
	}
	s.writer.Reset()
	s.render(node)
	return utf8.RuneCountInString(s.writer.String()) > width
}
