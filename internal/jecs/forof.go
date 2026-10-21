package jecs

import (
	"rotor/tsgo/ast"
)

const (
	// jecs ECS_ENTITY_T_LO: an entity's low 24 bits key its sparse-pool slot
	entityKeyModulus = "16777216"
	// each sparse-pool map slot is a u32
	mapSlotBytes = "4"
)

type componentBinding struct {
	identifier *ast.Node
	component  *ast.Node
}

type componentRow struct {
	component *ast.Node
	list      *ast.Node
	statement *ast.Node
}

func (s *state) variableStatement(name, initializer *ast.Node, flags ast.NodeFlags) *ast.Node {
	f := s.factory
	return f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{
		f.NewVariableDeclaration(name, nil, nil, initializer),
	}), flags))
}

func (s *state) binary(left *ast.Node, operator ast.Kind, right *ast.Node) *ast.Node {
	f := s.factory
	return f.NewBinaryExpression(nil, left, nil, f.NewToken(operator), right)
}

func (s *state) numeric(text string) *ast.Node {
	return s.factory.NewNumericLiteral(text, 0)
}

func (s *state) access(expression *ast.Node, name string) *ast.Node {
	f := s.factory
	return f.NewPropertyAccessExpression(expression, nil, f.NewIdentifier(name), ast.NodeFlagsNone)
}

func (s *state) call(expression *ast.Node, arguments ...*ast.Node) *ast.Node {
	f := s.factory
	return f.NewCallExpression(expression, nil, nil, f.NewNodeList(arguments), ast.NodeFlagsNone)
}

// rowIndex is `row - 1`: roblox-ts adds 1 to an index, so this cancels it.
func (s *state) rowIndex(row *ast.Node) *ast.Node {
	return s.binary(row, ast.KindMinusToken, s.numeric("1"))
}

