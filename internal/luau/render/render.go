package render

import "rotor/internal/luau"

// Render dispatches on node type — ports upstream render.ts KIND_TO_RENDERER.
func Render(s *RenderState, node luau.Node) string {
	// Unlike RenderAST, standalone Render does not solve scoped temporary names.
	// Reserve its fallback names in the old evaluation order before emitting
	// map-field indices before values. AST rendering bypasses this prepass.
	visitNode(node, &visitor{
		renderOrder: true,
		before: func(node luau.Node) {
			if temp, ok := node.(*luau.TemporaryIdentifier); ok {
				s.getTempName(temp)
			}
		},
	})
	start := s.writer.Len()
	s.render(node)
	return s.writer.String()[start:]
}

func (s *RenderState) render(node luau.Node) {
	if _, ok := node.(luau.Statement); ok {
		s.writeIndent()
	}
	if !s.positions {
		renderNode(s, node)
		return
	}
	parent := s.currentRange
	index := s.beginRange(node)
	s.currentRange = index
	renderNode(s, node)
	s.endRange(index)
	s.currentRange = parent
}

func renderNode(s *RenderState, node luau.Node) {
	switch n := node.(type) {
	// indexable expressions
	case *luau.Identifier:
		renderIdentifier(s, n)
	case *luau.TemporaryIdentifier:
		renderTemporaryIdentifier(s, n)
	case *luau.ComputedIndexExpression:
		renderComputedIndexExpression(s, n)
	case *luau.PropertyAccessExpression:
		renderPropertyAccessExpression(s, n)
	case *luau.CallExpression:
		renderCallExpression(s, n)
	case *luau.MethodCallExpression:
		renderMethodCallExpression(s, n)
	case *luau.ParenthesizedExpression:
		renderParenthesizedExpression(s, n)
	// expressions
	case *luau.None:
		panic("Cannot render None")
	case *luau.NilLiteral:
		s.write("nil")
	case *luau.FalseLiteral:
		s.write("false")
	case *luau.TrueLiteral:
		s.write("true")
	case *luau.NumberLiteral:
		renderNumberLiteral(s, n)
	case *luau.StringLiteral:
		renderStringLiteral(s, n)
	case *luau.VarArgsLiteral:
		s.write("...")
	case *luau.FunctionExpression:
		renderFunctionExpression(s, n)
	case *luau.BinaryExpression:
		renderBinaryExpression(s, n)
	case *luau.UnaryExpression:
		renderUnaryExpression(s, n)
	case *luau.IfExpression:
		renderIfExpression(s, n)
	case *luau.InterpolatedString:
		renderInterpolatedString(s, n)
	case *luau.Array:
		renderArray(s, n)
	case *luau.Map:
		renderMap(s, n)
	case *luau.Set:
		renderSet(s, n)
	case *luau.MixedTable:
		renderMixedTable(s, n)
	// statements
	case *luau.Assignment:
		renderAssignment(s, n)
	case *luau.BreakStatement:
		s.write("break")
		s.newline()
	case *luau.CallStatement:
		renderCallStatement(s, n)
	case *luau.ContinueStatement:
		s.write("continue")
		s.newline()
	case *luau.DoStatement:
		renderDoStatement(s, n)
	case *luau.WhileStatement:
		renderWhileStatement(s, n)
	case *luau.RepeatStatement:
		renderRepeatStatement(s, n)
	case *luau.IfStatement:
		renderIfStatement(s, n)
	case *luau.NumericForStatement:
		renderNumericForStatement(s, n)
	case *luau.ForStatement:
		renderForStatement(s, n)
	case *luau.FunctionDeclaration:
		renderFunctionDeclaration(s, n)
	case *luau.MethodDeclaration:
		renderMethodDeclaration(s, n)
	case *luau.VariableDeclaration:
		renderVariableDeclaration(s, n)
	case *luau.ReturnStatement:
		renderReturnStatement(s, n)
	case *luau.Comment:
		renderComment(s, n)
	// fields
	case *luau.MapField:
		renderMapField(s, n)
	case *luau.InterpolatedStringPart:
		renderInterpolatedStringPart(s, n)
	default:
		panic("render: no renderer for " + node.Kind().String())
	}
}

// RenderAST mirrors upstream renderAST(): solve temp ids, then render.
func RenderAST(ast *luau.List[luau.Statement]) string {
	s := NewRenderState()
	solveTempIDs(s, ast)
	s.statements(ast)
	return s.writer.String()
}
