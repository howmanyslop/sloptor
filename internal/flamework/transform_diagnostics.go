package flamework

import (
	"errors"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/scanner"
)

func captureFlameworkDiagnostic(state *TransformState, err error) bool {
	var macroErr *MacroError
	if errors.As(err, &macroErr) && macroErr.Node != nil {
		diagnostic := flameworkNodeDiagnostic(macroErr.Node, diagnostics.CategoryError, macroErr.Message)
		for _, related := range macroErr.RelatedInformation {
			if related.Node != nil {
				diagnostic.AddRelatedInfo(flameworkNodeDiagnostic(related.Node, diagnostics.CategoryMessage, related.Message))
			}
		}
		state.AddDiagnostic(diagnostic)
		return true
	}
	var guardErr *GuardGenerationError
	if errors.As(err, &guardErr) {
		file := state.program.GetSourceFile(guardErr.FileName)
		diagnostic := ast.NewDiagnosticWithStringCode(file, core.NewTextRange(guardErr.Start, guardErr.End), flameworkDiagnosticCode, diagnostics.CategoryError, guardErr.Error())
		for _, related := range guardErr.RelatedInformation {
			relatedFile := state.program.GetSourceFile(related.FileName)
			diagnostic.AddRelatedInfo(ast.NewDiagnosticWithStringCode(relatedFile, core.NewTextRange(related.Start, related.End), flameworkDiagnosticCode, diagnostics.CategoryError, "Type was defined here: "+related.TypeName))
		}
		state.AddDiagnostic(diagnostic)
		return true
	}
	return false
}

func flameworkNodeDiagnostic(node *ast.Node, category diagnostics.Category, message string) *ast.Diagnostic {
	file := ast.GetSourceFileOfNode(node)
	start := scanner.GetTokenPosOfNode(node, file, false)
	return ast.NewDiagnosticWithStringCode(file, core.NewTextRange(start, node.End()), flameworkDiagnosticCode, category, message)
}

func diagnosticFileName(diagnostic *ast.Diagnostic) string {
	if diagnostic == nil || diagnostic.File() == nil {
		return ""
	}
	return diagnostic.File().FileName()
}