func (s *state) inlineQueryIterators(node *ast.Node) *ast.Node {
	f := s.factory
	loop := node.AsForInOrOfStatement()
	if !ast.IsVariableDeclarationList(loop.Initializer) {
		return nil
	}
	declarations := loop.Initializer.AsVariableDeclarationList().Declarations.Nodes
	if len(declarations) == 0 {
		return nil
	}
	declaration := declarations[0]
	if !ast.IsArrayBindingPattern(declaration.Name()) {
		return nil
	}
	if ast.IsCallExpression(loop.Expression) {
		signature := s.checker.GetResolvedSignature(loop.Expression)
		if signature == nil {
			return nil
		}
		symbol := s.checker.GetReturnTypeOfSignature(signature).Alias().Symbol()
		if symbol == nil {
			return nil
		}
		if !genericSymbolsAreEqual(s.jecs.query.symbol, symbol) && !genericSymbolsAreEqual(s.jecs.cachedQuery, symbol) {
			return nil
		}
	}
	// a stored query may come from another scope, so its world is not ours to probe
	stored := ast.IsIdentifier(loop.Expression)
	queryExpression := loop.Expression
	if stored {
		queryExpression = s.storedInitializer(loop.Expression)
	}
	plan := s.classifyQuery(queryExpression)
	if plan == nil || !plan.inline || (stored && plan.dontFragment) {
		return nil
	}
	elements := declaration.Name().AsBindingPattern().Elements.Nodes
	if len(elements) == 0 {
		return nil
	}
	entity, componentIdentifiers := elements[0], elements[1:]
	if plan.dontFragment && s.shadowsBuffer(node, entity) {
		return nil
	}
	expression := s.transformExpression(loop.Expression)
	cached, isCached := cachedQuery{}, false
	if ast.IsIdentifier(expression) {
		cached, isCached = s.cachedQueries[expression]
	}
	if !isCached && !ast.IsCallExpression(expression) && !ast.IsIdentifier(expression) {
		return nil
	}
	var components []*ast.Node
	if isCached {
		components = cached.components
	} else {
		found, ok := s.findQueryComponents(expression, map[*ast.Node]bool{})
		if !ok {
			return nil
		}
		components = found
	}

	var prelude []*ast.Node
	hoist := func(name string, initializer *ast.Node) *ast.Node {
		identifier := f.NewUniqueName(name)
		prelude = append(prelude, s.variableStatement(identifier, initializer, ast.NodeFlagsConst))
		return identifier
	}
	var archetypes *ast.Node
	if isCached {
		archetypes = cached.archetypes
	}
	if archetypes == nil {
		query := expression
		if !ast.IsIdentifier(expression) {
			query = hoist("query", expression)
		}
		archetypes = s.call(s.access(query, "archetypes"))
		if !plan.dontFragment {
			archetypes = hoist("archetypes", archetypes)
			prelude = append(prelude, s.assertNoDontFragment(query, queryExpression))
		}
	}
	archetype := f.NewUniqueName("archetype")
	entities := f.NewUniqueName("entities")
	field := f.NewUniqueName("field")
	row := f.NewUniqueName("row")
	declarationFlags := ast.NodeFlagsConst
	if loop.Initializer.Flags&ast.NodeFlagsLet != 0 {
		declarationFlags = ast.NodeFlagsLet
	}
	entityName := s.entityBinding(entity, plan.dontFragment)
	var entityDeclaration *ast.Node
	if entityName != nil {
		entityDeclaration = s.variableStatement(entityName, f.NewNonNullExpression(f.NewElementAccessExpression(entities, nil, s.rowIndex(row), ast.NodeFlagsNone), ast.NodeFlagsNone), declarationFlags)
	}
	var probes *sparseProbes
	if plan.dontFragment {
		probes = s.newSparseProbes(plan.world, hoist, entityName)
	}
	isSparse := func(component *ast.Node) bool {
		return probes != nil && s.isDontFragment(component)
	}
	boundCount := min(len(componentIdentifiers), len(components))
	bindings := make([]componentBinding, boundCount)
	for index := range boundCount {
		bindings[index] = componentBinding{identifier: componentIdentifiers[index], component: components[index]}
	}
	var guards, sparseValues []*ast.Node
	if probes != nil {
		var unbound []*ast.Node
		if len(componentIdentifiers) < len(components) {
			unbound = components[len(componentIdentifiers):]
		}
		guards, sparseValues = s.sparseRowChecks(plan, probes, bindings, unbound, declarationFlags)
	}
	var rows []componentRow
	for _, binding := range bindings {
		if isOmitted(binding.identifier) || isSparse(binding.component) {
			continue
		}
		list := f.NewGeneratedNameForNode(binding.component)
		value := f.NewNonNullExpression(f.NewElementAccessExpression(list, nil, s.rowIndex(row), ast.NodeFlagsNone), ast.NodeFlagsNone)
		rows = append(rows, componentRow{component: binding.component, list: list, statement: s.variableStatement(binding.identifier.Name(), value, declarationFlags)})
	}
	if len(componentIdentifiers) > len(components) {
		for _, identifier := range componentIdentifiers[len(components):] {
			if isOmitted(identifier) {
				continue
			}
			rows = append(rows, componentRow{statement: s.variableStatement(identifier.Name(), f.NewIdentifier("undefined"), declarationFlags)})
		}
	}

	var brokenVariable *ast.Node
	var fixBreak func(node *ast.Node) *ast.Node
	breakVisitor := s.emitContext.NewNodeVisitor(func(node *ast.Node) *ast.Node { return fixBreak(node) })
	fixBreak = func(node *ast.Node) *ast.Node {
		// breaks will target them
		if ast.IsIterationStatement(node, true) || ast.IsSwitchStatement(node) {
			return node
		}
		if ast.IsBreakStatement(node) {
			if brokenVariable == nil {
				brokenVariable = f.NewUniqueName("broken")
			}
			return f.NewBlock(f.NewNodeList([]*ast.Node{
				f.NewExpressionStatement(f.NewAssignmentExpression(brokenVariable, f.NewTrueExpression())),
				f.NewBreakStatement(nil),
			}), false)
		}
		return breakVisitor.VisitEachChild(node)
	}
	statement := breakVisitor.VisitEachChild(loop.Statement)
	rowBindings := make([]*ast.Node, 0, len(rows)+len(sparseValues))
	for _, componentRow := range rows {
		rowBindings = append(rowBindings, componentRow.statement)
	}
	rowBindings = append(rowBindings, sparseValues...)
	inner := s.innerLoop(row, entities, entityDeclaration, guards, rowBindings, s.transform(statement))
	result := s.outerLoop(entities, archetype, archetypes, field, rows, inner, brokenVariable)
	if len(prelude) > 0 {
		return f.NewBlock(f.NewNodeList(append(prelude, result)), true)
	}
	return result
}

// entityBinding: the probes key on the entity, so a DontFragment loop binds
// one even when the pattern omits it.
func (s *state) entityBinding(entity *ast.Node, dontFragment bool) *ast.Node {
	if !isOmitted(entity) {
		return entity.Name()
	}
	if dontFragment {
		return s.factory.NewUniqueName("entity")
	}
	return nil
}

