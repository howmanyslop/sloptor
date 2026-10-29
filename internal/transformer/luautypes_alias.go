package transformer

import (
	"slices"
	"strconv"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

// Alias placement, exports, cross-file mirrors and naming for the luauTypes
// registry (the registry itself is the builder's aliases map, see
// luautypes.go). Three rules decide what a `type` statement looks like and
// where it goes:
//
//   - A top-level interface, type alias or enum of this file is placed AT its
//     declaration (`type` or `export type`, merged declarations once at the
//     first one). The dispatcher asks luauDeclarationStatements for it.
//   - Everything else is HOISTED after the header, in registration order:
//     declarations nested in a namespace, function or block (flattened
//     `Outer_Inner_Type` names), ambient declarations the dispatcher skips,
//     types imported from another file, node_modules or the lib (mirrored
//     under the local import name, because `TS.import` is opaque to Luau's
//     checker), and the Anonymous_N aliases synthesized for recursive
//     anonymous types.
//   - Names are unique within the file: `_2`, `_3`, ... on a collision with
//     another alias, a Luau builtin or type-function name, a Roblox extern
//     type, or any type-parameter name used in the file (a bare `T` inside an
//     alias body must always be the generic). Value names are a separate
//     namespace.
//
// Enum and class declarations register through the same registry; placing
// them at their declaration is WP7's job (luauDeclarationStatements handles
// an enum node once transformEnumDeclaration routes it here), and WP8 fills
// typeAliasEntry.TypeFunction for generic aliases whose body is a type-level
// computation.

// aliasPlacement is where an alias statement is emitted.
type aliasPlacement uint8

const (
	// atDeclaration emits the statement where the TS declaration stands,
	// after its leading comments.
	atDeclaration aliasPlacement = iota
	// hoisted emits the statement after the file header.
	hoisted
)

// luauTypeFunctionNames are the built-in type functions and the `typeof`
// spelling, which an alias name must not shadow.
var luauTypeFunctionNames = map[string]struct{}{
	"typeof": {}, "keyof": {}, "index": {}, "rawkeyof": {}, "rawget": {},
	"setmetatable": {}, "getmetatable": {}, "len": {}, "unm": {}, "add": {},
	"sub": {}, "mul": {}, "div": {}, "idiv": {}, "pow": {}, "mod": {},
	"concat": {}, "lt": {}, "le": {},
}

// aliasCommentLossKinds are the declaration-level losses that earn a comment
// before the alias statement; per-use losses (tuples, number literals) stay
// silent to keep output readable.
var aliasCommentLossKinds = map[fidelityLossKind]struct{}{
	lossMappedType: {}, lossConditionalType: {}, lossInferType: {}, lossRecursiveAliasArgs: {},
}

// ---------------------------------------------------------------------------
// Emission
// ---------------------------------------------------------------------------

// luauDeclarationStatements is the `type` statement to emit AT a top-level
// interface, type alias or enum declaration of this file in luauTypes mode,
// or an empty list: for a nested or ambient declaration (hoisted instead), a
// later declaration of a merged symbol, a declaration the spelling declined
// (a LuaTuple alias, a type-level computation without a type function), or
// when luauTypes is off. The alias is registered here when nothing referenced
// it earlier, so an unused exported type still gets its statement.
func (state *State) luauDeclarationStatements(node *ast.Node) *luau.List[luau.Statement] {
	statements := luau.NewList[luau.Statement]()
	if !state.IsLuauTyped || state.SourceFile == nil {
		return statements
	}
	name := node.Name()
	if name == nil {
		return statements
	}
	symbol := state.Checker.GetSymbolAtLocation(name)
	if symbol == nil || state.luauFirstDeclaration(symbol) != node {
		return statements
	}
	entry := state.typeAliasFor(symbol)
	if entry == nil || entry.Placement != atDeclaration || entry.placed {
		return statements
	}
	entry.placed = true
	statements.PushList(entry.statements())
	return statements
}

// luauHoistedTypeStatements lists the hoisted alias statements, in
// registration order, for prependHeader to place after the header. An alias
// placed at its declaration that the dispatcher never reached (its statement
// was not transformed) is hoisted too, so no reference dangles.
func (state *State) luauHoistedTypeStatements() *luau.List[luau.Statement] {
	statements := luau.NewList[luau.Statement]()
	if state.luauTypeBuilder == nil {
		return statements
	}
	for _, entry := range state.luauTypeBuilder.Aliases() {
		if entry.placed {
			continue
		}
		entry.placed = true
		statements.PushList(entry.statements())
	}
	return statements
}

// statements is the alias's emitted form: its loss comments, then the type
// function or the alias statement carrying the export marker.
func (entry *typeAliasEntry) statements() *luau.List[luau.Statement] {
	statements := luau.NewList[luau.Statement]()
	for _, text := range entry.lossComments() {
		statements.Push(luau.NewComment(text))
	}
	switch {
	case entry.TypeFunction != nil:
		entry.TypeFunction.Export = entry.Exported
		statements.Push(entry.TypeFunction)
	case entry.Statement != nil:
		entry.Statement.Export = entry.Exported
		statements.Push(entry.Statement)
	}
	return statements
}

// lossComments renders the declaration-level losses of the alias body as
// comment texts, one per (replacement, reason) pair with the lost TS types
// listed once each, in the order they were recorded.
func (entry *typeAliasEntry) lossComments() []string {
	type group struct {
		luau, reason string
		tsTexts      []string
	}
	groups := []*group{}
	for _, loss := range entry.Losses {
		if _, comment := aliasCommentLossKinds[loss.Kind]; !comment {
			continue
		}
		var matching *group
		for _, g := range groups {
			if g.luau == loss.Luau && g.reason == loss.Reason {
				matching = g
				break
			}
		}
		if matching == nil {
			matching = &group{luau: loss.Luau, reason: loss.Reason}
			groups = append(groups, matching)
		}
		if !slices.Contains(matching.tsTexts, loss.TSText) {
			matching.tsTexts = append(matching.tsTexts, loss.TSText)
		}
	}
	texts := make([]string, 0, len(groups))
	for _, g := range groups {
		texts = append(texts, " luauTypes: "+strings.Join(g.tsTexts, ", ")+" spelled as "+g.luau+": "+g.reason)
	}
	return texts
}

// luauFirstDeclaration is the first interface, type alias or enum
// declaration of symbol IN THIS FILE, or nil. A declaration emits its alias
// only when it is that node, so a merged symbol emits one alias.
func (state *State) luauFirstDeclaration(symbol *ast.Symbol) *ast.Node {
	for _, declaration := range symbol.Declarations {
		if isAliasDeclaration(declaration) && ast.GetSourceFileOfNode(declaration) == state.SourceFile {
			return declaration
		}
	}
	return nil
}

// isAliasDeclaration reports a declaration that can own a `type` statement.
func isAliasDeclaration(node *ast.Node) bool {
	return ast.IsInterfaceDeclaration(node) || ast.IsTypeAliasDeclaration(node) || ast.IsEnumDeclaration(node)
}

// ---------------------------------------------------------------------------
// Placement and exports
// ---------------------------------------------------------------------------

// aliasPlacementFor decides where symbol's alias goes: at its declaration
// when that declaration is a top-level, non-ambient interface or type alias
// of this file; hoisted otherwise. Enum and class declarations stay hoisted
// until WP7 routes their statements through luauDeclarationStatements.
func (state *State) aliasPlacementFor(declaration *ast.Node) aliasPlacement {
	switch {
	case ast.GetSourceFileOfNode(declaration) != state.SourceFile,
		!isTopLevelDeclaration(declaration),
		ast.HasSyntacticModifier(declaration, ast.ModifierFlagsAmbient),
		!ast.IsInterfaceDeclaration(declaration) && !ast.IsTypeAliasDeclaration(declaration):
		return hoisted
	}
	return atDeclaration
}

// isLuauExportedType reports whether a type declared in this file is part
// of the module's type surface: an `export` modifier (`export default`
// included), a member of an exported namespace chain, an ambient export
// context, or an `export { X }` / `export type { X }` specifier. A mirrored
// type from another file is never exported again. `export =` modules keep
// `export type`: Luau's `export type` is independent of the value returned.
func (state *State) isLuauExportedType(symbol *ast.Symbol) bool {
	if state.SourceFile == nil {
		return false
	}
	for _, declaration := range symbol.Declarations {
		if !isAliasDeclaration(declaration) || ast.GetSourceFileOfNode(declaration) != state.SourceFile {
			continue
		}
		if exportedThroughContainers(declaration) {
			return true
		}
	}
	merged := state.Checker.GetMergedSymbol(symbol)
	for _, statement := range state.SourceFile.Statements.Nodes {
		if !ast.IsExportDeclaration(statement) || statement.ModuleSpecifier() != nil {
			continue
		}
		clause := statement.AsExportDeclaration().ExportClause
		if clause == nil || !ast.IsNamedExports(clause) {
			continue
		}
		for _, specifier := range clause.AsNamedExports().Elements.Nodes {
			target := state.Checker.GetExportSpecifierLocalTargetSymbol(specifier)
			if target != nil && (target == symbol || state.Checker.GetMergedSymbol(target) == merged) {
				return true
			}
		}
	}
	return false
}

// exportedThroughContainers reports whether declaration is exported from
// every container up to the source file: it carries `export` (or sits in
// an ambient export context), and so does each enclosing namespace. A
// declaration inside a function or block is never exported.
func exportedThroughContainers(declaration *ast.Node) bool {
	node := declaration
	for {
		parent := node.Parent
		if parent == nil {
			return false
		}
		container := parent
		if ast.IsModuleBlock(parent) {
			container = parent.Parent
			if container == nil {
				return false
			}
		}
		exported := ast.HasSyntacticModifier(node, ast.ModifierFlagsExport) || container.Flags&ast.NodeFlagsExportContext != 0
		if !exported {
			return false
		}
		if ast.IsSourceFile(parent) {
			return true
		}
		if !ast.IsModuleBlock(parent) || !ast.IsModuleDeclaration(container) {
			return false
		}
		node = container
	}
}

// ---------------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------------

// luauAliasBaseName is the name an alias wants before uniqueness: the TS
// name, prefixed by the enclosing namespaces (and named functions or
// classes) for a nested declaration of this file; for a type from another
// file, the local import name (`import { Foo as Bar }` -> `Bar`, `import *
// as NS` -> `NS_Foo`), else the TS name under its namespace chain. A name
// Luau cannot spell becomes `Alias`; `export default interface Foo` keeps
// `Foo`.
func luauAliasBaseName(state *State, symbol *ast.Symbol) string {
	name := symbol.Name
	if name == ast.InternalSymbolNameDefault {
		name = "Default"
		for _, declaration := range symbol.Declarations {
			if declarationName := declaration.Name(); declarationName != nil && ast.IsIdentifier(declarationName) {
				name = declarationName.Text()
				break
			}
		}
	}
	if declaration := state.luauFirstDeclaration(symbol); declaration != nil {
		name = strings.Join(append(enclosingDeclarationNames(declaration), name), "_")
	} else if local, ok := state.types().importedTypeName(symbol); ok {
		name = local
	} else {
		name = strings.Join(append(namespaceChainNames(symbol), name), "_")
	}
	if !luau.IsValidIdentifier(name) {
		return "Alias"
	}
	return name
}

// enclosingDeclarationNames lists the names of the namespaces, functions and
// classes that enclose declaration, outermost first. Blocks contribute
// nothing.
func enclosingDeclarationNames(declaration *ast.Node) []string {
	var names []string
	for parent := declaration.Parent; parent != nil; parent = parent.Parent {
		if !ast.IsModuleDeclaration(parent) && !ast.IsFunctionLike(parent) && !ast.IsClassLike(parent) {
			continue
		}
		name := parent.Name()
		if name == nil || !ast.IsIdentifier(name) {
			continue
		}
		names = append([]string{name.Text()}, names...)
	}
	return names
}

// namespaceChainNames lists the non-module namespaces a foreign symbol is
// declared under (`declare namespace Foo { interface Bar }` -> ["Foo"]),
// outermost first, stopping at an external module or source file symbol.
func namespaceChainNames(symbol *ast.Symbol) []string {
	var names []string
	for parent := symbol.Parent; parent != nil; parent = parent.Parent {
		if parent.Flags&ast.SymbolFlagsModule == 0 || parent.IsExternalModule() || parent.Flags&ast.SymbolFlagsValueModule != 0 && isSourceFileSymbol(parent) {
			break
		}
		names = append([]string{parent.Name}, names...)
	}
	return names
}

// isSourceFileSymbol reports the module symbol of a source file.
func isSourceFileSymbol(symbol *ast.Symbol) bool {
	return slices.ContainsFunc(symbol.Declarations, ast.IsSourceFile)
}

// uniqueLuauTypeName picks a free alias name from base: base itself, else
// `base_2`, `base_3`, ... skipping names another alias holds, Luau builtins
// and type-function names, Roblox extern types, and every type-parameter
// name used in this file.
func (state *State) uniqueLuauTypeName(base string) string {
	return state.types().uniqueName(base)
}

func (b *typeBuilder) uniqueName(base string) string {
	name := base
	if !luau.IsValidIdentifier(name) {
		name = "Alias"
	}
	for suffix := 1; ; suffix++ {
		candidate := name
		if suffix > 1 {
			candidate = name + "_" + strconv.Itoa(suffix)
		}
		if luau.IsValidIdentifier(candidate) && !b.typeNameTaken(candidate) {
			b.aliasNames[candidate] = struct{}{}
			return candidate
		}
	}
}

// typeNameTaken reports whether candidate cannot name a new alias.
func (b *typeBuilder) typeNameTaken(candidate string) bool {
	if _, taken := b.aliasNames[candidate]; taken {
		return true
	}
	if _, reserved := nativeReservedTypeAliasNames[candidate]; reserved {
		return true
	}
	if _, reserved := luauTypeFunctionNames[candidate]; reserved {
		return true
	}
	if _, used := b.typeParameterNames()[candidate]; used {
		return true
	}
	if global := b.state.Checker.GetGlobalSymbol(candidate, ast.SymbolFlagsType, nil); global != nil && isRobloxSymbol(global) {
		return true
	}
	return false
}

// typeParameterNames is the set of type-parameter names declared anywhere in
// this file, collected once.
func (b *typeBuilder) typeParameterNames() map[string]struct{} {
	if b.reservedTypeParameters != nil {
		return b.reservedTypeParameters
	}
	b.reservedTypeParameters = make(map[string]struct{})
	if b.state.SourceFile == nil {
		return b.reservedTypeParameters
	}
	var visit func(node *ast.Node) bool
	visit = func(node *ast.Node) bool {
		if node.Kind == ast.KindTypeParameter {
			if name := node.Name(); name != nil {
				b.reservedTypeParameters[name.Text()] = struct{}{}
			}
		}
		node.ForEachChild(visit)
		return false
	}
	b.state.SourceFile.AsNode().ForEachChild(visit)
	return b.reservedTypeParameters
}

// importedTypeName is the local name this file imports symbol under, scanned
// once from the import declarations: a default or named import binding
// (renamed or not), or `Namespace_Name` for a member of a namespace import.
func (b *typeBuilder) importedTypeName(symbol *ast.Symbol) (string, bool) {
	if b.importedNames == nil {
		b.importedNames, b.importedNamespaces = b.scanImports()
	}
	if name, ok := b.importedNames[symbol]; ok {
		return name, true
	}
	merged := b.state.Checker.GetMergedSymbol(symbol)
	if name, ok := b.importedNames[merged]; ok {
		return name, true
	}
	if symbol.Parent != nil {
		if prefix, ok := b.importedNamespaces[b.state.Checker.GetMergedSymbol(symbol.Parent)]; ok {
			return prefix + "_" + symbol.Name, true
		}
	}
	return "", false
}

// scanImports maps every symbol this file imports to its local name, and
// every namespace import's module to its local name.
func (b *typeBuilder) scanImports() (names map[*ast.Symbol]string, namespaces map[*ast.Symbol]string) {
	names = make(map[*ast.Symbol]string)
	namespaces = make(map[*ast.Symbol]string)
	if b.state.SourceFile == nil {
		return names, namespaces
	}
	bind := func(local *ast.Node, into map[*ast.Symbol]string) {
		if local == nil || !ast.IsIdentifier(local) {
			return
		}
		alias := b.state.Checker.GetSymbolAtLocation(local)
		if alias == nil || alias.Flags&ast.SymbolFlagsAlias == 0 {
			return
		}
		target := b.state.Checker.GetAliasedSymbol(alias)
		if target == nil || target == alias {
			return
		}
		if _, seen := into[target]; !seen {
			into[target] = local.Text()
		}
	}
	for _, statement := range b.state.SourceFile.Statements.Nodes {
		switch statement.Kind {
		case ast.KindImportEqualsDeclaration:
			bind(statement.Name(), names)
		case ast.KindImportDeclaration:
			clause := statement.AsImportDeclaration().ImportClause
			if clause == nil {
				continue
			}
			bind(clause.Name(), names)
			bindings := clause.AsImportClause().NamedBindings
			switch {
			case bindings == nil:
			case ast.IsNamespaceImport(bindings):
				bind(bindings.Name(), namespaces)
			case ast.IsNamedImports(bindings):
				for _, specifier := range bindings.AsNamedImports().Elements.Nodes {
					bind(specifier.Name(), names)
				}
			}
		}
	}
	return names, namespaces
}

// ---------------------------------------------------------------------------
// Generics with defaults
// ---------------------------------------------------------------------------

// aliasGenerics completes an alias's declared generics with their TS
// defaults, built in the alias's own scope (the entry is already registered,
// so a self reference with other arguments is `any` plus a loss, as Luau
// requires). A default is dropped when it names a parameter declared at or
// after its own (Luau defaults may only reference earlier parameters), and
// then every default before the last parameter without one is dropped too
// (every parameter after a defaulted one must have a default).
func (b *typeBuilder) aliasGenerics(entry *typeAliasEntry, declaration *ast.Node) []luau.TypeGeneric {
	generics := entry.Generics
	for index, parameterIndex := range entry.GenericIndexes {
		parameter := entry.Params[parameterIndex]
		if parameter == nil {
			continue
		}
		defaultType := b.state.Checker.GetDefaultFromTypeParameter(parameter)
		if defaultType == nil {
			continue
		}
		node := b.Build(defaultType, typeContext{Position: positionNested, Owner: declaration}).Node
		later := make(map[string]struct{}, len(generics)-index)
		for _, generic := range generics[index:] {
			later[generic.Name] = struct{}{}
		}
		if typeNodeReferencesName(node, later) {
			continue
		}
		generics[index].Default = node
	}
	return dropLeadingDefaults(generics)
}

// dropLeadingDefaults clears the default of every generic before the last
// generic without one, so defaults are trailing as Luau requires.
func dropLeadingDefaults(generics []luau.TypeGeneric) []luau.TypeGeneric {
	last := -1
	for index, generic := range generics {
		if generic.Default == nil {
			last = index
		}
	}
	for index := range generics[:max(last, 0)] {
		generics[index].Default = nil
	}
	return generics
}

// typeNodeReferencesName reports whether node contains a bare reference to
// one of names. Alias names never collide with type-parameter names (see
// typeNameTaken), so a bare name among a declaration's parameters is that
// parameter.
func typeNodeReferencesName(node luau.TypeNode, names map[string]struct{}) bool {
	found := false
	walkTypeNode(node, func(child luau.TypeNode) {
		if reference, ok := child.(*luau.TypeReference); ok && reference.Module == "" {
			if _, named := names[reference.Name]; named {
				found = true
			}
		}
	})
	return found
}

// walkTypeNode calls visit on node and every type node under it.
func walkTypeNode(node luau.TypeNode, visit func(luau.TypeNode)) {
	if node == nil {
		return
	}
	visit(node)
	walkList := func(list *luau.List[luau.TypeNode]) {
		if list != nil {
			list.ForEach(func(child luau.TypeNode) { walkTypeNode(child, visit) })
		}
	}
	switch n := node.(type) {
	case *luau.TypeReference:
		walkList(n.Arguments)
	case *luau.TypeOptional:
		walkTypeNode(n.Inner, visit)
	case *luau.TypeUnion:
		walkList(n.Members)
	case *luau.TypeIntersection:
		walkList(n.Members)
	case *luau.TypeTable:
		if n.Fields != nil {
			n.Fields.ForEach(func(field luau.TypeTableField) {
				switch f := field.(type) {
				case *luau.TypeTableProp:
					walkTypeNode(f.Value, visit)
				case *luau.TypeTableIndexer:
					walkTypeNode(f.Key, visit)
					walkTypeNode(f.Value, visit)
				}
			})
		}
	case *luau.TypeArray:
		walkTypeNode(n.Element, visit)
	case *luau.TypeFunction:
		for _, generic := range n.Generics {
			walkTypeNode(generic.Default, visit)
		}
		for _, parameter := range n.Parameters {
			walkTypeNode(parameter.Type, visit)
		}
		walkTypeNode(n.VarArgs, visit)
		walkTypeNode(n.Return, visit)
	case *luau.TypePack:
		walkList(n.Elements)
	case *luau.TypeVariadic:
		walkTypeNode(n.Inner, visit)
	}
}

// declaredArguments keeps the type arguments that belong to the alias's
// declared generics: a TS parameter Luau cannot name (a keyword) has no
// generic, so its argument is dropped from the reference.
func declaredArguments(entry *typeAliasEntry, arguments []*checker.Type) []*checker.Type {
	if len(entry.GenericIndexes) == len(entry.Params) {
		return arguments
	}
	kept := make([]*checker.Type, 0, len(entry.GenericIndexes))
	for _, index := range entry.GenericIndexes {
		if index < len(arguments) {
			kept = append(kept, arguments[index])
		}
	}
	return kept
}
