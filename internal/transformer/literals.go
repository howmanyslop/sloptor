package transformer

import (
	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/scanner"
)

// getText ports ts.Node.getText(): the raw source slice from the token start
// (leading trivia skipped) to the node end.
func getText(s *State, node *ast.Node) string {
	return scanner.GetTextOfNodeFromSourceText(s.SourceFileText, node, false /*includeTrivia*/)
}

// transformNumericLiteral ports expressions/transformNumericLiteral.ts: the
// raw source text passes through untouched (hex stays `0xff`, binary stays
// `0b101`, separators stay `1_000_000`); the luau renderer owns any
// normalization.
func transformNumericLiteral(s *State, node *ast.Node) luau.Expression {
	return luau.NewNumberLiteral(getText(s, node))
}

// createStringFromLiteral ports util/createStringFromLiteral.ts: slice the
// raw getText() instead of using node.text, because the cooked text converts
// `\\n` to `\n` — escapes must be preserved verbatim.
func createStringFromLiteral(s *State, node *ast.Node) string {
	text := getText(s, node)
	switch node.Kind {
	case ast.KindStringLiteral, ast.KindNoSubstitutionTemplateLiteral:
		// ts.stripQuotes: drop the surrounding quote/backtick pair.
		return text[1 : len(text)-1]
	case ast.KindTemplateHead:
		// remove starting ` and ending ${
		return text[1 : len(text)-2]
	case ast.KindTemplateMiddle:
		// remove starting } and ending ${
		return text[1 : len(text)-2]
	case ast.KindTemplateTail:
		// remove starting } and ending `
		return text[1 : len(text)-1]
	}
	return text
}

// transformStringLiteral ports expressions/transformStringLiteral.ts. Quote
// style selection ("double" vs 'single') happens in the luau string renderer;
// the transformer stores the raw inner text.
func transformStringLiteral(s *State, node *ast.Node) luau.Expression {
	return luau.Str(createStringFromLiteral(s, node))
}

// transformInterpolatedStringPart ports nodes/transformInterpolatedStringPart.ts.
// The transformer passes the raw source text; the renderer escapes braces and
// newlines for the Luau backtick syntax (do not double-escape here).
func transformInterpolatedStringPart(s *State, node *ast.Node) *luau.InterpolatedStringPart {
	return luau.NewInterpolatedStringPart(createStringFromLiteral(s, node))
}

// transformNoSubstitutionTemplateLiteral ports
// expressions/transformNoSubstitutionTemplateLiteral.ts: backtick strings
// without `${}` stay as Luau interpolated strings (still valid Luau).
func transformNoSubstitutionTemplateLiteral(s *State, node *ast.Node) luau.Expression {
	parts := luau.NewList[luau.Node](luau.Node(transformInterpolatedStringPart(s, node)))
	return luau.NewInterpolatedString(parts)
}

// transformTemplateExpression ports expressions/transformTemplateExpression.ts:
// TS template strings ALWAYS become Luau InterpolatedString (no concatenation
// fallback in v3). Parts with empty cooked text are skipped; emitted text uses
// the raw getText() slices.
func transformTemplateExpression(s *State, node *ast.Node) luau.Expression {
	expression := node.AsTemplateExpression()
	parts := luau.NewList[luau.Node]()

	if len(expression.Head.Text()) > 0 {
		parts.Push(transformInterpolatedStringPart(s, expression.Head))
	}

	spans := expression.TemplateSpans.Nodes
	spanExpressions := make([]*ast.Node, len(spans))
	for i, span := range spans {
		spanExpressions[i] = span.AsTemplateSpan().Expression
	}
	orderedExpressions := ensureTransformOrder(s, spanExpressions)

	for i, span := range spans {
		parts.Push(orderedExpressions[i])
		literal := span.AsTemplateSpan().Literal
		if len(literal.Text()) > 0 {
			parts.Push(transformInterpolatedStringPart(s, literal))
		}
	}

	return luau.NewInterpolatedString(parts)
}