// sparseRowChecks: per-row skips for every DontFragment term, plus value
// bindings for the bound ones.
func (s *state) sparseRowChecks(plan *queryPlan, probes *sparseProbes, bindings []componentBinding, unboundComponents []*ast.Node, declarationFlags ast.NodeFlags) ([]*ast.Node, []*ast.Node) {
	guards := []*ast.Node{probes.keyDeclaration}
	for _, term := range plan.required {
		guards = append(guards, probes.require(term))
	}
	for _, term := range plan.excluded {
		guards = append(guards, probes.exclude(term))
	}
	var sparseValues []*ast.Node
	for _, binding := range bindings {
		if !s.isDontFragment(binding.component) {
			continue
		}
		if isOmitted(binding.identifier) {
			guards = append(guards, probes.require(binding.component))
			continue
		}
		slotGuards, value := probes.read(binding.component)
		guards = append(guards, slotGuards...)
		sparseValues = append(sparseValues, s.variableStatement(binding.identifier.Name(), value, declarationFlags))
	}
	for _, component := range unboundComponents {
		if s.isDontFragment(component) {
			guards = append(guards, probes.require(component))
		}
	}
	return guards, sparseValues
}

// shadowsBuffer: the probes read the `buffer` global, which a user binding
// of that name would shadow.
func (s *state) shadowsBuffer(node, entity *ast.Node) bool {
	if !isOmitted(entity) && ast.IsIdentifier(entity.Name()) && entity.Name().Text() == "buffer" {
		return true
	}
	for _, symbol := range s.checker.GetSymbolsInScope(node, ast.SymbolFlagsValue) {
		if symbol.Name != "buffer" {
			continue
		}
		for _, declaration := range symbol.Declarations {
			if file := ast.GetSourceFileOfNode(declaration); file != nil && !file.IsDeclarationFile {
				return true
			}
		}
	}
	return false
}

type sparseProbes struct {
	keyDeclaration *ast.Node
	require        func(term *ast.Node) *ast.Node
	exclude        func(term *ast.Node) *ast.Node
	read           func(term *ast.Node) ([]*ast.Node, *ast.Node)
}

// newSparseProbes mirrors jecs `pool_find`; every row re-reads `map` and
// `map_max`, since an insert can reallocate them. A term has no record until
// first use, and a pair record dies with its target.
func (s *state) newSparseProbes(world *ast.Node, hoist func(string, *ast.Node) *ast.Node, entity *ast.Node) *sparseProbes {
	f := s.factory
	records := hoist("records", s.access(f.NewAsExpression(f.NewAsExpression(world, f.NewKeywordTypeNode(ast.KindUnknownKeyword)), s.jecsType("WorldInternals")), "component_index"))
	pool := func(term *ast.Node) *ast.Node {
		record := s.call(s.access(records, "get"), term)
		return hoist("pool", f.NewPropertyAccessExpression(record, f.NewToken(ast.KindQuestionDotToken), f.NewIdentifier("sparse"), ast.NodeFlagsOptionalChain))
	}
	key := f.NewUniqueName("key")
	zero := func() *ast.Node { return s.numeric("0") }
	present := func(target *ast.Node) *ast.Node {
		return s.binary(
			s.binary(target, ast.KindExclamationEqualsEqualsToken, f.NewIdentifier("undefined")),
			ast.KindAmpersandAmpersandToken,
			s.binary(key, ast.KindLessThanEqualsToken, s.access(target, "map_max")),
		)
	}
	slot := func(target *ast.Node) *ast.Node {
		return s.call(s.access(f.NewIdentifier("buffer"), "readu32"), s.access(target, "map"), s.binary(key, ast.KindAsteriskToken, s.numeric(mapSlotBytes)))
	}
	skipRow := func(condition *ast.Node) *ast.Node {
		return f.NewIfStatement(condition, f.NewContinueStatement(nil), nil)
	}
	return &sparseProbes{
		keyDeclaration: s.variableStatement(key, s.binary(entity, ast.KindPercentToken, s.numeric(entityKeyModulus)), ast.NodeFlagsConst),
		require: func(term *ast.Node) *ast.Node {
			target := pool(term)
			return skipRow(s.binary(
				f.NewPrefixUnaryExpression(ast.KindExclamationToken, f.NewParenthesizedExpression(present(target))),
				ast.KindBarBarToken,
				s.binary(slot(target), ast.KindEqualsEqualsEqualsToken, zero()),
			))
		},
		exclude: func(term *ast.Node) *ast.Node {
			target := pool(term)
			return skipRow(s.binary(present(target), ast.KindAmpersandAmpersandToken, s.binary(slot(target), ast.KindExclamationEqualsEqualsToken, zero())))
		},
		read: func(term *ast.Node) ([]*ast.Node, *ast.Node) {
			target := pool(term)
			index := f.NewUniqueName("slot")
			value := f.NewAsExpression(s.call(s.access(s.access(f.NewNonNullExpression(target, ast.NodeFlagsNone), "values"), "get"), index), s.jecsType("InferComponent", s.staticCtToTypeNode(term)))
			return []*ast.Node{
				s.variableStatement(index, f.NewConditionalExpression(present(target), f.NewToken(ast.KindQuestionToken), slot(target), f.NewToken(ast.KindColonToken), zero()), ast.NodeFlagsConst),
				skipRow(s.binary(index, ast.KindEqualsEqualsEqualsToken, zero())),
			}, value
		},
	}
}

