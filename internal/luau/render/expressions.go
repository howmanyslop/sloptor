package render

import (
	"strings"

	"rotor/internal/luau"
)

func renderIdentifier(s *RenderState, node *luau.Identifier) {
	if !luau.IsValidIdentifier(node.Name) {
		panic("Invalid Luau Identifier: \"" + node.Name + "\"")
	}
	s.write(node.Name)
}

func renderTemporaryIdentifier(s *RenderState, node *luau.TemporaryIdentifier) {
	name := s.getTempName(node)
	if !luau.IsValidIdentifier(name) {
		panic("Invalid Temporary Identifier: \"" + name + "\"")
	}
	s.write(name)
}

func renderComputedIndexExpression(s *RenderState, node *luau.ComputedIndexExpression) {
	s.render(node.Expression)
	if str, ok := node.Index.(*luau.StringLiteral); ok && luau.IsValidIdentifier(str.Value) {
		s.write(".")
		s.identifierKey(str)
		return
	}
	s.write("[")
	s.render(node.Index)
	s.write("]")
}

func renderPropertyAccessExpression(s *RenderState, node *luau.PropertyAccessExpression) {
	s.render(node.Expression)
	if luau.IsValidIdentifier(node.Name) {
		s.write(".")
		s.write(node.Name)
		return
	}
	s.write("[\"")
	s.write(node.Name)
	s.write("\"]")
}

func renderCallExpression(s *RenderState, node *luau.CallExpression) {
	s.render(node.Expression)
	s.write("(")
	s.arguments(node.Args)
	s.write(")")
}

func renderMethodCallExpression(s *RenderState, node *luau.MethodCallExpression) {
	if !luau.IsValidIdentifier(node.Name) {
		panic("invalid method name: " + node.Name)
	}
	s.render(node.Expression)
	s.write(":")
	s.write(node.Name)
	s.write("(")
	s.arguments(node.Args)
	s.write(")")
}

func renderParenthesizedExpression(s *RenderState, node *luau.ParenthesizedExpression) {
	expression := node.Expression
	for {
		if p, ok := expression.(*luau.ParenthesizedExpression); ok {
			expression = p.Expression
		} else {
			break
		}
	}
	if luau.IsSimple(expression) {
		s.render(node.Expression)
		return
	}
	s.write("(")
	s.render(node.Expression)
	s.write(")")
}

func renderNumberLiteral(s *RenderState, node *luau.NumberLiteral) {
	if luau.IsValidNumberLiteral(node.Value) {
		s.write(node.Value)
		return
	}
	f, err := luau.JSNumberParse(node.Value)
	if err != nil {
		s.write("NaN")
		return
	}
	s.write(luau.JSNumberString(f))
}

func needsBracketSpacing(node *luau.StringLiteral) bool {
	switch p := node.Parent().(type) {
	case *luau.MapField:
		return luau.Node(node) == luau.Node(p.Index)
	case *luau.ComputedIndexExpression:
		return luau.Node(node) == luau.Node(p.Index)
	case *luau.Set:
		return true
	}
	return false
}

func renderStringLiteral(s *RenderState, node *luau.StringLiteral) {
	isMultiline := strings.Contains(node.Value, "\n")
	if !isMultiline && !strings.Contains(node.Value, "\"") {
		s.write("\"")
		s.write(node.Value)
		s.write("\"")
		return
	}
	if !isMultiline && !strings.Contains(node.Value, "'") {
		s.write("'")
		s.write(node.Value)
		s.write("'")
		return
	}
	eqStr := getSafeBracketEquals(node.Value)
	spacing := needsBracketSpacing(node)
	if spacing {
		s.write(" ")
	}
	s.write("[")
	s.write(eqStr)
	s.write("[")
	s.write(node.Value)
	s.write("]")
	s.write(eqStr)
	s.write("]")
	if spacing {
		s.write(" ")
	}
}

