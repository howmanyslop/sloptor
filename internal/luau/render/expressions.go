package render

import (
	"strings"

	"rotor/internal/luau"
)

// renderIdentifier.ts
func renderIdentifier(s *RenderState, node *luau.Identifier) string {
	if !luau.IsValidIdentifier(node.Name) {
		panic("Invalid Luau Identifier: \"" + node.Name + "\"")
	}
	return node.Name
}

// renderTemporaryIdentifier.ts
func renderTemporaryIdentifier(s *RenderState, node *luau.TemporaryIdentifier) string {
	name := s.getTempName(node)
	if !luau.IsValidIdentifier(name) {
		panic("Invalid Temporary Identifier: \"" + name + "\"")
	}
	return name
}

// renderComputedIndexExpression.ts
func renderComputedIndexExpression(s *RenderState, node *luau.ComputedIndexExpression) string {
	expStr := Render(s, node.Expression)
	if str, ok := node.Index.(*luau.StringLiteral); ok && luau.IsValidIdentifier(str.Value) {
		return expStr + "." + str.Value
	}
	return expStr + "[" + Render(s, node.Index) + "]"
}

// renderPropertyAccessExpression.ts
func renderPropertyAccessExpression(s *RenderState, node *luau.PropertyAccessExpression) string {
	expStr := Render(s, node.Expression)
	if luau.IsValidIdentifier(node.Name) {
		return expStr + "." + node.Name
	}
	return expStr + "[\"" + node.Name + "\"]"
}

// renderCallExpression.ts
func renderCallExpression(s *RenderState, node *luau.CallExpression) string {
	return Render(s, node.Expression) + "(" + renderArguments(s, node.Args) + ")"
}

// renderMethodCallExpression.ts
func renderMethodCallExpression(s *RenderState, node *luau.MethodCallExpression) string {
	if !luau.IsValidIdentifier(node.Name) {
		panic("invalid method name: " + node.Name)
	}
	return Render(s, node.Expression) + ":" + node.Name + "(" + renderArguments(s, node.Args) + ")"
}

// renderParenthesizedExpression.ts
func renderParenthesizedExpression(s *RenderState, node *luau.ParenthesizedExpression) string {
	// skip nested parentheses
	expression := node.Expression
	for {
		if p, ok := expression.(*luau.ParenthesizedExpression); ok {
			expression = p.Expression
		} else {
			break
		}
	}
	if luau.IsSimple(expression) {
		return Render(s, node.Expression)
	}
	return "(" + Render(s, node.Expression) + ")"
}

// renderNumberLiteral.ts
func renderNumberLiteral(s *RenderState, node *luau.NumberLiteral) string {
	if luau.IsValidNumberLiteral(node.Value) {
		return node.Value
	}
	// upstream: String(Number(node.value.replace(/_/g, "")))
	f, err := luau.JSNumberParse(node.Value)
	if err != nil {
		// JS Number(garbage) is NaN, and String(NaN) renders as "NaN".
		return "NaN"
	}
	return luau.JSNumberString(f)
}

// renderStringLiteral.ts
func needsBracketSpacing(node *luau.StringLiteral) bool {
	parent := node.Parent()
	if parent == nil {
		return false
	}
	switch p := parent.(type) {
	case *luau.MapField:
		return luau.Node(node) == luau.Node(p.Index)
	case *luau.ComputedIndexExpression:
		return luau.Node(node) == luau.Node(p.Index)
	case *luau.Set:
		return true
	}
	return false
}

func renderStringLiteral(s *RenderState, node *luau.StringLiteral) string {
	isMultiline := strings.Contains(node.Value, "\n")
	if !isMultiline && !strings.Contains(node.Value, "\"") {
		return "\"" + node.Value + "\""
	}
	if !isMultiline && !strings.Contains(node.Value, "'") {
		return "'" + node.Value + "'"
	}
	eqStr := getSafeBracketEquals(node.Value)
	spacing := ""
	if needsBracketSpacing(node) {
		spacing = " "
	}
	return spacing + "[" + eqStr + "[" + node.Value + "]" + eqStr + "]" + spacing
}

