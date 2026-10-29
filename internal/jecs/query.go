package jecs

import (
	"strings"

	"rotor/tsgo/ast"
)

// findMatchingChild keeps the LAST match in a pre-order walk that does not
// descend into a match, which is what the JavaScript version's
// `forEachChild` callback (always returning undefined) amounts to.
func findMatchingChild(node *ast.Node, predicate func(*ast.Node) bool) *ast.Node {
	var found *ast.Node
	var visit func(child *ast.Node) bool
	visit = func(child *ast.Node) bool {
		if predicate(child) {
			found = child
			return false
		}
		child.ForEachChild(visit)
		return false
	}
	node.ForEachChild(visit)
	return found
}

func (s *state) staticDeclarations(node *ast.Node, canHaveCalls bool) []*ast.Node {
	switch {
	case ast.IsIdentifier(node):
		symbol := s.checker.GetSymbolAtLocation(node)
		if symbol == nil || len(symbol.Declarations) == 0 {
			return nil
		}
		statement := symbol.Declarations[0]
		for !ast.IsStatement(statement) {
			// function parameters are considered dynamic
			if ast.IsParameterDeclaration(statement) {
				return nil
			}
			// if the variable isn't initialized it is considered dynamic
			if ast.IsVariableDeclaration(statement) && statement.Initializer() == nil {
				return nil
			}
			statement = statement.Parent
			if statement == nil {
				return nil
			}
		}
		if statement.Parent != s.currentCacheNode() && statement.Parent.Kind != ast.KindSourceFile {
			return nil
		}
		return []*ast.Node{statement}
	case ast.IsCallExpression(node) && canHaveCalls:
		call := node.AsCallExpression()
		expressionDeclarations := s.staticDeclarations(call.Expression, true)
		if len(expressionDeclarations) == 0 {
			return nil
		}
		var argumentDeclarations []*ast.Node
		for _, argument := range call.Arguments.Nodes {
			declarations := s.staticDeclarations(argument, true)
			if len(declarations) == 0 {
				return nil
			}
			argumentDeclarations = append(argumentDeclarations, declarations...)
		}
		if len(argumentDeclarations) == 0 {
			return nil
		}
		return append(expressionDeclarations, argumentDeclarations...)
	case ast.IsPropertyAccessExpression(node):
		access := node.AsPropertyAccessExpression()
		if ast.IsPrivateIdentifier(access.Name()) {
			return nil
		}
		nameDeclarations := s.staticDeclarations(access.Name(), true)
		if len(nameDeclarations) == 0 {
			return nil
		}
		expressionDeclarations := s.staticDeclarations(access.Expression, true)
		if len(expressionDeclarations) == 0 {
			return nil
		}
		return append(nameDeclarations, expressionDeclarations...)
	}
	return nil
}

func (s *state) staticCtToTypeNode(node *ast.Node) *ast.Node {
	f := s.factory
	switch {
	case ast.IsIdentifier(node):
		return f.NewTypeQueryNode(node, nil)
	case ast.IsPropertyAccessExpression(node):
		return f.NewTypeQueryNode(s.generateQualifiedName(node), nil)
	case ast.IsCallExpression(node):
		arguments := node.Arguments()
		if len(arguments) < 2 {
			fail("Unsupported query component type: %s", node.Kind)
		}
		return s.jecsType("Pair",
			s.jecsType("InferComponent", s.staticCtToTypeNode(arguments[0])),
			s.jecsType("InferComponent", s.staticCtToTypeNode(arguments[1])))
	}
	fail("Unsupported query component type: %s", node.Kind)
	return nil
}

func (s *state) generateQualifiedName(node *ast.Node) *ast.Node {
	access := node.AsPropertyAccessExpression()
	var left *ast.Node
	if ast.IsPropertyAccessExpression(access.Expression) {
		left = s.generateQualifiedName(access.Expression)
	} else {
		if !ast.IsIdentifier(access.Expression) {
			fail("expected an identifier")
		}
		left = access.Expression
	}
	if !ast.IsIdentifier(access.Name()) {
		fail("expected an identifier")
	}
	return s.factory.NewQualifiedName(left, access.Name())
}

type queryPlan struct {
	world        *ast.Node
	required     []*ast.Node
	excluded     []*ast.Node
	dontFragment bool
	inline       bool
}

