// Package jecs is a native port of rbxts-transformer-jecs: it caches
// `world.query(...)` calls and inlines query iteration into archetype loops.
//
// It mirrors the JavaScript transformer function for function (see
// node_modules/rbxts-transformer-jecs/out/src), so its output stays identical
// to the sidecar's. Callers only route a project here when the installed
// transformer's sources hash to the version this port was written against.
package jecs

import (
	"errors"
	"fmt"
	"regexp"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/printer"
	"rotor/tsgo/scanner"
)

// Config is the transformer's tsconfig plugin entry.
type Config struct {
	JecsPackage string
	Silent      bool
}

// Input is one transform over a program's source files.
type Input struct {
	Program *compiler.Program
	Checker *checker.Checker
	Files   []*ast.SourceFile
	Config  Config
	// Warn receives the warnings the JavaScript transformer prints.
	Warn func(string)
}

// Result holds every input file, transformed or not, in input order.
type Result struct {
	Files       []*ast.SourceFile
	Changed     []bool
	EmitContext *printer.EmitContext
}

var queryPattern = regexp.MustCompile(`(?:\.\s*query\b|\[\s*["']query["']\s*\])`)

// NeedsTransform is the patched transformer's cheap text test: a file that
// fails it cannot hold a query or a registration, so it is handed back
// without a type-checker visit.
func NeedsTransform(text string) bool {
	return strings.Contains(text, "registerComponent<") || strings.Contains(text, "createBinarySerializer") || queryPattern.MatchString(text)
}

// transformError carries a JavaScript `throw` or failed `assert` out of the
// visitors, which have no error return.
type transformError struct{ err error }

func fail(format string, args ...any) {
	panic(transformError{fmt.Errorf(format, args...)})
}

// Transform runs the transformer over input.Files.
func Transform(input Input) (result Result, err error) {
	if input.Program == nil || input.Checker == nil {
		return Result{}, errors.New("jecs: program and checker are required")
	}
	if input.Config.JecsPackage == "" {
		input.Config.JecsPackage = "@rbxts/jecs"
	}
	emitContext := printer.NewEmitContext()
	s := &state{
		program:       input.Program,
		checker:       input.Checker,
		emitContext:   emitContext,
		factory:       emitContext.Factory,
		config:        input.Config,
		warn:          input.Warn,
		cachedQueries: map[*ast.Node]cachedQuery{},
	}
	if s.warn == nil {
		s.warn = func(string) {}
	}
	s.visitor = emitContext.NewNodeVisitor(s.transformNode)

	defer func() {
		if recovered := recover(); recovered != nil {
			failure, ok := recovered.(transformError)
			if !ok {
				panic(recovered)
			}
			result, err = Result{}, failure.err
		}
	}()

	result = Result{
		Files:       make([]*ast.SourceFile, len(input.Files)),
		Changed:     make([]bool, len(input.Files)),
		EmitContext: emitContext,
	}
	for index, file := range input.Files {
		result.Files[index] = file
		if file.IsDeclarationFile || !NeedsTransform(file.Text()) {
			continue
		}
		transformed := s.transformFile(file)
		result.Files[index] = transformed
		result.Changed[index] = transformed != file
	}
	return result, nil
}

type querySymbols struct {
	symbol  *ast.Symbol
	with    *ast.Symbol
	without *ast.Symbol
	cached  *ast.Symbol
}

type jecsSymbols struct {
	query       querySymbols
	cachedQuery *ast.Symbol
	wildcard    *ast.Symbol
	worldSymbol *ast.Symbol
	worldQuery  *ast.Symbol
}

type cachedQuery struct {
	archetypes *ast.Node
	components []*ast.Node
}

type cacheFrame struct {
	node    *ast.Node
	order   []*cache
	byWorld map[*ast.Symbol]*cache
}

type state struct {
	program     *compiler.Program
	checker     *checker.Checker
	emitContext *printer.EmitContext
	factory     *printer.NodeFactory
	visitor     *ast.NodeVisitor
	config      Config
	warn        func(string)

	file          *ast.SourceFile
	jecs          *jecsSymbols
	prereqStack   [][]*ast.Node
	cacheStack    []*cacheFrame
	cachedQueries map[*ast.Node]cachedQuery
}

