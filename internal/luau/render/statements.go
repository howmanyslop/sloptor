package render

import (
	"strings"

	"rotor/internal/luau"
)

func (s *RenderState) exprOrList(v luau.NodeOrList) {
	switch x := v.(type) {
	case *luau.List[luau.Expression]:
		if x.IsEmpty() {
			panic("empty expression list")
		}
		s.arguments(x)
	case *luau.List[luau.WritableExpression]:
		if x.IsEmpty() {
			panic("empty writable expression list")
		}
		for current := x.Head; current != nil; current = current.Next {
			if current != x.Head {
				s.write(", ")
			}
			s.render(current.Value)
		}
	case *luau.List[luau.AnyIdentifier]:
		if x.IsEmpty() {
			panic("empty identifier list")
		}
		s.identifiers(x)
	case luau.Node:
		s.render(x)
	default:
		panic("renderExprOrList: unsupported value")
	}
}

func renderReturnStatement(s *RenderState, node *luau.ReturnStatement) {
	// Empty tuple returns preserve upstream's trailing space.
	s.write("return ")
	if list, ok := node.Expression.(*luau.List[luau.Expression]); !ok || list.IsNonEmpty() {
		s.exprOrList(node.Expression)
	}
	s.newline()
}

func renderAssignment(s *RenderState, node *luau.Assignment) {
	s.exprOrList(node.Left)
	s.write(" ")
	s.write(string(node.Operator))
	s.write(" ")
	s.exprOrList(node.Right)
	s.write(getEnding(s, node))
	s.newline()
}

func renderCallStatement(s *RenderState, node *luau.CallStatement) {
	s.render(node.Expression)
	s.write(getEnding(s, node))
	s.newline()
}

func renderComment(s *RenderState, node *luau.Comment) {
	text := strings.ReplaceAll(node.Text, "\r\n", "\n")
	text = strings.ReplaceAll(text, "\r", "\n")
	if strings.Contains(text, "\n") {
		eqStr := getSafeBracketEquals(text)
		s.write("--[")
		s.write(eqStr)
		s.write("[")
		s.newline()
		s.pushIndent()
		for _, line := range strings.Split(text, "\n") {
			s.writeIndent()
			s.write(line)
			s.newline()
		}
		s.popIndent()
		s.writeIndent()
		s.write("]")
		s.write(eqStr)
		s.write("]")
		s.newline()
		return
	}
	s.write("--")
	s.write(text)
	s.newline()
}

func renderDoStatement(s *RenderState, node *luau.DoStatement) {
	s.write("do")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderWhileStatement(s *RenderState, node *luau.WhileStatement) {
	s.write("while ")
	s.render(node.Condition)
	s.write(" do")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderRepeatStatement(s *RenderState, node *luau.RepeatStatement) {
	s.write("repeat")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("until ")
	s.render(node.Condition)
	s.newline()
}

func renderIfStatement(s *RenderState, node *luau.IfStatement) {
	s.ifStatement(node, "if ")
}

func (s *RenderState) ifStatement(node *luau.IfStatement, keyword string) {
	s.write(keyword)
	s.render(node.Condition)
	s.write(" then")
	s.newline()
	s.block(node.Statements)
	if link, ok := node.ElseBody.(*luau.IfStatement); ok {
		s.writeIndent()
		if s.positions {
			parent := s.currentRange
			index := s.beginRange(link)
			s.currentRange = index
			s.ifStatement(link, "elseif ")
			s.endRange(index)
			s.currentRange = parent
			s.ranges[parent].closing = s.ranges[index].closing
		} else {
			s.ifStatement(link, "elseif ")
		}
		return
	}
	if elseList, ok := node.ElseBody.(*luau.List[luau.Statement]); ok && elseList.IsNonEmpty() {
		s.writeIndent()
		s.write("else")
		s.newline()
		s.block(elseList)
	}
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderNumericForStatement(s *RenderState, node *luau.NumericForStatement) {
	s.write("for ")
	s.render(node.ID)
	s.write(" = ")
	s.render(node.Start)
	s.write(", ")
	s.render(node.End)
	if node.Step != nil {
		isOne := false
		if lit, ok := node.Step.(*luau.NumberLiteral); ok {
			if f, err := luau.JSNumberParse(lit.Value); err == nil && f == 1 {
				isOne = true
			}
		}
		if !isOne {
			s.write(", ")
			s.render(node.Step)
		}
	}
	s.write(" do")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderForStatement(s *RenderState, node *luau.ForStatement) {
	s.write("for ")
	if node.IDs.IsEmpty() {
		s.write("_")
	} else {
		s.identifiers(node.IDs)
	}
	s.write(" in ")
	s.render(node.Expression)
	s.write(" do")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderFunctionDeclaration(s *RenderState, node *luau.FunctionDeclaration) {
	if node.Localize {
		if _, ok := node.Name.(luau.AnyIdentifier); !ok {
			panic("local function cannot be a property")
		}
		s.write("local ")
	}
	s.write("function ")
	s.render(node.Name)
	s.write("(")
	s.parameters(node)
	s.write(")")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderMethodDeclaration(s *RenderState, node *luau.MethodDeclaration) {
	s.write("function ")
	s.render(node.Expression)
	s.write(":")
	s.write(node.Name)
	s.write("(")
	s.parameters(node)
	s.write(")")
	s.newline()
	s.block(node.Statements)
	s.writeIndent()
	s.closing("end")
	s.newline()
}

func renderVariableDeclaration(s *RenderState, node *luau.VariableDeclaration) {
	s.write("local ")
	s.exprOrList(node.Left)
	if node.Right != nil {
		s.write(" = ")
		s.exprOrList(node.Right)
	}
	s.write(getEnding(s, node))
	s.newline()
}