// classifyQuery: jecs `.with` and `.without` replace their filter lists, so
// only the outermost of each counts. `inline` is false when a per-row
// archetype walk cannot match the jecs iterator.
func (s *state) classifyQuery(expression *ast.Node) *queryPlan {
	var withTerms, withoutTerms []*ast.Node
	withSeen, withoutSeen := false, false
	for ast.IsCallExpression(expression) && ast.IsPropertyAccessExpression(expression.Expression()) {
		callee := expression.Expression()
		symbol := s.checker.GetSymbolAtLocation(callee)
		if symbol != nil && genericSymbolsAreEqual(s.jecs.worldQuery, symbol) {
			return s.planQuery(callee.Expression(), expression.Arguments(), withTerms, withoutTerms)
		}
		if symbol != nil && genericSymbolsAreEqual(s.jecs.query.with, symbol) {
			if !withSeen {
				withTerms, withSeen = expression.Arguments(), true
			}
		} else if symbol != nil && genericSymbolsAreEqual(s.jecs.query.without, symbol) {
			if !withoutSeen {
				withoutTerms, withoutSeen = expression.Arguments(), true
			}
		}
		expression = callee.Expression()
	}
	return nil
}

func (s *state) planQuery(world *ast.Node, queryTerms, withTerms, withoutTerms []*ast.Node) *queryPlan {
	filter := func(terms []*ast.Node, keep bool) []*ast.Node {
		var out []*ast.Node
		for _, term := range terms {
			if s.isDontFragment(term) == keep {
				out = append(out, term)
			}
		}
		return out
	}
	required := filter(withTerms, true)
	excluded := filter(withoutTerms, true)
	queriedDontFragment := filter(queryTerms, true)
	fragmenting := filter(append(append([]*ast.Node{}, queryTerms...), withTerms...), false)
	inline := len(fragmenting) > 0
	if inline {
		for _, term := range append(append(append([]*ast.Node{}, queryTerms...), withTerms...), withoutTerms...) {
			if s.hasWildcardRelation(term) {
				inline = false
				break
			}
		}
	}
	if inline {
		for _, term := range queriedDontFragment {
			if s.hasWildcardTarget(term) {
				inline = false
				break
			}
		}
	}
	return &queryPlan{
		world:        world,
		required:     required,
		excluded:     excluded,
		dontFragment: len(required)+len(excluded)+len(queriedDontFragment) > 0,
		inline:       inline,
	}
}

func (s *state) hasWildcardRelation(term *ast.Node) bool {
	return ast.IsCallExpression(term) && len(term.Arguments()) > 0 && s.resolvesTo(term.Arguments()[0], s.jecs.wildcard)
}

func (s *state) hasWildcardTarget(term *ast.Node) bool {
	return ast.IsCallExpression(term) && len(term.Arguments()) > 1 && s.resolvesTo(term.Arguments()[1], s.jecs.wildcard)
}

func (s *state) resolvedSymbol(node *ast.Node) *ast.Symbol {
	symbol := s.checker.GetSymbolAtLocation(node)
	if symbol != nil && symbol.Flags&ast.SymbolFlagsAlias != 0 {
		return s.checker.GetAliasedSymbol(symbol)
	}
	return symbol
}

func (s *state) resolvesTo(node *ast.Node, target *ast.Symbol) bool {
	symbol := s.resolvedSymbol(node)
	return symbol != nil && genericSymbolsAreEqual(symbol, target)
}

// isDontFragment: a component registered with `{ dontFragment: true }`, an
// alias of one, or a pair over one.
func (s *state) isDontFragment(term *ast.Node) bool {
	// a pair takes DontFragment from its relation
	if ast.IsCallExpression(term) {
		return len(term.Arguments()) > 0 && s.isDontFragment(term.Arguments()[0])
	}
	symbol := s.resolvedSymbol(term)
	if symbol == nil {
		return false
	}
	declaration := symbol.ValueDeclaration
	if declaration == nil || !ast.IsVariableDeclaration(declaration) || declaration.Initializer() == nil {
		return false
	}
	initializer := declaration.Initializer()
	if ast.IsIdentifier(initializer) || ast.IsPropertyAccessExpression(initializer) {
		return initializer != term && s.isDontFragment(initializer)
	}
	if !ast.IsCallExpression(initializer) {
		return false
	}
	for _, argument := range initializer.Arguments() {
		if !ast.IsObjectLiteralExpression(argument) {
			continue
		}
		for _, property := range argument.AsObjectLiteralExpression().Properties.Nodes {
			if ast.IsPropertyAssignment(property) &&
				ast.IsIdentifier(property.Name()) &&
				property.Name().Text() == "dontFragment" &&
				property.Initializer().Kind == ast.KindTrueKeyword {
				return true
			}
		}
	}
	return false
}

