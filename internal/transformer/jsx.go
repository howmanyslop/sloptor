package transformer

// This file ports the complete roblox-ts 3.0 JSX surface:
//   nodes/expressions/transformJsxElement.ts
//   nodes/expressions/transformJsxSelfClosingElement.ts
//   nodes/expressions/transformJsxFragment.ts
//   nodes/expressions/transformJsxExpression.ts
//   nodes/jsx/transformJsx.ts
//   nodes/jsx/transformJsxTagName.ts
//   nodes/jsx/transformJsxAttributes.ts
//   nodes/jsx/transformJsxChildren.ts
// (util/fixupWhitespaceAndDecodeEntities.ts lives in jsxtext.go.)

import (
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/scanner"
)

// transformJsxElement ports transformJsxElement.ts (L5-7).
func transformJsxElement(s *State, node *ast.Node) luau.Expression {
	element := node.AsJsxElement()
	opening := element.OpeningElement.AsJsxOpeningElement()
	return transformJsx(s, node, opening.TagName, opening.Attributes, element.Children.Nodes)
}

// transformJsxSelfClosingElement ports transformJsxSelfClosingElement.ts
// (L5-7): no children.
func transformJsxSelfClosingElement(s *State, node *ast.Node) luau.Expression {
	element := node.AsJsxSelfClosingElement()
	return transformJsx(s, node, element.TagName, element.Attributes, nil)
}

// transformJsx ports nodes/jsx/transformJsx.ts (L12-46): the single factory-
// call assembler `<factory>(tag, attrs-map-or-nil, ...children)`. The nil
// placeholder appears ONLY when there are children and no attributes.
func transformJsx(s *State, node *ast.Node, tagName *ast.Node, attributes *ast.Node, children []*ast.Node) luau.Expression {
	// jsxFactoryEntity seems to always be defined and will default to `React.createElement`
	jsxFactoryEntity := s.EmitResolver().GetJsxFactoryEntityUnsafe(node)
	if jsxFactoryEntity == nil {
		panic("Expected jsxFactoryEntity to be defined") // upstream assert
	}

	createElementExpression := convertToIndexableExpression(transformEntityName(s, jsxFactoryEntity))

	tagNameExp := transformJsxTagName(s, tagName)

	var attributesPtr *MapPointer
	if len(attributes.AsJsxAttributes().Properties.Nodes) > 0 {
		attributesPtr = CreateMapPointer("attributes")
		transformJsxAttributes(s, attributes, attributesPtr)
	}

	transformedChildren := transformJsxChildren(s, children)

	args := luau.NewList[luau.Expression](tagNameExp)

	if attributesPtr != nil {
		args.Push(attributesPtr.Value)
	} else if len(transformedChildren) > 0 {
		args.Push(luau.Nil())
	}

	for _, child := range transformedChildren {
		args.Push(child)
	}

	return luau.NewCall(createElementExpression, args)
}

// transformJsxFragment ports transformJsxFragment.ts (L9-34). Fragments never
// have attributes, so they ALWAYS push `nil` before children (vs the
// attrs-or-nil asymmetry in transformJsx).
func transformJsxFragment(s *State, node *ast.Node) luau.Expression {
	jsxFactoryEntity := s.EmitResolver().GetJsxFactoryEntityUnsafe(node)
	if jsxFactoryEntity == nil {
		panic("Expected jsxFactoryEntity to be defined") // upstream assert
	}

	createElementExpression := convertToIndexableExpression(transformEntityName(s, jsxFactoryEntity))

	// getJsxFragmentFactoryEntity() doesn't seem to default to "Fragment"..
	// but the typechecker does, so we should follow that behavior
	jsxFragmentFactoryEntity := s.EmitResolver().GetJsxFragmentFactoryEntityUnsafe(node)
	if jsxFragmentFactoryEntity == nil {
		// upstream ts.parseIsolatedEntityName("Fragment", ESNext); tsgo's
		// ParseIsolatedEntityName does not synthesize positions, so mirror
		// checker.markAsSynthetic (tsgo/checker/jsx.go:1444-1448) to hit
		// TransformIdentifier's synthetic early-out.
		jsxFragmentFactoryEntity = parser.ParseIsolatedEntityName("Fragment")
		if jsxFragmentFactoryEntity != nil {
			markEntityAsSynthetic(jsxFragmentFactoryEntity)
		}
	}
	if jsxFragmentFactoryEntity == nil {
		panic("Unable to find valid jsxFragmentFactoryEntity") // upstream assert
	}

	// NOT convertToIndexableExpression — the fragment factory is an argument.
	args := luau.NewList[luau.Expression](transformEntityName(s, jsxFragmentFactoryEntity))

	transformedChildren := transformJsxChildren(s, node.AsJsxFragment().Children.Nodes)

	// props parameter
	if len(transformedChildren) > 0 {
		args.Push(luau.Nil())
	}

	for _, child := range transformedChildren {
		args.Push(child)
	}

	return luau.NewCall(createElementExpression, args)
}

