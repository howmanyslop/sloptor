package flamework

import (
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)

func TestPrependFlameworkReflectImport_deduplicatesAliasAndPreservesFirstComment(t *testing.T) {
	// Given
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/input.ts", Path: tspath.Path("/input.ts")}, `// @directive
import { Reflect as RuntimeReflect } from "@flamework/core";
class Counter {}
`, core.ScriptKindTS)
	factory := ast.NewNodeFactory(ast.NodeFactoryHooks{})

	// When
	transformed := prependFlameworkReflectImport(factory, file)
	// Then
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if strings.Count(printed, `from "@flamework/core"`) != 1 {
		t.Fatalf("Reflect import count mismatch:\n%s", printed)
	}
	if !strings.HasPrefix(printed, "// @directive\nimport") {
		t.Fatalf("first-statement comment was not preserved:\n%s", printed)
	}
}

func TestPrependFlameworkReflectImport_placesNewImportAfterFirstStatementComment(t *testing.T) {
	// Given
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/input.ts", Path: tspath.Path("/input.ts")}, `// @directive
class Counter {}
`, core.ScriptKindTS)

	// When
	transformed := prependFlameworkReflectImport(ast.NewNodeFactory(ast.NodeFactoryHooks{}), file)
	// Then
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.HasPrefix(printed, "// @directive\nimport { Reflect }") {
		t.Fatalf("comment/import order mismatch:\n%s", printed)
	}
	if strings.Count(printed, "// @directive") != 1 {
		t.Fatalf("first comment duplicated:\n%s", printed)
	}
}

func TestPrependFlameworkReflectImport_hoistsAndDeduplicatesGeneratedPrerequisiteImports(t *testing.T) {
	// Given
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/input.ts", Path: tspath.Path("/input.ts")}, `// @directive
const before = true;
`, core.ScriptKindTS)
	factory := ast.NewNodeFactory(ast.NodeFactoryHooks{})
	generated := macroImportStatements(factory, []MacroImport{
		{Module: "@flamework/core", Export: "Flamework", Local: "Flamework"},
		{Module: "@flamework/core", Export: "Flamework", Local: "Flamework"},
	})
	statements := append([]*ast.Node(nil), file.Statements.Nodes...)
	statements = append(statements, generated...)
	file = factory.UpdateSourceFile(file, factory.NewNodeList(statements), file.EndOfFileToken).AsSourceFile()

	// When
	transformed := prependFlameworkReflectImport(factory, file)
	// Then
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	reflectIndex := strings.Index(printed, `import { Reflect }`)
	flameworkIndex := strings.Index(printed, `import { Flamework }`)
	statementIndex := strings.Index(printed, `const before`)
	if reflectIndex < 0 || flameworkIndex < 0 || statementIndex < 0 || reflectIndex > statementIndex || flameworkIndex > statementIndex {
		t.Fatalf("generated import prerequisite order mismatch:\n%s", printed)
	}
	if strings.Count(printed, `import { Flamework }`) != 1 || strings.Count(printed, "// @directive") != 1 {
		t.Fatalf("generated import dedupe/comment mismatch:\n%s", printed)
	}
}

func TestPrependFlameworkReflectImport_movesFirstImportCommentToHoistedGeneratedImport(t *testing.T) {
	// Given
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/input.ts", Path: tspath.Path("/input.ts")}, `// @directive
import { Reflect as RuntimeReflect } from "@flamework/core";
class Counter {}
`, core.ScriptKindTS)
	factory := ast.NewNodeFactory(ast.NodeFactoryHooks{})
	statements := append(macroImportStatements(factory, []MacroImport{{Module: flameworkPreludeModule, Export: "t", Local: "t"}}), file.Statements.Nodes...)
	file = factory.UpdateSourceFile(file, factory.NewNodeList(statements), file.EndOfFileToken).AsSourceFile()

	// When
	transformed := prependFlameworkReflectImport(factory, file)
	// Then
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	if !strings.HasPrefix(printed, `// @directive
import { t } from "@flamework/core/out/prelude";`) || strings.Count(printed, "// @directive") != 1 {
		t.Fatalf("hoisted import comment transfer mismatch:\n%s", printed)
	}
}
