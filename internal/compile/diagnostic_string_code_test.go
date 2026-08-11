package compile

import (
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
)

func TestInfoFromTSDiag_prefersConcreteStringCodeAndMessage(t *testing.T) {
	// Given: a Flamework warning carried by the concrete diagnostic seam.
	diagnostic := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), " @flamework/core", diagnostics.CategoryWarning, "plugin warning")

	// When: the compiler converts the AST diagnostic to its public structure.
	info := infoFromTSDiag(diagnostic, nil)

	// Then: exact plugin code/text/category survive without a synthetic TS0 code.
	if info.Code != " @flamework/core" || info.Message != "plugin warning" || !info.Warning || info.FileName != "" {
		t.Fatalf("DiagnosticInfo = %#v", info)
	}
}

func TestInfoFromTSDiag_preservesGeneratedTypeScriptPath(t *testing.T) {
	// Given: an ordinary generated TypeScript error.
	diagnostic := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Identifier_expected)

	// When: the compiler converts it through the same adapter.
	info := infoFromTSDiag(diagnostic, nil)

	// Then: numeric code formatting and localized text remain unchanged.
	if info.Code != "TS1003" || info.Message != "Identifier expected." || info.Warning {
		t.Fatalf("DiagnosticInfo = %#v", info)
	}
}

func TestTSDiagnosticInfos_preservesMultipleOrderedStringCodeDiagnostics(t *testing.T) {
	// Given: distinct plugin diagnostics already ordered by the compiler collection.
	diagnostics := []*ast.Diagnostic{
		ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin-a", diagnostics.CategoryWarning, "first message"),
		ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin-b", diagnostics.CategoryError, "second message"),
	}

	// When: the compile adapter converts the complete tuple sequence.
	got := tsDiagnosticInfos(diagnostics, nil)

	// Then: every code/message/category tuple remains ordered and distinct.
	if len(got) != 2 || got[0].Code != "plugin-a" || got[0].Message != "first message" || !got[0].Warning || got[1].Code != "plugin-b" || got[1].Message != "second message" || got[1].Warning {
		t.Fatalf("DiagnosticInfo tuples = %#v", got)
	}
}