// markEntityAsSynthetic mirrors tsgo checker.markAsSynthetic
// (tsgo/checker/jsx.go:1444-1448): recursively set Loc = (-1, -1) so
// PositionIsSynthesized holds for every node of the parsed entity.
func markEntityAsSynthetic(node *ast.Node) bool {
	node.Loc = core.NewTextRange(-1, -1)
	node.ForEachChild(markEntityAsSynthetic)
	return false
}

// transformJsxExpression ports transformJsxExpression.ts (L6-15): a `{expr}` /
// `{...expr}` child in expression position. Note global `unpack`, not
// `table.unpack` (oracle digest §3 case 10).
func transformJsxExpression(s *State, node *ast.Node) luau.Expression {
	expression := node.AsJsxExpression()
	if expression.Expression != nil {
		exp := TransformExpression(s, expression.Expression)
		if expression.DotDotDotToken != nil {
			return luau.NewCall(luau.GlobalID("unpack"), luau.NewList[luau.Expression](exp))
		}
		return exp
	}
	return luau.NewNone()
}

// ---------------------------------------------------------------------------
// Tag names — nodes/jsx/transformJsxTagName.ts
// ---------------------------------------------------------------------------

// getTextOfJsxNamespacedName ports ts.getTextOfJsxNamespacedName:
// `namespace ":" name` (cf. tsgo/ast/utilities.go:2108-2109).
func getTextOfJsxNamespacedName(node *ast.Node) string {
	namespaced := node.AsJsxNamespacedName()
	return namespaced.Namespace.Text() + ":" + namespaced.Name().Text()
}

// transformJsxTagNameExpression ports transformJsxTagNameExpression (L9-28).
func transformJsxTagNameExpression(s *State, node *ast.Node) luau.Expression {
	if ast.IsIdentifier(node) && scanner.IsIntrinsicJsxName(node.Text()) {
		return luau.Str(node.Text())
	}

	if ast.IsPropertyAccessExpression(node) {
		if ast.IsPrivateIdentifier(node.Name()) {
			s.Diags.Add(DiagNoPrivateIdentifier(node.Name()))
		}
		return luau.NewPropertyAccess(convertToIndexableExpression(TransformExpression(s, node.Expression())), node.Name().Text())
	} else if ast.IsJsxNamespacedName(node) {
		return luau.Str(getTextOfJsxNamespacedName(node))
	} else {
		return TransformExpression(s, node)
	}
}

// transformJsxTagName ports transformJsxTagName (L30-38): only when the tag
// expression captured prereqs does it get pinned (`tagName` temp).
func transformJsxTagName(s *State, tagName *ast.Node) luau.Expression {
	expression, prereqs := s.Capture(func() luau.Expression {
		return transformJsxTagNameExpression(s, tagName)
	})
	tagNameExp := expression
	if prereqs.IsNonEmpty() {
		s.PrereqList(prereqs)
		tagNameExp = s.PushToVarIfComplex(tagNameExp, "tagName")
	}
	return tagNameExp
}

// ---------------------------------------------------------------------------
// Attributes — nodes/jsx/transformJsxAttributes.ts
// ---------------------------------------------------------------------------

// createJsxAttributeLoop ports createJsxAttributeLoop (L11-48): the generic
// spread merge `for _k, _v in <exp> do _attributes[_k] = _v end`, wrapped in a
// truthiness `if` (and with the expression pinned to an `attribute` temp) when
// the spread operand is not definitely an object.
func createJsxAttributeLoop(s *State, attributesPtrValue luau.AnyIdentifier, expression luau.Expression, tsExpression *ast.Node) luau.Statement {
	definitelyObject := IsDefinitelyType(s, s.GetType(tsExpression), IsObjectType, IsNonPrimitiveType)
	if !definitelyObject {
		expression = s.PushToVarIfComplex(expression, "attribute")
	}

	keyId := luau.TempID("k")
	valueId := luau.TempID("v")
	var statement luau.Statement = luau.NewFor(
		luau.NewList[luau.AnyIdentifier](keyId, valueId),
		expression,
		luau.NewList[luau.Statement](luau.NewAssignment(
			luau.NewComputedIndex(attributesPtrValue, keyId),
			"=",
			valueId,
		)),
	)

	if !definitelyObject {
		statement = luau.NewIf(
			CreateTruthinessChecks(s, expression, tsExpression, s.GetType(tsExpression)),
			luau.NewList[luau.Statement](statement),
			nil,
		)
	}

	return statement
}