func (s *state) storedInitializer(identifier *ast.Node) *ast.Node {
	symbol := s.checker.GetSymbolAtLocation(identifier)
	if symbol == nil {
		return identifier
	}
	declaration := symbol.ValueDeclaration
	if declaration == nil || !ast.IsVariableDeclaration(declaration) {
		return identifier
	}
	if initializer := declaration.Initializer(); initializer != nil {
		return initializer
	}
	return identifier
}

// assertNoDontFragment: the archetype walk skips DontFragment probes for this
// query, so a term the static scan missed would silently match nothing; jecs
// flags it as `df`.
func (s *state) assertNoDontFragment(query, queryExpression *ast.Node) *ast.Node {
	f := s.factory
	internals := f.NewAsExpression(f.NewAsExpression(query, f.NewKeywordTypeNode(ast.KindUnknownKeyword)), s.jecsType("QueryInternals"))
	source := queryExpression
	if ast.IsIdentifier(queryExpression) {
		source = s.storedInitializer(queryExpression)
	}
	message := "query has a DontFragment term the transformer did not detect: " + strings.Join(strings.Fields(nodeText(source)), " ")
	return f.NewExpressionStatement(f.NewCallExpression(f.NewIdentifier("assert"), nil, nil, f.NewNodeList([]*ast.Node{
		f.NewStrictEqualityExpression(f.NewPropertyAccessExpression(internals, nil, f.NewIdentifier("df"), ast.NodeFlagsNone), f.NewIdentifier("undefined")),
		f.NewStringLiteral(message, 0),
	}), ast.NodeFlagsNone))
}

func (s *state) isQueryCreation(node *ast.Node) bool {
	return ast.IsCallExpression(node) && s.checker.GetSymbolAtLocation(node.Expression()) == s.jecs.worldQuery
}