// transformTaggedTemplateExpression ports
// expressions/transformTaggedTemplateExpression.ts: the tag is called with an
// array of raw string segments followed by the ordered substitution
// expressions.
func transformTaggedTemplateExpression(s *State, node *ast.Node) luau.Expression {
	tagged := node.AsTaggedTemplateExpression()
	tagExp := TransformExpression(s, tagged.Tag)

	var stringParts []luau.Expression
	var args []luau.Expression

	if ast.IsTemplateExpression(tagged.Template) {
		template := tagged.Template.AsTemplateExpression()
		stringParts = append(stringParts, luau.Str(template.Head.Text()))

		spans := template.TemplateSpans.Nodes
		expressions := make([]*ast.Node, len(spans))
		for i, span := range spans {
			stringParts = append(stringParts, luau.Str(span.AsTemplateSpan().Literal.Text()))
			expressions[i] = span.AsTemplateSpan().Expression
		}
		args = ensureTransformOrder(s, expressions)
	} else {
		stringParts = append(stringParts, luau.Str(tagged.Template.Text()))
	}

	callArgs := append([]luau.Expression{
		luau.NewArray(luau.NewList(stringParts...)),
	}, args...)
	return luau.NewCall(convertToIndexableExpression(tagExp), luau.NewList(callArgs...))
}

// transformArrayLiteralExpression ports
// expressions/transformArrayLiteralExpression.ts. Non-spread fast path:
// inline `{ e1, e2, ... }`. With spreads, an ArrayPointer materializes into
// `local _array = { ...so far }` + `local _length = #_array` at the first
// spread (or at the first element whose transform produced prereqs), spreads
// append through the shared addIterableToArrayBuilder machinery, and later
// inline elements assign to `_array[_length + n]`. `_length` is only
// re-synced (`_length += <spread size>`) when more elements follow a spread;
// inline elements between updates are counted by amtElementsSinceUpdate
// (which spreads do NOT reset — verbatim upstream).
func transformArrayLiteralExpression(s *State, node *ast.Node) luau.Expression {
	elements := node.AsArrayLiteralExpression().Elements.Nodes
	hasSpread := false
	for _, element := range elements {
		if ast.IsSpreadElement(element) {
			hasSpread = true
			break
		}
	}
	if !hasSpread {
		members := luau.NewList[luau.Expression](ensureTransformOrder(s, elements)...)
		return luau.NewArray(members)
	}

	ptr := CreateArrayPointer("array")
	lengthID := luau.TempID("length")
	lengthInitialized := false
	amtElementsSinceUpdate := 0

	updateLengthID := func() {
		right := luau.NewUnary("#", ptr.Value)
		if lengthInitialized {
			s.Prereq(luau.NewAssignment(lengthID, "=", right))
		} else {
			s.Prereq(luau.NewVariableDeclaration(lengthID, right))
			lengthInitialized = true
		}
		amtElementsSinceUpdate = 0
	}

	for i, element := range elements {
		if ast.IsSpreadElement(element) {
			if _, isArray := ptr.Value.(*luau.Array); isArray {
				DisableArrayInline(s, ptr)
				updateLengthID()
			}
			arrayID, ok := ptr.Value.(luau.AnyIdentifier)
			if !ok {
				panic("transformer: transformArrayLiteralExpression: pointer is not an identifier") // upstream assert
			}

			spreadExpression := element.AsSpreadElement().Expression
			t := s.GetType(spreadExpression)
			builder := getAddIterableToArrayBuilder(s, spreadExpression, t)
			spreadExp := TransformExpression(s, spreadExpression)
			shouldUpdateLengthID := i < len(elements)-1
			s.PrereqList(builder(s, spreadExp, arrayID, lengthID, amtElementsSinceUpdate, shouldUpdateLengthID))
		} else {
			expression, prereqs := s.Capture(func() luau.Expression { return TransformExpression(s, element) })
			if _, isArray := ptr.Value.(*luau.Array); isArray && prereqs.IsNonEmpty() {
				DisableArrayInline(s, ptr)
				updateLengthID()
			}
			if array, isArray := ptr.Value.(*luau.Array); isArray {
				array.Members.Push(expression)
			} else {
				s.PrereqList(prereqs)
				s.Prereq(luau.NewAssignment(
					luau.NewComputedIndex(
						ptr.Value.(luau.IndexableExpression),
						luau.NewBinary(lengthID, "+", luau.Num(float64(amtElementsSinceUpdate+1))),
					),
					"=",
					expression,
				))
			}
			amtElementsSinceUpdate++
		}
	}

	return ptr.Value
}