// transformJsxAttribute ports transformJsxAttribute (L50-68).
func transformJsxAttribute(s *State, attribute *ast.Node, attributesPtr *MapPointer) {
	jsxAttribute := attribute.AsJsxAttribute()
	initializer := jsxAttribute.Initializer
	if initializer != nil && ast.IsJsxExpression(initializer) {
		// QUIRK (digest §7.3): `{}` initializer has Expression == nil, folding
		// into the implicit-true branch below — `<frame Visible={}/>` emits
		// `Visible = true`.
		initializer = initializer.AsJsxExpression().Expression
	}

	var init luau.Expression
	var initPrereqs *luau.List[luau.Statement]
	if initializer != nil {
		init, initPrereqs = s.Capture(func() luau.Expression {
			return TransformExpression(s, initializer)
		})
	} else {
		init, initPrereqs = luau.Bool(true), luau.NewList[luau.Statement]()
	}

	if initPrereqs.IsNonEmpty() {
		// disableMapInline BEFORE prereqList: `local _attributes = {…so far…}`
		// precedes the initializer's prereqs (digest §7.10, oracle §3 case C).
		DisableMapInline(s, attributesPtr)
		s.PrereqList(initPrereqs)
	}

	var text string
	if name := jsxAttribute.Name(); ast.IsIdentifier(name) {
		text = name.Text()
	} else {
		text = getTextOfJsxNamespacedName(name)
	}
	AssignToMapPointer(s, attributesPtr, luau.Str(text), init)
}

// transformJsxAttributes ports transformJsxAttributes (L70-103).
func transformJsxAttributes(s *State, attributes *ast.Node, attributesPtr *MapPointer) {
	properties := attributes.AsJsxAttributes().Properties.Nodes
	for _, attribute := range properties {
		if ast.IsJsxAttribute(attribute) {
			transformJsxAttribute(s, attribute, attributesPtr)
		} else {
			// spread attributes: `<frame { ...x }/>`

			tsExpression := attribute.AsJsxSpreadAttribute().Expression
			expType := s.Checker.GetNonOptionalType(s.GetType(tsExpression))
			symbol := GetFirstDefinedSymbol(s, expType)
			if symbol != nil && s.Macros().IsMacroOnlyClass(symbol) {
				s.Diags.Add(DiagNoMacroObjectSpread(attribute))
			}

			expression := TransformExpression(s, tsExpression)

			if attribute == properties[0] && IsDefinitelyType(s, expType, IsObjectType, IsNonPrimitiveType) {
				// first property overall AND definitely an object: clone
				// fast path (digest §7.5).
				attributesPtr.Value = s.PushToVar(
					luau.NewCall(luau.GlobalProperty("table", "clone"), luau.NewList[luau.Expression](expression)),
					attributesPtr.Name,
				)
				// Explicitly remove metatable because things like classes can be spread
				s.Prereq(luau.NewCallStatement(
					luau.NewCall(luau.GlobalID("setmetatable"), luau.NewList[luau.Expression](attributesPtr.Value, luau.Nil())),
				))
				continue
			}

			DisableMapInline(s, attributesPtr)
			s.Prereq(createJsxAttributeLoop(s, attributesPtr.Value.(luau.AnyIdentifier), expression, tsExpression))
		}
	}
}

// ---------------------------------------------------------------------------
// Children — nodes/jsx/transformJsxChildren.ts
// ---------------------------------------------------------------------------

// isSignificantJsxChild reports `!isJsxText(child) || !containsOnlyTriviaWhiteSpaces`.
func isSignificantJsxChild(child *ast.Node) bool {
	return !ast.IsJsxText(child) || !child.AsJsxText().ContainsOnlyTriviaWhiteSpaces
}

// transformJsxChildren ports transformJsxChildren (L11-39). The spread-must-
// be-last scan runs on the UNFILTERED list for indices strictly below the last
// significant child (digest §7.7); the transform then drops whitespace-only
// JsxText and empty `{}` JsxExpressions and runs the standard
// ensureTransformOrder walk.
func transformJsxChildren(s *State, children []*ast.Node) []luau.Expression {
	if len(children) == 0 {
		// Self-closing elements pass nil children — the same empty result
		// the walk below would produce, without indexing into nothing.
		return []luau.Expression{}
	}
	// findLastIndex(children, child => !isJsxText(child) || !child.containsOnlyTriviaWhiteSpaces)
	lastJsxChildIndex := -1
	for i := len(children) - 1; i >= 0; i-- {
		if isSignificantJsxChild(children[i]) {
			lastJsxChildIndex = i
			break
		}
	}

	for i := 0; i < lastJsxChildIndex; i++ {
		child := children[i]
		if ast.IsJsxExpression(child) && child.AsJsxExpression().DotDotDotToken != nil {
			s.Diags.Add(DiagNoPrecedingJsxSpreadElement(child))
		}
	}

	filtered := make([]*ast.Node, 0, len(children))
	for _, child := range children {
		// ignore jsx text that only contains whitespace
		if !isSignificantJsxChild(child) {
			continue
		}
		// ignore empty jsx expressions, i.e. `{}`
		if ast.IsJsxExpression(child) && child.AsJsxExpression().Expression == nil {
			continue
		}
		filtered = append(filtered, child)
	}

	return ensureTransformOrderWith(s, filtered, func(s *State, node *ast.Node) luau.Expression {
		if ast.IsJsxText(node) {
			text := fixupWhitespaceAndDecodeEntities(node.AsJsxText().Text)
			// QUIRK (digest §7.2): double every backslash — luau-ast (and
			// rotor's renderStringLiteral) emit string values raw, so rbxtsc
			// writes `"back\\slash"` for source text `back\slash`.
			return luau.Str(strings.ReplaceAll(text, "\\", "\\\\"))
		}
		return TransformExpression(s, node)
	})
}