// transform is TransformState.transform: visit every child with transformNode.
func (s *state) transform(node *ast.Node) *ast.Node {
	return s.visitor.VisitEachChild(node)
}

func (s *state) transformNode(node *ast.Node) *ast.Node {
	if ast.IsExpression(node) {
		return s.transformExpression(node)
	}
	if ast.IsStatement(node) {
		return s.statements(s.transformStatement(node))
	}
	return s.transform(node)
}

// statements is how a JavaScript visitor's array result reaches a tsgo
// visitor: one node stands alone, several are spliced as a SyntaxList.
func (s *state) statements(nodes []*ast.Node) *ast.Node {
	if len(nodes) == 1 {
		return nodes[0]
	}
	return s.factory.NewSyntaxList(nodes)
}

func (s *state) transformExpression(expression *ast.Node) *ast.Node {
	if ast.IsCallExpression(expression) {
		if replaced := s.cacheQuery(expression); replaced != nil {
			return replaced
		}
	}
	return s.transform(expression)
}

func (s *state) transformStatement(statement *ast.Node) []*ast.Node {
	s.prereqStack = append(s.prereqStack, []*ast.Node{})
	var node *ast.Node
	if ast.IsForOfStatement(statement) {
		node = s.inlineQueryIterators(statement)
	}
	if node == nil {
		node = s.transform(statement)
	}
	prereqs := s.prereqStack[len(s.prereqStack)-1]
	s.prereqStack = s.prereqStack[:len(s.prereqStack)-1]
	return append(prereqs, node)
}

func (s *state) prereq(statements ...*ast.Node) {
	if len(s.prereqStack) == 0 {
		return
	}
	top := len(s.prereqStack) - 1
	s.prereqStack[top] = append(s.prereqStack[top], statements...)
}

func (s *state) transformFile(file *ast.SourceFile) *ast.SourceFile {
	s.file = file
	s.loadJecsSymbols(file)
	original := file.Statements.Nodes
	statements, changed := s.transformStatementList(file.AsNode(), original)
	if !changed {
		return file
	}
	list := s.factory.NewNodeList(statements)
	list.Loc = file.Statements.Loc
	return s.factory.UpdateSourceFile(file, list, file.EndOfFileToken).AsSourceFile()
}

func (s *state) transformStatementList(node *ast.Node, toTransform []*ast.Node) ([]*ast.Node, bool) {
	endIndices := map[*ast.Node]int{}
	changed := false
	frame := &cacheFrame{node: node, byWorld: map[*ast.Symbol]*cache{}}
	s.cacheStack = append(s.cacheStack, frame)
	statements := []*ast.Node{}
	for _, original := range toTransform {
		transformed := s.transformStatement(original)
		if len(transformed) != 1 || transformed[0] != original {
			changed = true
		}
		statements = append(statements, transformed...)
		endIndices[original] = len(statements)
	}
	s.cacheStack = s.cacheStack[:len(s.cacheStack)-1]
	caches := frame.order
	if !changed && len(caches) == 0 {
		return toTransform, false
	}

	result := statements
	offset := 0
	for _, c := range caches {
		if c.hasMarkers() {
			var visit func(child *ast.Node) *ast.Node
			visitor := s.emitContext.NewNodeVisitor(func(child *ast.Node) *ast.Node { return visit(child) })
			visit = func(child *ast.Node) *ast.Node {
				if inner := c.toInnerResults(child); inner != nil {
					return s.statements(inner)
				}
				return visitor.VisitEachChild(child)
			}
			for index, statement := range result {
				result[index] = visitor.VisitEachChild(statement)
			}
		}
		outerResults := c.outerStatements
		insertIndex := 0
		for _, required := range c.requires {
			endIndex := endIndices[required]
			if endIndex > insertIndex {
				insertIndex = endIndex + offset
				offset += len(outerResults)
			}
		}
		// Array.prototype.slice clamps, and the offset bookkeeping above can
		// run past the end.
		insertIndex = min(insertIndex, len(result))
		next := make([]*ast.Node, 0, len(result)+len(outerResults))
		next = append(next, result[:insertIndex]...)
		next = append(next, outerResults...)
		next = append(next, result[insertIndex:]...)
		result = next
	}
	return result, true
}