// transformObjectLiteralExpression ports
// expressions/transformObjectLiteralExpression.ts: walk the properties
// building a MapPointer — inline `{ ... }` until a member's transform
// produces prereqs, then materialize a temp and emit per-field assignments.
func transformObjectLiteralExpression(s *State, node *ast.Node) luau.Expression {
	ptr := CreateMapPointer("object")
	for _, property := range node.AsObjectLiteralExpression().Properties.Nodes {
		validateMethodAssignment(s, property)
		switch property.Kind {
		case ast.KindPropertyAssignment:
			name := property.Name()
			if name.Kind == ast.KindPrivateIdentifier {
				s.Diags.Add(DiagNoPrivateIdentifier(name))
				continue
			}
			transformPropertyAssignment(s, ptr, name, property.AsPropertyAssignment().Initializer)
			s.setMapFieldSourcePosition(ptr, property)
		case ast.KindShorthandPropertyAssignment:
			// `{ a }` — the name doubles as the initializer expression.
			name := property.Name()
			transformPropertyAssignment(s, ptr, name, name)
			s.setMapFieldSourcePosition(ptr, property)
		case ast.KindSpreadAssignment:
			transformSpreadAssignment(s, ptr, property)
		case ast.KindMethodDeclaration:
			s.PrereqList(transformMethodDeclaration(s, property, ptr))
			s.setMapFieldSourcePosition(ptr, property)
		default:
			// must be an AccessorDeclaration, which is banned
			s.Diags.Add(DiagNoGetterSetter(property))
		}
	}
	return ptr.Value
}

func (state *State) setMapFieldSourcePosition(ptr *MapPointer, property *ast.Node) {
	if state.SourceMapEnabled {
		if table, ok := ptr.Value.(*luau.Map); ok && table.Fields.Tail != nil {
			state.setSourcePosition(table.Fields.Tail.Value, state.getOriginalSourcePosition(property, false))
		}
	}
}

// transformPropertyAssignment ports transformObjectLiteralExpression.ts
// transformPropertyAssignment: if either side produced prereqs the inline
// table can no longer hold the field (evaluation order would change), so the
// pointer degrades to a temp and the key is pinned.
func transformPropertyAssignment(s *State, ptr *MapPointer, name *ast.Node, initializer *ast.Node) {
	left, leftPrereqs := s.Capture(func() luau.Expression { return transformPropertyName(s, name) })
	right, rightPrereqs := s.Capture(func() luau.Expression { return TransformExpression(s, initializer) })

	if leftPrereqs.IsNonEmpty() || rightPrereqs.IsNonEmpty() {
		DisableMapInline(s, ptr)
		s.PrereqList(leftPrereqs)
		left = s.PushToVar(left, "left")
	}

	s.PrereqList(rightPrereqs)

	AssignToMapPointer(s, ptr, left, right)
}

// transformPropertyName ports nodes/transformPropertyName.ts: a plain
// identifier key `a` becomes the string "a"; a computed key `[a]` transforms
// `name.expression`; string/number literal names transform directly. (The
// luau map renderer chooses `a = v` vs `["a"] = v` per index shape.)
func transformPropertyName(s *State, name *ast.Node) luau.Expression {
	if ast.IsIdentifier(name) {
		return luau.Str(name.Text())
	}
	if ast.IsComputedPropertyName(name) {
		return TransformExpression(s, name.AsComputedPropertyName().Expression)
	}
	return TransformExpression(s, name)
}

// transformParenthesizedExpression ports
// expressions/transformParenthesizedExpression.ts: unwrap, transform, and
// keep Luau parens only when the result is not simple.
func transformParenthesizedExpression(s *State, node *ast.Node) luau.Expression {
	expression := TransformExpression(s, SkipDownwards(node.AsParenthesizedExpression().Expression))
	if luau.IsSimple(expression) {
		return expression
	}
	return luau.NewParenthesized(expression)
}
