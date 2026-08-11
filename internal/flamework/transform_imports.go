package flamework

import (
	"strings"

	"rotor/tsgo/ast"
)

const flameworkCoreModule = "@flamework/core"

func newReflectImport(factory *ast.NodeFactory) *ast.Node {
	reflectIdentifier := factory.NewIdentifier("Reflect")
	specifier := factory.NewImportSpecifier(false, nil, reflectIdentifier)
	namedImports := factory.NewNamedImports(factory.NewNodeList([]*ast.Node{specifier}))
	clause := factory.NewImportClause(ast.KindUnknown, nil, namedImports)
	return factory.NewImportDeclaration(nil, clause, factory.NewStringLiteral(flameworkCoreModule, ast.TokenFlagsNone), nil)
}

func prependFlameworkReflectImport(factory *ast.NodeFactory, sourceFile *ast.SourceFile) *ast.SourceFile {
	generatedImports := make([]*ast.Node, 0)
	sourceImports := make([]*ast.Node, 0)
	statements := make([]*ast.Node, 0, len(sourceFile.Statements.Nodes))
	for _, statement := range sourceFile.Statements.Nodes {
		if !ast.IsImportDeclaration(statement) {
			statements = append(statements, statement)
			continue
		}
		if statement.Pos() < 0 {
			generatedImports = append(generatedImports, statement)
		} else {
			sourceImports = append(sourceImports, statement)
		}
	}
	imports := make([]*ast.Node, 0, len(generatedImports)+len(sourceImports)+1)
	if _, found := flameworkReflectImport(sourceFile); !found {
		imports = append(imports, newReflectImport(factory))
	}
	imports = append(imports, generatedImports...)
	imports = append(imports, sourceImports...)
	imports = deduplicateNamedImports(imports)
	var firstOriginal *ast.Node
	for _, statement := range sourceFile.Statements.Nodes {
		if statement.Pos() >= 0 {
			firstOriginal = statement
			break
		}
	}
	if len(imports) > 0 && firstOriginal != nil && imports[0] != firstOriginal {
		first := firstOriginal
		imports[0].Loc = first.Loc
		if ast.IsImportDeclaration(first) {
			for index, importNode := range imports {
				if importNode == first {
					imports[index] = factory.DeepCloneNode(first)
					break
				}
			}
		} else {
			for index, statement := range statements {
				if statement == first {
					statements[index] = factory.DeepCloneNode(first)
					break
				}
			}
		}
	}
	statements = append(imports, statements...)
	return factory.UpdateSourceFile(sourceFile, factory.NewNodeList(statements), sourceFile.EndOfFileToken).AsSourceFile()
}

func deduplicateNamedImports(imports []*ast.Node) []*ast.Node {
	seen := make(map[string]struct{}, len(imports))
	result := make([]*ast.Node, 0, len(imports))
	for _, importNode := range imports {
		key := namedImportKey(importNode)
		if key != "" {
			if _, found := seen[key]; found {
				continue
			}
			seen[key] = struct{}{}
		}
		result = append(result, importNode)
	}
	return result
}

func namedImportKey(node *ast.Node) string {
	declaration := node.AsImportDeclaration()
	if !ast.IsStringLiteral(declaration.ModuleSpecifier) || declaration.ImportClause == nil {
		return ""
	}
	clause := declaration.ImportClause.AsImportClause()
	if clause.Name() != nil || clause.NamedBindings == nil || !ast.IsNamedImports(clause.NamedBindings) {
		return ""
	}
	var key strings.Builder
	key.WriteString(declaration.ModuleSpecifier.Text())
	for _, specifierNode := range clause.NamedBindings.AsNamedImports().Elements.Nodes {
		specifier := specifierNode.AsImportSpecifier()
		key.WriteByte('|')
		if specifier.IsTypeOnly {
			key.WriteString("type:")
		}
		if specifier.PropertyName != nil {
			key.WriteString(specifier.PropertyName.Text())
		} else {
			key.WriteString(specifier.Name().Text())
		}
		key.WriteByte(':')
		key.WriteString(specifier.Name().Text())
	}
	return key.String()
}

func reflectIdentifierForNode(node *ast.Node) string {
	if node == nil {
		return "Reflect"
	}
	identifier, found := flameworkReflectImport(ast.GetSourceFileOfNode(node))
	if !found {
		return "Reflect"
	}
	return identifier
}

func flameworkReflectImport(sourceFile *ast.SourceFile) (string, bool) {
	if sourceFile == nil {
		return "", false
	}
	for _, statement := range sourceFile.Statements.Nodes {
		if !ast.IsImportDeclaration(statement) {
			continue
		}
		declaration := statement.AsImportDeclaration()
		if !ast.IsStringLiteral(declaration.ModuleSpecifier) || declaration.ModuleSpecifier.Text() != flameworkCoreModule || declaration.ImportClause == nil {
			continue
		}
		clause := declaration.ImportClause.AsImportClause()
		if clause.PhaseModifier == ast.KindTypeKeyword || clause.NamedBindings == nil || !ast.IsNamedImports(clause.NamedBindings) {
			continue
		}
		for _, specifierNode := range clause.NamedBindings.AsNamedImports().Elements.Nodes {
			specifier := specifierNode.AsImportSpecifier()
			if specifier.IsTypeOnly {
				continue
			}
			exported := specifier.Name().Text()
			if specifier.PropertyName != nil {
				exported = specifier.PropertyName.Text()
			}
			if exported == "Reflect" {
				return specifier.Name().Text(), true
			}
		}
	}
	return "", false
}