// renderFunctionExpression.ts
func renderFunctionExpression(s *RenderState, node *luau.FunctionExpression) string {
	if node.Statements.IsEmpty() {
		return "function(" + renderParameters(s, node) + ") end"
	}
	result := "function(" + renderParameters(s, node) + ")\n"
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Indented("end")
	return result
}

// renderBinaryExpression.ts
func renderBinaryExpression(s *RenderState, node *luau.BinaryExpression) string {
	result := Render(s, node.Left) + " " + string(node.Operator) + " " + Render(s, node.Right)
	if needsParentheses(node) {
		result = "(" + result + ")"
	}
	return result
}

// renderUnaryExpression.ts
func unaryNeedsSpace(node *luau.UnaryExpression) bool {
	// not always needs a space
	if node.Operator == "not" {
		return true
	}
	// "--" will create a comment!
	if inner, ok := node.Expression.(*luau.UnaryExpression); ok && inner.Operator == "-" {
		return true
	}
	return false
}

func renderUnaryExpression(s *RenderState, node *luau.UnaryExpression) string {
	opStr := string(node.Operator)
	if unaryNeedsSpace(node) {
		opStr += " "
	}
	result := opStr + Render(s, node.Expression)
	if needsParentheses(node) {
		result = "(" + result + ")"
	}
	return result
}

// renderIfExpression.ts
func renderIfExpression(s *RenderState, node *luau.IfExpression) string {
	result := "if " + Render(s, node.Condition) + " then " + Render(s, node.Expression) + " "
	currentAlternative := node.Alternative
	for {
		ifExp, ok := currentAlternative.(*luau.IfExpression)
		if !ok {
			break
		}
		result += "elseif " + Render(s, ifExp.Condition) + " then " + Render(s, ifExp.Expression) + " "
		currentAlternative = ifExp.Alternative
	}
	result += "else " + Render(s, currentAlternative)
	if needsParentheses(node) {
		result = "(" + result + ")"
	}
	return result
}

// renderInterpolatedString.ts
func renderInterpolatedString(s *RenderState, node *luau.InterpolatedString) string {
	var b strings.Builder
	b.WriteString("`")
	node.Parts.ForEach(func(part luau.Node) {
		expressionStr := Render(s, part)
		if _, isPart := part.(*luau.InterpolatedStringPart); isPart {
			b.WriteString(expressionStr)
		} else {
			b.WriteString("{")
			// `{{}}` is invalid, so we wrap it in parenthesis
			if luau.IsTable(part) {
				expressionStr = "(" + expressionStr + ")"
			}
			b.WriteString(expressionStr)
			b.WriteString("}")
		}
	})
	b.WriteString("`")
	return b.String()
}

// renderArray.ts
func renderArray(s *RenderState, node *luau.Array) string {
	if node.Members.IsEmpty() {
		return "{}"
	}
	parts := []string{}
	node.Members.ForEach(func(m luau.Expression) { parts = append(parts, Render(s, m)) })
	return "{ " + strings.Join(parts, ", ") + " }"
}

// renderMap.ts
func renderMap(s *RenderState, node *luau.Map) string {
	if node.Fields.IsEmpty() {
		return "{}"
	}
	result := "{\n"
	s.Block(func() string {
		node.Fields.ForEach(func(f *luau.MapField) { result += s.Line(Render(s, f) + ",") })
		return ""
	})
	result += s.Indented("}")
	return result
}

// renderSet.ts
func renderSet(s *RenderState, node *luau.Set) string {
	if node.Members.IsEmpty() {
		return "{}"
	}
	result := "{\n"
	s.Block(func() string {
		node.Members.ForEach(func(m luau.Expression) {
			result += s.Line("[" + Render(s, m) + "] = true,")
		})
		return ""
	})
	result += s.Indented("}")
	return result
}

// renderMixedTable.ts
func renderMixedTable(s *RenderState, node *luau.MixedTable) string {
	if node.Fields.IsEmpty() {
		return "{}"
	}
	result := "{\n"
	s.Block(func() string {
		node.Fields.ForEach(func(f luau.Node) { result += s.Line(Render(s, f) + ",") })
		return ""
	})
	result += s.Indented("}")
	return result
}
