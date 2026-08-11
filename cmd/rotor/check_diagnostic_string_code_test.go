package main

import (
	"testing"

	"rotor/internal/compile"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/diagnosticwriter"
)

func TestJSONDiagnostics_prefersConcreteStringCodeAndMessage(t *testing.T) {
	// Given: one plugin diagnostic with no source file and one generated TypeScript diagnostic.
	plugin := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), " @flamework/core", diagnostics.CategoryWarning, "plugin warning")
	typescript := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Identifier_expected)

	// When: check serializes both diagnostics to its JSON structure.
	got := jsonDiagnostics([]*ast.Diagnostic{plugin, typescript}, &diagnosticwriter.FormattingOptions{})

	// Then: plugin payload is preferred and TypeScript formatting is unchanged.
	if len(got) != 2 || got[0].Code != " @flamework/core" || got[0].Message != "plugin warning" || got[0].Severity != "warning" || got[1].Code != compile.TypeScriptDiagnosticCode(typescript.Code()) || got[1].Message != "Identifier expected." || got[1].Severity != "error" {
		t.Fatalf("JSON diagnostics = %#v", got)
	}
}