func renderFunctionExpression(s *RenderState, node *luau.FunctionExpression) {
	s.write("function(")
	s.parameters(node)
	s.write(")")
	s.returnType(node)
	if node.Statements.IsEmpty() {
		s.write(" ")
		s.closing("end")
		return
	}
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
}

func renderBinaryExpression(s *RenderState, node *luau.BinaryExpression) {
	parens := needsParentheses(node)
	if parens {
		s.write("(")
	}
	s.render(node.Left)
	s.write(" ")
	s.operator(string(node.Operator))
	s.write(" ")
	s.render(node.Right)
	if parens {
		s.write(")")
	}
}

func unaryNeedsSpace(node *luau.UnaryExpression) bool {
	if node.Operator == "not" {
		return true
	}
	if inner, ok := node.Expression.(*luau.UnaryExpression); ok && inner.Operator == "-" {
		return true
	}
	return false
}

func renderUnaryExpression(s *RenderState, node *luau.UnaryExpression) {
	parens := needsParentheses(node)
	if parens {
		s.write("(")
	}
	s.write(string(node.Operator))
	if unaryNeedsSpace(node) {
		s.write(" ")
	}
	s.render(node.Expression)
	if parens {
		s.write(")")
	}
}

func renderIfExpression(s *RenderState, node *luau.IfExpression) {
	parens := needsParentheses(node)
	if parens {
		s.write("(")
	}
	s.ifExpression(node, "if ")
	if parens {
		s.write(")")
	}
}

func (s *RenderState) ifExpression(node *luau.IfExpression, keyword string) {
	s.write(keyword)
	s.render(node.Condition)
	s.write(" then ")
	s.render(node.Expression)
	s.write(" ")
	if alternative, ok := node.Alternative.(*luau.IfExpression); ok {
		if s.positions {
			index := s.beginRange(alternative)
			s.ifExpression(alternative, "elseif ")
			s.endRange(index)
		} else {
			s.ifExpression(alternative, "elseif ")
		}
		return
	}
	s.write("else ")
	s.render(node.Alternative)
}

func renderInterpolatedString(s *RenderState, node *luau.InterpolatedString) {
	s.write("`")
	for part := node.Parts.Head; part != nil; part = part.Next {
		if _, isPart := part.Value.(*luau.InterpolatedStringPart); isPart {
			s.render(part.Value)
			continue
		}
		s.write("{")
		table := luau.IsTable(part.Value)
		if table {
			s.write("(")
		}
		s.render(part.Value)
		if table {
			s.write(")")
		}
		s.write("}")
	}
	s.write("`")
}

func renderArray(s *RenderState, node *luau.Array) {
	if node.Members.IsEmpty() {
		s.write("{}")
		return
	}
	s.write("{ ")
	s.arguments(node.Members)
	s.write(" }")
}

func renderMap(s *RenderState, node *luau.Map) {
	if node.Fields.IsEmpty() {
		s.write("{}")
		return
	}
	s.write("{")
	s.newline()
	s.pushIndent()
	for field := node.Fields.Head; field != nil; field = field.Next {
		s.writeIndent()
		s.render(field.Value)
		s.write(",")
		s.newline()
	}
	s.popIndent()
	s.writeIndent()
	s.write("}")
}

func renderSet(s *RenderState, node *luau.Set) {
	if node.Members.IsEmpty() {
		s.write("{}")
		return
	}
	s.write("{")
	s.newline()
	s.pushIndent()
	for member := node.Members.Head; member != nil; member = member.Next {
		s.writeIndent()
		s.write("[")
		s.render(member.Value)
		s.write("] = true,")
		s.newline()
	}
	s.popIndent()
	s.writeIndent()
	s.write("}")
}

func renderMixedTable(s *RenderState, node *luau.MixedTable) {
	if node.Fields.IsEmpty() {
		s.write("{}")
		return
	}
	s.write("{")
	s.newline()
	s.pushIndent()
	for field := node.Fields.Head; field != nil; field = field.Next {
		s.writeIndent()
		s.render(field.Value)
		s.write(",")
		s.newline()
	}
	s.popIndent()
	s.writeIndent()
	s.write("}")
}
