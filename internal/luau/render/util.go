package render

import (
	"strings"

	"rotor/internal/luau"
)

// getSafeBracketEquals returns the `=` padding needed so that `[`+eq+`[` and
// `]`+eq+`]` brackets safely enclose str. Literal port of upstream
// getSafeBracketEquals.ts.
func getSafeBracketEquals(str string) string {
	amtEquals := 0
	for strings.Contains(str, "]"+strings.Repeat("=", amtEquals)+"]") ||
		strings.HasSuffix(str, "]"+strings.Repeat("=", amtEquals)) {
		amtEquals++
	}
	return strings.Repeat("=", amtEquals)
}

func (s *RenderState) arguments(expressions *luau.List[luau.Expression]) {
	for current := expressions.Head; current != nil; current = current.Next {
		if current != expressions.Head {
			s.write(", ")
		}
		s.render(current.Value)
	}
}

func (s *RenderState) identifiers(identifiers *luau.List[luau.AnyIdentifier]) {
	for current := identifiers.Head; current != nil; current = current.Next {
		if current != identifiers.Head {
			s.write(", ")
		}
		s.render(current.Value)
	}
}

func (s *RenderState) parameters(node luau.HasParameters) {
	params, hasDotDotDot := node.ParamData()
	s.identifiers(params)
	if hasDotDotDot {
		if params.IsNonEmpty() {
			s.write(", ")
		}
		s.write("...")
	}
}

func renderStatements(s *RenderState, statements *luau.List[luau.Statement]) string {
	start := s.writer.Len()
	s.statements(statements)
	return s.writer.String()[start:]
}

func (s *RenderState) statements(statements *luau.List[luau.Statement]) {
	hasFinalStatement := false
	for listNode := statements.Head; listNode != nil; listNode = listNode.Next {
		if hasFinalStatement {
			if _, isComment := listNode.Value.(*luau.Comment); !isComment {
				panic("Cannot render statement after break, continue, or return!")
			}
		}
		hasFinalStatement = hasFinalStatement || luau.IsFinalStatement(listNode.Value)
		s.pushListNode(listNode)
		s.render(listNode.Value)
		s.popListNode()
	}
}
