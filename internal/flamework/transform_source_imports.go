package flamework

import (
	"fmt"

	"rotor/tsgo/ast"
)

func deduplicateSourceImports(factory *ast.NodeFactory, original, sourceFile *ast.SourceFile) *ast.SourceFile {
	authoredCounts := make(map[string]int)
	for _, statement := range original.Statements.Nodes {
		if key := sourceNamedImportKey(statement); key != "" {
			authoredCounts[key]++
		}
	}
	authored := make(map[*ast.Node]struct{}, len(authoredCounts))
	for index := len(sourceFile.Statements.Nodes) - 1; index >= 0; index-- {
		statement := sourceFile.Statements.Nodes[index]
		key := sourceNamedImportKey(statement)
		if key == "" || authoredCounts[key] == 0 {
			continue
		}
		authoredCounts[key]--
		authored[statement] = struct{}{}
	}
	authoredBindings := make(map[string]map[string]struct{})
	for statement := range authored {
		groupKey := sourceNamedImportGroupKey(statement)
		if groupKey == "" {
			continue
		}
		bindings := authoredBindings[groupKey]
		if bindings == nil {
			bindings = make(map[string]struct{})
			authoredBindings[groupKey] = bindings
		}
		clause := statement.AsImportDeclaration().ImportClause.AsImportClause()
		for _, element := range clause.NamedBindings.AsNamedImports().Elements.Nodes {
			bindings[sourceImportSpecifierKey(element)] = struct{}{}
		}
	}

	positions := make(map[string]int)
	bases := make(map[string]*ast.Node)
	elements := make(map[string][]*ast.Node)
	seen := make(map[string]struct{})
	statements := make([]*ast.Node, 0, len(sourceFile.Statements.Nodes))
	changed := false
	for _, statement := range sourceFile.Statements.Nodes {
		if _, isAuthored := authored[statement]; isAuthored {
			statements = append(statements, statement)
			continue
		}
		if !ast.IsImportDeclaration(statement) {
			statements = append(statements, statement)
			continue
		}
		declaration := statement.AsImportDeclaration()
		if !ast.IsStringLiteral(declaration.ModuleSpecifier) || declaration.ImportClause == nil {
			statements = append(statements, statement)
			continue
		}
		clause := declaration.ImportClause.AsImportClause()
		if clause.Name() != nil || clause.NamedBindings == nil || !ast.IsNamedImports(clause.NamedBindings) {
			statements = append(statements, statement)
			continue
		}
		key := sourceNamedImportGroupKey(statement)
		position, found := positions[key]
		if !found {
			position = len(statements)
			positions[key] = position
			bases[key] = statement
			statements = append(statements, statement)
		} else {
			changed = true
		}
		for _, element := range clause.NamedBindings.AsNamedImports().Elements.Nodes {
			specifier := element.AsImportSpecifier()
			specifierKey := sourceImportSpecifierKey(element)
			if _, suppliedBySource := authoredBindings[key][specifierKey]; suppliedBySource {
				changed = true
				continue
			}
			exported := specifier.Name().Text()
			if specifier.PropertyName != nil {
				exported = specifier.PropertyName.Text()
			}
			elementKey := fmt.Sprintf("%s|%t|%s|%s", key, specifier.IsTypeOnly, exported, specifier.Name().Text())
			if _, duplicate := seen[elementKey]; duplicate {
				changed = true
				continue
			}
			seen[elementKey] = struct{}{}
			if specifier.PropertyName == nil {
				changed = true
				element = factory.NewImportSpecifier(
					specifier.IsTypeOnly,
					factory.NewIdentifier(exported),
					factory.NewIdentifier(specifier.Name().Text()),
				)
			}
			elements[key] = append(elements[key], element)
		}
		base := bases[key].AsImportDeclaration()
		baseClause := base.ImportClause.AsImportClause()
		named := factory.UpdateNamedImports(baseClause.NamedBindings.AsNamedImports(), factory.NewNodeList(elements[key]))
		updatedClause := factory.UpdateImportClause(baseClause, baseClause.PhaseModifier, baseClause.Name(), named)
		statements[position] = factory.UpdateImportDeclaration(base, base.Modifiers(), updatedClause, base.ModuleSpecifier, base.Attributes)
	}
	if !changed {
		return sourceFile
	}
	return factory.UpdateSourceFile(sourceFile, factory.NewNodeList(statements), sourceFile.EndOfFileToken).AsSourceFile()
}

func sourceNamedImportKey(node *ast.Node) string {
	if !ast.IsImportDeclaration(node) {
		return ""
	}
	declaration := node.AsImportDeclaration()
	if declaration.ImportClause == nil {
		return ""
	}
	clause := declaration.ImportClause.AsImportClause()
	if clause.Name() != nil || clause.NamedBindings == nil || !ast.IsNamedImports(clause.NamedBindings) {
		return ""
	}
	return fmt.Sprintf("%d|%s", clause.PhaseModifier, namedImportKey(node))
}

func sourceNamedImportGroupKey(node *ast.Node) string {
	if sourceNamedImportKey(node) == "" {
		return ""
	}
	declaration := node.AsImportDeclaration()
	clause := declaration.ImportClause.AsImportClause()
	return fmt.Sprintf("%s|%d", declaration.ModuleSpecifier.Text(), clause.PhaseModifier)
}

func sourceImportSpecifierKey(node *ast.Node) string {
	specifier := node.AsImportSpecifier()
	exported := specifier.Name().Text()
	if specifier.PropertyName != nil {
		exported = specifier.PropertyName.Text()
	}
	return fmt.Sprintf("%t|%s|%s", specifier.IsTypeOnly, exported, specifier.Name().Text())
}