func (s *state) getCache(position int, world *ast.Node) *cache {
	frame := s.cacheStack[position]
	worldSymbol := s.checker.GetSymbolAtLocation(world)
	if worldSymbol == nil {
		fail("Unable to find symbol for world")
	}
	c := frame.byWorld[worldSymbol]
	if c == nil {
		c = &cache{state: s}
		frame.byWorld[worldSymbol] = c
		frame.order = append(frame.order, c)
	}
	return c
}

func (s *state) currentCache(world *ast.Node) *cache {
	return s.getCache(len(s.cacheStack)-1, world)
}

func (s *state) currentCacheNode() *ast.Node {
	return s.cacheStack[len(s.cacheStack)-1].node
}

func (s *state) fileCache(world *ast.Node) *cache {
	return s.getCache(0, world)
}

func (s *state) jecsType(typeName string, args ...*ast.Node) *ast.Node {
	f := s.factory
	var typeArguments *ast.NodeList
	if len(args) > 0 {
		typeArguments = f.NewNodeList(args)
	}
	return f.NewImportTypeNode(false, f.NewLiteralTypeNode(f.NewStringLiteral(s.config.JecsPackage, 0)), nil, f.NewIdentifier(typeName), typeArguments)
}

func (s *state) loadJecsSymbols(file *ast.SourceFile) {
	if s.jecs != nil {
		return
	}
	jecsSymbol := s.packageSymbol(file, s.config.JecsPackage)
	if jecsSymbol == nil {
		fail("Unable to extract type information from Jecs")
	}
	exports := s.checker.GetExportsOfModule(jecsSymbol)
	find := func(name, message string) *ast.Symbol {
		for _, symbol := range exports {
			if symbol.Name == name {
				return symbol
			}
		}
		fail("%s", message)
		return nil
	}
	property := func(t *checker.Type, name, message string) *ast.Symbol {
		symbol := s.checker.GetPropertyOfType(t, name)
		if symbol == nil {
			fail("%s", message)
		}
		return symbol
	}
	querySymbol := find("Query", "Unable to find Query type information from Jecs")
	queryType := s.checker.GetDeclaredTypeOfSymbol(querySymbol)
	worldSymbol := find("World", "Unable to find World type information from Jecs")
	worldType := s.checker.GetDeclaredTypeOfSymbol(worldSymbol)
	s.jecs = &jecsSymbols{
		query: querySymbols{
			symbol:  querySymbol,
			with:    property(queryType, "with", "Unable to find Query.with type information from Jecs"),
			without: property(queryType, "without", "Unable to find Query.without type information from Jecs"),
			cached:  property(queryType, "cached", "Unable to find Query.cached type information from Jecs"),
		},
		cachedQuery: find("CachedQuery", "Unable to find CachedQuery type information from Jecs"),
		wildcard:    find("Wildcard", "Unable to find Wildcard type information from Jecs"),
		worldSymbol: worldSymbol,
		worldQuery:  property(worldType, "query", "Unable to find World.query type information from Jecs"),
	}
}

// packageSymbol stands in for ts.resolveModuleName: the program has already
// resolved the package for every file that imports it, so the file's own
// resolution wins and any other file's resolution of the same name is the
// fallback.
func (s *state) packageSymbol(file *ast.SourceFile, packageName string) *ast.Symbol {
	resolvedFileName := ""
	for _, mode := range []core.ResolutionMode{core.ResolutionModeNone, core.ResolutionModeESM, core.ResolutionModeCommonJS} {
		if resolved := s.program.GetResolvedModule(file, packageName, mode); resolved != nil && resolved.IsResolved() {
			resolvedFileName = resolved.ResolvedFileName
			break
		}
	}
	if resolvedFileName == "" {
		for _, resolutions := range s.program.GetResolvedModules() {
			for key, resolved := range resolutions {
				if key.Name == packageName && resolved != nil && resolved.IsResolved() {
					resolvedFileName = resolved.ResolvedFileName
					break
				}
			}
			if resolvedFileName != "" {
				break
			}
		}
	}
	if resolvedFileName == "" {
		return nil
	}
	moduleFile := s.program.GetSourceFile(resolvedFileName)
	if moduleFile == nil {
		return nil
	}
	if symbol := s.checker.GetSymbolAtLocation(moduleFile.AsNode()); symbol != nil {
		return symbol
	}
	return moduleFile.Symbol
}

