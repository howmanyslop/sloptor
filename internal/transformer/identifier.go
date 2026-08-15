package transformer

import (
	"fmt"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// identifierSymbol is the shorthand-aware symbol lookup shared by
// TransformIdentifier and TransformIdentifierDefined (transformIdentifier.ts
// L15-18/L119-122): an identifier inside a ShorthandPropertyAssignment
// resolves through getShorthandAssignmentValueSymbol, everything else through
// getSymbolAtLocation.
func identifierSymbol(s *State, node *ast.Node) *ast.Symbol {
	var symbol *ast.Symbol
	if node.Parent != nil && ast.IsShorthandPropertyAssignment(node.Parent) {
		symbol = s.Checker.GetShorthandAssignmentValueSymbol(node.Parent)
	} else {
		symbol = s.Checker.GetSymbolAtLocation(node)
	}
	if symbol == nil {
		// tsgo's GetSymbolAtLocation returns nil where TypeScript returns
		// unknownSymbol. Upstream never sees that: getPreEmitDiagnostics
		// skips the file first. [flamework] noSemanticDiagnostics skips
		// that precheck, so emit the identifier text instead of asserting.
		if s.SkipSemanticDiagnostics {
			return nil
		}
		panic(missingIdentifierSymbol(node)) // upstream assert
	}
	return symbol
}

func missingIdentifierSymbol(node *ast.Node) string {
	name := node.Text()
	if file := ast.GetSourceFileOfNode(node); file != nil && file.FileName() != "" {
		return fmt.Sprintf("transformer: identifier %q has no symbol\n    at %s", name, file.FileName())
	}
	return fmt.Sprintf("transformer: identifier %q has no symbol", name)
}

// TransformIdentifier ports transformIdentifier.ts transformIdentifier (L111-
// 176) — an identifier in a *use* position.
func TransformIdentifier(s *State, node *ast.Node) luau.Expression {
	// Synthetic nodes don't have parents or symbols, so skip all the
	// symbol-related logic. JSX EntityName functions like getJsxFactoryEntity()
	// return synthetic nodes and transformEntityName eventually ends up here.
	if node.Parent == nil || ast.PositionIsSynthesized(node.Pos()) {
		return luau.ID(node.Text())
	}

	symbol := identifierSymbol(s, node)
	if symbol == nil {
		return luau.ID(node.Text())
	}

	if s.Checker.IsUndefinedSymbol(symbol) {
		return luau.Nil()
	} else if s.Checker.IsArgumentsSymbol(symbol) {
		s.Diags.Add(DiagNoArguments(node))
	} else if isGlobalThisSymbol(symbol) {
		s.Diags.Add(DiagNoGlobalThis(node))
	}

	// Identifier macros (upstream L132-135): currently just `Promise` ->
	// `TS.Promise`. NOTE constructor-macro NEW expressions never reach here:
	// transformNewExpression dispatches the macro before transforming its
	// callee — but its non-macro fallback (`X.new(args)`) DOES transform the
	// callee identifier, which is how `new Promise(...)` becomes
	// `TS.Promise.new(...)`.
	if macro := s.Macros().GetIdentifierMacro(symbol); macro != nil {
		if macro.Macro != nil {
			return macro.Macro(s, node)
		}
		s.Diags.Add(DiagRotorNotYetSupported(node, "macro `"+macro.Name+"`"))
		return luau.NewNone()
	}

	// rotor extension: a bare `$env` outside a call/property/element access
	// has no runtime value (those positions are consumed earlier, by
	// interceptEnvChain in transformOptionalChain — see envmacro.go); emitting
	// the identifier itself would produce invalid Luau.
	if node.Text() == "$env" {
		if envSymbol := envMacroSymbol(s); envSymbol != nil && symbol == envSymbol {
			s.Diags.Add(DiagRotorEnvBadUsage(node))
			return luau.NewNone()
		}
	}

	// rotor extension: a bare `$asset` outside a call has no runtime value
	// (the call position is consumed earlier, by interceptAssetChain — see
	// assetmacro.go); emitting the identifier itself would produce invalid
	// Luau.
	if node.Text() == "$asset" {
		if assetSymbol := assetMacroSymbol(s); assetSymbol != nil && symbol == assetSymbol {
			s.Diags.Add(DiagRotorAssetBadUsage(node))
			return luau.NewNone()
		}
	}

	// rotor extension: bare $nameof / $keys / $file / $git / $buildTime outside
	// a call have no runtime value (the call position is consumed earlier by the
	// respective interceptors in transformOptionalChain). Emitting the macro
	// identifier itself would produce invalid Luau.
	switch node.Text() {
	case "$nameof":
		if sym := nameofMacroSymbol(s); sym != nil && symbol == sym {
			s.Diags.Add(DiagRotorNameofBadUsage(node))
			return luau.NewNone()
		}
	case "$keys":
		if sym := keysMacroSymbol(s); sym != nil && symbol == sym {
			s.Diags.Add(DiagRotorKeysBadUsage(node))
			return luau.NewNone()
		}
	case "$file":
		if sym := fileMacroSymbol(s); sym != nil && symbol == sym {
			s.Diags.Add(DiagRotorFileBadUsage(node))
			return luau.NewNone()
		}
	case "$git":
		if sym := gitMacroSymbol(s); sym != nil && symbol == sym {
			s.Diags.Add(DiagRotorGitBadUsage(node))
			return luau.NewNone()
		}
	case "$buildTime":
		if sym := buildTimeMacroSymbol(s); sym != nil && symbol == sym {
			s.Diags.Add(DiagRotorBuildTimeBadUsage(node))
			return luau.NewNone()
		}
	}

	// Constructor-macro misuse (upstream L137-150): a constructor-macro
	// identifier used WITHOUT `new` has no value to emit. As a class
	// `extends` expression that is noMacroExtends; anywhere else
	// noConstructorMacroWithoutNew. (Macro `new` expressions were already
	// dispatched by transformNewExpression and never reach here.)
	if constructSymbol := getFirstConstructSymbol(s, node); constructSymbol != nil {
		if s.Macros().GetConstructorMacro(constructSymbol) != nil {
			isClassExtendsNode := false
			if node.Parent.Parent != nil && node.Parent.Parent.Parent != nil &&
				ast.IsClassLike(node.Parent.Parent.Parent) {
				if extendsNode := ast.GetExtendsHeritageClauseElement(node.Parent.Parent.Parent); extendsNode != nil &&
					extendsNode.Expression() == node {
					isClassExtendsNode = true
				}
			}
			if isClassExtendsNode {
				s.Diags.Add(DiagNoMacroExtends(node))
			} else {
				s.Diags.Add(DiagNoConstructorMacroWithoutNew(node))
			}
		}
	}

	// Call-macro misuse outside call position (upstream L152-159): a
	// call-macro identifier (`typeOf`, `identity`, ...) has no value — it
	// must be invoked directly. (isValidMethodIndexWithoutCall carves out
	// `typeof import("x").y` positions before the access transforms get here.)
	if parent := SkipUpwards(node).Parent; parent != nil {
		if (!ast.IsCallExpression(parent) || SkipDownwards(parent.AsCallExpression().Expression) != node) &&
			s.Macros().GetCallMacro(symbol) != nil {
			s.Diags.Add(DiagNoIndexWithoutCall(node))
			return luau.NewNone()
		}
	}

	// `export let` indirection (upstream L161-171): reads of mutable exported
	// symbols compile to `exports.<name>` (exit here so hoisting is never
	// checked for them; consts stay plain locals).
	if vd := symbol.ValueDeclaration; vd != nil &&
		ast.GetSourceFileOfNode(vd) == ast.GetSourceFileOfNode(node) &&
		ast.FindAncestor(vd, func(n *ast.Node) bool {
			return ast.IsModuleDeclaration(n) && !isNamespace(n)
		}) == nil {
		exportAccess := s.GetModuleIDPropertyAccess(symbol)
		if exportAccess != nil && IsSymbolMutable(s, symbol) {
			return exportAccess
		}
	}

	checkIdentifierHoist(s, node, symbol)

	return TransformIdentifierDefined(s, node)
}

// TransformIdentifierDefined ports transformIdentifier.ts
// transformIdentifierDefined (L14-28): an identifier in a *defining* position.
// The symbol is looked up (shorthand-property-assignment aware) so renamed/
// captured variables resolve through SymbolToID; otherwise the identifier's
// own text is used.
func TransformIdentifierDefined(s *State, node *ast.Node) luau.AnyIdentifier {
	symbol := identifierSymbol(s, node)
	if symbol == nil {
		return luau.ID(node.Text())
	}
	if replacement := s.SymbolToID[symbol]; replacement != nil {
		return replacement
	}
	return luau.ID(node.Text())
}

// ---------------------------------------------------------------------------
// checkIdentifierHoist — use-before-declare hoisting (upstream L30-109)
// ---------------------------------------------------------------------------

// getAncestorWhichIsChildOf ports the same-named upstream helper (L30-35):
// climbs from node to the ancestor that is a direct child of parent, or nil
// when parent is not an ancestor of node.
func getAncestorWhichIsChildOf(parent, node *ast.Node) *ast.Node {
	for node.Parent != nil && node.Parent != parent {
		node = node.Parent
	}
	if node.Parent == nil {
		return nil
	}
	return node
}

// getDeclarationFromImport ports the same-named upstream helper (L38-45):
// scans symbol.declarations for one under any import syntax ("for some
// reason, symbol.valueDeclaration doesn't point to imports").
func getDeclarationFromImport(symbol *ast.Symbol) *ast.Node {
	for _, declaration := range symbol.Declarations {
		if ast.FindAncestor(declaration, ast.IsAnyImportSyntax) != nil {
			return declaration
		}
	}
	return nil
}

// checkIdentifierHoist ports transformIdentifier.ts checkIdentifierHoist
// (L47-109): records that a symbol must be pre-declared (`local x` merged at
// the premature reference's statement by createHoistDeclaration) when it is
// referenced lexically before/at its declaring statement within the same
// BlockLike.
func checkIdentifierHoist(s *State, node *ast.Node, symbol *ast.Symbol) {
	if _, decided := s.IsHoisted[symbol]; decided {
		return
	}

	declaration := symbol.ValueDeclaration
	if declaration == nil {
		declaration = getDeclarationFromImport(symbol)
	}

	// parameters cannot be hoisted
	if declaration == nil ||
		ast.FindAncestorKind(declaration, ast.KindParameter) != nil ||
		ast.IsShorthandPropertyAssignment(declaration) {
		return
	}

	// class expressions can self refer
	if ast.IsClassLike(declaration) && isAncestorOf(declaration, node) {
		return
	}

	declarationStatement := ast.FindAncestor(declaration, ast.IsStatement)
	if declarationStatement == nil ||
		ast.IsForStatement(declarationStatement) ||
		ast.IsForOfStatement(declarationStatement) ||
		ast.IsTryStatement(declarationStatement) {
		return
	}

	parent := declarationStatement.Parent
	if parent == nil || !isBlockLike(parent) {
		return
	}

	sibling := getAncestorWhichIsChildOf(parent, node)
	if sibling == nil || !ast.IsStatement(sibling) {
		return
	}

	statements := parent.Statements()
	declarationIdx := indexOfNode(statements, declarationStatement)
	siblingIdx := indexOfNode(statements, sibling)

	if siblingIdx > declarationIdx {
		return
	}

	if siblingIdx == declarationIdx {
		// non-async function declarations, class declarations, and variable
		// statements can self refer
		if (ast.IsFunctionDeclaration(declarationStatement) &&
			!ast.HasSyntacticModifier(declarationStatement, ast.ModifierFlagsAsync)) ||
			ast.IsClassDeclaration(declarationStatement) ||
			(ast.IsVariableStatement(declarationStatement) &&
				ast.FindAncestor(node, func(n *ast.Node) bool {
					return ast.IsStatement(n) || ast.IsFunctionLikeDeclaration(n)
				}) == declarationStatement) {
			return
		}
	}

	s.HoistsByStatement[sibling] = append(s.HoistsByStatement[sibling], node)
	s.IsHoisted[symbol] = true
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

// isAncestorOf ports util/traversal.ts isAncestorOf: true when ancestor is
// node or any of node's parents.
func isAncestorOf(ancestor, node *ast.Node) bool {
	for node != nil {
		if ancestor == node {
			return true
		}
		node = node.Parent
	}
	return false
}

// isBlockLike ports typeGuards.ts isBlockLike: SourceFile | Block |
// ModuleBlock | CaseClause | DefaultClause (exactly the kinds tsgo's
// Node.Statements() accepts).
func isBlockLike(node *ast.Node) bool {
	switch node.Kind {
	case ast.KindSourceFile, ast.KindBlock, ast.KindModuleBlock,
		ast.KindCaseClause, ast.KindDefaultClause:
		return true
	}
	return false
}

// isNamespace ports typeGuards.ts isNamespace: a ModuleDeclaration written
// with the `namespace` keyword. Upstream tests NodeFlags.Namespace; tsgo keeps
// the declaring keyword kind on the node instead.
func isNamespace(node *ast.Node) bool {
	return ast.IsModuleDeclaration(node) && node.AsModuleDeclaration().Keyword == ast.KindNamespaceKeyword
}

func indexOfNode(nodes []*ast.Node, node *ast.Node) int {
	for i, n := range nodes {
		if n == node {
			return i
		}
	}
	return -1
}

// isGlobalThisSymbol detects the checker-synthesized `globalThis` symbol.
// Upstream compares against macroManager.getSymbolOrThrow(SYMBOL_NAMES.
// globalThis); tsgo's checker builds that symbol internally (Module-flagged,
// named "globalThis", no declarations) and does not expose it, so rotor
// matches it structurally.
func isGlobalThisSymbol(symbol *ast.Symbol) bool {
	return symbol.Name == "globalThis" &&
		symbol.Flags&ast.SymbolFlagsModule != 0 &&
		len(symbol.Declarations) == 0
}
