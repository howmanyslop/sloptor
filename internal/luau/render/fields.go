package render

import (
	"regexp"

	"rotor/internal/luau"
)

// renderMapField.ts
func renderMapField(s *RenderState, node *luau.MapField) {
	if str, ok := node.Index.(*luau.StringLiteral); ok && luau.IsValidIdentifier(str.Value) {
		s.identifierKey(str)
	} else {
		s.write("[")
		s.render(node.Index)
		s.write("]")
	}
	s.write(" = ")
	s.render(node.Value)
}

// Identifier-shaped string indices are emitted without quotes.
func (s *RenderState) identifierKey(node *luau.StringLiteral) {
	if !s.positions {
		s.write(node.Value)
		return
	}
	index := s.beginRange(node)
	s.write(node.Value)
	s.endRange(index)
}

// renderInterpolatedStringPart.ts
var (
	bracePartRegex   = regexp.MustCompile(`(\\u\{[a-fA-F0-9]+\})|([{}])`)
	newlinePartRegex = regexp.MustCompile("(\r\n?|\n)")
)

func renderInterpolatedStringPart(s *RenderState, node *luau.InterpolatedStringPart) {
	// escape braces, but do not touch braces within unicode escape codes
	text := bracePartRegex.ReplaceAllStringFunc(node.Text, func(m string) string {
		if m == "{" || m == "}" {
			return "\\" + m
		}
		return m // unicode escape untouched
	})
	// escape newlines, captures a CR with optionally an LF after it or just an LF on its own
	s.write(newlinePartRegex.ReplaceAllString(text, "\\$1"))
}