// cacheQuery returns the identifier replacing a cacheable query, or nil to
// leave the call to the ordinary visit.
func (s *state) cacheQuery(expression *ast.Node) *ast.Node {
	f := s.factory
	if strings.Contains(s.leadingTrivia(expression), "no-cache") {
		return nil
	}
	signature := s.checker.GetResolvedSignature(expression)
	if signature == nil {
		return nil
	}
	if s.checker.GetReturnTypeOfSignature(signature).Alias().Symbol() != s.jecs.query.symbol {
		return nil
	}
	queryCreation := expression
	if !s.isQueryCreation(expression) {
		queryCreation = findMatchingChild(expression, s.isQueryCreation)
	}
	if queryCreation == nil {
		return nil
	}
	queryExpression := queryCreation
	for queryExpression.Parent != nil && ast.IsPropertyAccessExpression(queryExpression.Parent) &&
		queryExpression.Parent.Parent != nil && ast.IsCallExpression(queryExpression.Parent.Parent) {
		symbol := s.checker.GetSymbolAtLocation(queryExpression.Parent)
		// if the query is manually cached, do not cache it again
		if symbol != nil && genericSymbolsAreEqual(symbol, s.jecs.query.cached) {
			return nil
		}
		queryExpression = queryExpression.Parent.Parent
	}
	statement := queryExpression
	for !ast.IsStatement(statement) {
		statement = statement.Parent
		if statement == nil {
			return nil
		}
	}
	if !ast.IsForOfStatement(statement) && !ast.IsIfStatement(statement) {
		return nil
	}
	plan := s.classifyQuery(queryExpression)
	if plan == nil || !plan.inline {
		return nil
	}
	valid, fault, componentDeclarations, queryComponents := s.parseQuery(queryExpression)
	if !valid {
		if !s.config.Silent {
			s.warn("'" + nodeText(fault) + "' isn't simple. Not caching query: \n\t" + nodeText(expression))
		}
		return nil
	}
	world := queryCreation.Expression()
	if !ast.IsPropertyAccessExpression(world) {
		s.warn("Unsupported world.query access type: " + world.Kind.String())
		return nil
	}
	world = world.Expression()
	worldDeclarations := s.staticDeclarations(world, true)
	fileLevel := false
	for _, declaration := range worldDeclarations {
		if declaration.Parent.Kind == ast.KindSourceFile {
			fileLevel = true
			break
		}
	}
	var c *cache
	if fileLevel {
		c = s.fileCache(world)
	} else {
		c = s.currentCache(world)
	}
	c.require(worldDeclarations)
	c.require(componentDeclarations)

	queryIdentifier := f.NewUniqueName("query")
	archetypesIdentifier := f.NewUniqueName("archetypes")
	s.cachedQueries[queryIdentifier] = cachedQuery{archetypes: archetypesIdentifier, components: queryComponents}
	expressionCached := f.NewCallExpression(f.NewPropertyAccessExpression(expression, nil, f.NewIdentifier("cached"), ast.NodeFlagsNone), nil, nil, f.NewNodeList(nil), ast.NodeFlagsNone)
	queryArchetypes := f.NewCallExpression(f.NewPropertyAccessExpression(queryIdentifier, nil, f.NewIdentifier("archetypes"), ast.NodeFlagsNone), nil, nil, f.NewNodeList(nil), ast.NodeFlagsNone)
	queryRange := s.textRangeOf(queryCreation)
	if len(worldDeclarations) > 0 {
		queryDeclaration := f.NewVariableDeclaration(queryIdentifier, nil, nil, expressionCached)
		archetypesDeclaration := f.NewVariableDeclaration(archetypesIdentifier, nil, nil, queryArchetypes)
		c.outerResult(anchored(f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{queryDeclaration, archetypesDeclaration}), ast.NodeFlagsConst)), queryRange))
		if !plan.dontFragment && ast.IsForOfStatement(statement) {
			c.outerResult(anchored(s.assertNoDontFragment(queryIdentifier, queryExpression), queryRange))
		}
	} else {
		componentTypes := make([]*ast.Node, len(queryComponents))
		for index, component := range queryComponents {
			componentTypes[index] = s.staticCtToTypeNode(component)
		}
		queryDeclaration := f.NewVariableDeclaration(queryIdentifier, nil, s.jecsType("CachedQuery", f.NewTupleTypeNode(f.NewNodeList(componentTypes))), nil)
		archetypesDeclaration := f.NewVariableDeclaration(archetypesIdentifier, nil, f.NewTypeReferenceNode(f.NewIdentifier("ReturnType"), f.NewNodeList([]*ast.Node{
			f.NewIndexedAccessTypeNode(f.NewTypeQueryNode(queryIdentifier, nil), f.NewLiteralTypeNode(f.NewStringLiteral("archetypes", 0))),
		})), nil)
		c.outerResult(anchored(f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{queryDeclaration, archetypesDeclaration}), ast.NodeFlagsLet)), queryRange))
		c.newGroup(queryRange)
		c.conditioned(world)
		c.innerResult(
			anchored(f.NewExpressionStatement(f.NewAssignmentExpression(queryIdentifier, expressionCached)), queryRange),
			anchored(f.NewExpressionStatement(f.NewAssignmentExpression(archetypesIdentifier, queryArchetypes)), queryRange),
		)
		if !plan.dontFragment && ast.IsForOfStatement(statement) {
			c.innerResult(anchored(s.assertNoDontFragment(queryIdentifier, queryExpression), queryRange))
		}
	}
	return queryIdentifier
}

func (s *state) parseQuery(expression *ast.Node) (valid bool, fault *ast.Node, declarations, queryComponents []*ast.Node) {
	symbols := []*ast.Symbol{s.jecs.worldQuery, s.jecs.query.with, s.jecs.query.without}
	var visit func(node *ast.Node) bool
	visit = func(node *ast.Node) bool {
		if ast.IsCallExpression(node) {
			symbol := s.checker.GetSymbolAtLocation(node.Expression())
			container := false
			if symbol != nil {
				for _, candidate := range symbols {
					if genericSymbolsAreEqual(candidate, symbol) {
						container = true
						break
					}
				}
			}
			if container {
				for _, component := range node.Arguments() {
					found := s.staticDeclarations(component, true)
					if len(found) == 0 {
						fault = component
					}
					declarations = append(declarations, found...)
					if symbol == s.jecs.worldQuery {
						queryComponents = append(queryComponents, component)
					}
				}
			}
		}
		return node.ForEachChild(visit)
	}
	visit(expression)
	if fault != nil {
		return false, fault, nil, queryComponents
	}
	return true, nil, declarations, queryComponents
}