func (s *state) outerLoop(entities, archetype, archetypes, field *ast.Node, rows []componentRow, inner, brokenVariable *ast.Node) *ast.Node {
	f := s.factory
	body := []*ast.Node{
		f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{
			f.NewVariableDeclaration(entities, nil, nil, s.access(archetype, "entities")),
		}), ast.NodeFlagsConst)),
	}
	if len(rows) > 0 {
		declarations := []*ast.Node{f.NewVariableDeclaration(field, nil, nil, s.access(archetype, "columns_map"))}
		for _, componentRow := range rows {
			if componentRow.component == nil {
				continue
			}
			declarations = append(declarations, f.NewVariableDeclaration(componentRow.list, nil, nil, f.NewNonNullExpression(f.NewElementAccessExpression(field, nil, componentRow.component, ast.NodeFlagsNone), ast.NodeFlagsNone)))
		}
		body = append(body, f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList(declarations), ast.NodeFlagsConst)))
	}
	if brokenVariable != nil {
		body = append(body, f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{
			f.NewVariableDeclaration(brokenVariable, nil, nil, f.NewFalseExpression()),
		}), ast.NodeFlagsLet)))
	}
	body = append(body, inner)
	if brokenVariable != nil {
		body = append(body, f.NewIfStatement(brokenVariable, f.NewBreakStatement(nil), nil))
	}
	initializer := f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{f.NewVariableDeclaration(archetype, nil, nil, nil)}), ast.NodeFlagsConst)
	return f.NewForInOrOfStatement(ast.KindForOfStatement, nil, initializer, archetypes, f.NewBlock(f.NewNodeList(body), true))
}

func (s *state) innerLoop(row, entities, entityDeclaration *ast.Node, guards, bindings []*ast.Node, statement *ast.Node) *ast.Node {
	f := s.factory
	rowDeclaration := f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{f.NewVariableDeclaration(row, nil, nil, nil)}), ast.NodeFlagsConst)
	// iterate in reverse to prevent iterator invalidation
	rangeExpression := s.call(f.NewIdentifier("$range"),
		s.call(s.access(entities, "size")),
		s.numeric("1"),
		f.NewPrefixUnaryExpression(ast.KindMinusToken, s.numeric("1")),
	)
	var body []*ast.Node
	if entityDeclaration != nil {
		body = append(body, entityDeclaration)
	}
	body = append(body, guards...)
	body = append(body, bindings...)
	body = append(body, statement)
	return f.NewForInOrOfStatement(ast.KindForOfStatement, nil, rowDeclaration, rangeExpression, f.NewBlock(f.NewNodeList(body), true))
}

// findQueryComponents follows identifiers to their declarations, unlike
// parseQuery, since a for-of over a stored query has no components on its own
// expression. ok is false where the JavaScript version returns undefined.
func (s *state) findQueryComponents(expression *ast.Node, visited map[*ast.Node]bool) ([]*ast.Node, bool) {
	if visited[expression] {
		return nil, false
	}
	visited[expression] = true
	switch {
	case ast.IsCallExpression(expression):
		callee := expression.Expression()
		if ast.IsPropertyAccessExpression(callee) {
			symbol := s.checker.GetSymbolAtLocation(callee)
			if symbol != nil && (genericSymbolsAreEqual(s.jecs.query.with, symbol) || genericSymbolsAreEqual(s.jecs.query.without, symbol) || genericSymbolsAreEqual(s.jecs.query.cached, symbol)) {
				return s.findQueryComponents(callee.Expression(), visited)
			}
		}
		symbol := s.checker.GetSymbolAtLocation(callee)
		if symbol != nil && genericSymbolsAreEqual(s.jecs.worldQuery, symbol) {
			components := []*ast.Node{}
			for _, component := range expression.Arguments() {
				if len(s.staticDeclarations(component, true)) == 0 {
					return nil, false
				}
				components = append(components, component)
			}
			return components, true
		}
	case ast.IsIdentifier(expression):
		symbol := s.checker.GetSymbolAtLocation(expression)
		if symbol == nil {
			return nil, false
		}
		declaration := symbol.ValueDeclaration
		if declaration == nil || !ast.IsVariableDeclaration(declaration) || declaration.Initializer() == nil {
			return nil, false
		}
		return s.findQueryComponents(declaration.Initializer(), visited)
	}
	return nil, false
}

// isOmitted is ts.isOmittedExpression for an array binding element: tsgo
// parses a hole in `[, a]` as a BindingElement without a name instead.
func isOmitted(element *ast.Node) bool {
	return ast.IsOmittedExpression(element) || (ast.IsBindingElement(element) && element.Name() == nil)
}