type cacheGroup struct {
	statements []*ast.Node
	anchor     *core.TextRange
	marker     *ast.Node
	worldKey   *ast.Node
	condition  *ast.Node
}

type cache struct {
	state           *state
	requires        []*ast.Node
	outerStatements []*ast.Node
	groups          []*cacheGroup
}

func (c *cache) require(declarations []*ast.Node) {
	c.requires = append(c.requires, declarations...)
}

func (c *cache) outerResult(statements ...*ast.Node) {
	c.outerStatements = append(c.outerStatements, statements...)
}

func (c *cache) newGroup(anchor *core.TextRange) *cacheGroup {
	group := &cacheGroup{anchor: anchor}
	c.groups = append(c.groups, group)
	return group
}

func (c *cache) currentGroup() *cacheGroup {
	if len(c.groups) == 0 {
		return c.newGroup(nil)
	}
	return c.groups[len(c.groups)-1]
}

func (c *cache) innerResult(statements ...*ast.Node) {
	group := c.currentGroup()
	if group.marker == nil {
		group.marker = c.state.factory.NewEmptyStatement()
		c.state.prereq(group.marker)
	}
	group.statements = append(group.statements, statements...)
}

func (c *cache) conditioned(world *ast.Node) {
	f := c.state.factory
	group := c.currentGroup()
	if group.condition != nil {
		return
	}
	group.worldKey = f.NewUniqueName("worldKey")
	group.condition = f.NewStrictInequalityExpression(group.worldKey, world)
	c.outerResult(anchored(f.NewVariableStatement(nil, f.NewVariableDeclarationList(f.NewNodeList([]*ast.Node{
		f.NewVariableDeclaration(group.worldKey, nil, c.state.jecsType("World"), nil),
	}), ast.NodeFlagsLet)), group.anchor))
	c.innerResult(anchored(f.NewExpressionStatement(f.NewAssignmentExpression(group.worldKey, world)), group.anchor))
}

func (c *cache) hasMarkers() bool {
	for _, group := range c.groups {
		if group.marker != nil {
			return true
		}
	}
	return false
}

func (c *cache) toInnerResults(marker *ast.Node) []*ast.Node {
	for _, group := range c.groups {
		if group.marker != marker {
			continue
		}
		if group.condition != nil {
			f := c.state.factory
			return []*ast.Node{anchored(f.NewIfStatement(group.condition, f.NewBlock(f.NewNodeList(group.statements), true), nil), group.anchor)}
		}
		return group.statements
	}
	return nil
}

// anchored gives a synthesized statement the text range of the query it
// serves, so the emitter does not attribute it to whatever it emitted last.
func anchored(node *ast.Node, anchor *core.TextRange) *ast.Node {
	if anchor != nil {
		node.Loc = *anchor
	}
	return node
}

func (s *state) textRangeOf(node *ast.Node) *core.TextRange {
	loc := core.NewTextRange(scanner.GetTokenPosOfNode(node, ast.GetSourceFileOfNode(node), false), node.End())
	return &loc
}

func (s *state) leadingTrivia(node *ast.Node) string {
	file := ast.GetSourceFileOfNode(node)
	if file == nil {
		return ""
	}
	return file.Text()[node.Pos():scanner.GetTokenPosOfNode(node, file, false)]
}

func nodeText(node *ast.Node) string {
	return scanner.GetSourceTextOfNodeFromSourceFile(ast.GetSourceFileOfNode(node), node, false)
}

// genericSymbolsAreEqual compares symbols by first declaration: Query's
// members are generic, so the checker hands back a different symbol per
// instantiation.
func genericSymbolsAreEqual(a, b *ast.Symbol) bool {
	return a != nil && b != nil && len(a.Declarations) > 0 && len(b.Declarations) > 0 && a.Declarations[0] == b.Declarations[0]
}
