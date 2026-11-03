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

// binding renders an identifier where it is bound, with its `: T` annotation.
func (s *RenderState) binding(identifier luau.AnyIdentifier) {
	s.render(identifier)
	if annotation := luau.BindingType(identifier); annotation != nil {
		s.write(": ")
		s.renderType(annotation, typeSlotTop)
	}
}

func (s *RenderState) typedBindings(identifiers *luau.List[luau.AnyIdentifier]) {
	for current := identifiers.Head; current != nil; current = current.Next {
		if current != identifiers.Head {
			s.write(", ")
		}
		s.binding(current.Value)
	}
}

// bindings renders the left side of a `local`: one identifier or a list,
// each with its binding annotation.
func (s *RenderState) bindings(left luau.NodeOrList) {
	switch x := left.(type) {
	case *luau.List[luau.AnyIdentifier]:
		if x.IsEmpty() {
			panic("empty identifier list")
		}
		s.typedBindings(x)
	case luau.AnyIdentifier:
		s.binding(x)
	default:
		s.exprOrList(left)
	}
}

func (s *RenderState) parameters(node luau.HasParameters) {
	params, hasDotDotDot, varArgsType := node.ParamData()
	if params == nil {
		// Unreachable: every function constructor receives its (possibly
		// empty) parameter list. Fail loudly here rather than nil-dereffing
		// in identifiers below.
		panic("render: parameters got nil parameter list") // upstream assert
	}
	s.typedBindings(params)
	if hasDotDotDot {
		if params.IsNonEmpty() {
			s.write(", ")
		}
		s.write("...")
		if varArgsType != nil {
			s.write(": ")
			s.renderType(varArgsType, typeSlotTop)
		}
	}
}

func (s *RenderState) returnType(node luau.HasParameters) {
	if annotation := node.ReturnTypeData(); annotation != nil {
		s.write(": ")
		s.renderType(annotation, typeSlotTop)
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
