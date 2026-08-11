package flamework

import (
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/printer"
)

func TestNewTransformState_preservesSyntheticComment_whenNodeIsUpdatedAndPrinted(t *testing.T) {
	// Given
	base, sourceFile := newExpressionTransformFixture(t, "export {};\n")
	state, err := newTransformState(TransformInput{Program: base.program, Checker: base.checker, Files: []*ast.SourceFile{sourceFile}, Project: base.project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	statement := state.Factory().NewExpressionStatement(state.Factory().NewIdentifier("before"))

	// When
	updated := state.Factory().UpdateExpressionStatement(statement.AsExpressionStatement(), state.Factory().NewIdentifier("after"))
	if state.EmitContext().Original(updated) != statement {
		t.Fatal("updated node lost its source mapping")
	}
	state.EmitContext().AddSyntheticLeadingComment(updated, ast.KindSingleLineCommentTrivia, " @flamework metadata", true)
	transformed := state.Factory().UpdateSourceFile(sourceFile, state.Factory().NewNodeList([]*ast.Node{updated}), sourceFile.EndOfFileToken).AsSourceFile()
	metadata := SourceMetadata{transformed: transformed, emitContext: state.EmitContext()}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, metadata.EmitContext()).EmitSourceFile(metadata.Transformed())

	// Then
	if !strings.Contains(printed, "// @flamework metadata\nafter;") {
		t.Fatalf("printed synthetic metadata =\n%s", printed)
	}
	t.Logf("printed synthetic metadata:\n%s", printed)
}

func TestTransform_exposesEmitContext_forCallerPrinting(t *testing.T) {
	// Given
	base, sourceFile := newExpressionTransformFixture(t, "export {};\n")

	// When
	result, err := Transform(TransformInput{Program: base.program, Checker: base.checker, Files: []*ast.SourceFile{sourceFile}, Project: base.project})
	// Then
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	metadata := result.Sources[0]
	emitContext := metadata.EmitContext()
	if emitContext == nil {
		t.Fatal("SourceMetadata.EmitContext() is nil")
	}
	statement := metadata.Transformed().Statements.Nodes[0]
	emitContext.AddSyntheticLeadingComment(statement, ast.KindSingleLineCommentTrivia, " caller metadata", true)
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, emitContext).EmitSourceFile(metadata.Transformed())
	if !strings.Contains(printed, "// caller metadata\nexport {};") {
		t.Fatalf("caller-printed synthetic metadata =\n%s", printed)
	}
	t.Logf("caller-printed synthetic metadata:\n%s", printed)
}
