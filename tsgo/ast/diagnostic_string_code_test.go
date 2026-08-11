package ast_test

import (
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/parser"
)

func TestDiagnosticStringCode_preservesPluginTupleAndRelatedInformation(t *testing.T) {
	// Given: a plugin warning located at EOF with an ordered related diagnostic.
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/tmp/task7-diagnostic/src/index.ts"}, strings.Repeat("x", 206), core.ScriptKindTS)
	related := ast.NewDiagnosticWithStringCode(file, core.NewTextRange(10, 14), " @flamework/core", diagnostics.CategoryMessage, "related")

	// When: the arbitrary-text diagnostic is constructed and related information is attached.
	diagnostic := ast.NewDiagnosticWithStringCode(file, core.NewTextRange(206, 206), " @flamework/core", diagnostics.CategoryWarning, "Valid `@rbxts/t` was not found, guard generation may not work.")
	diagnostic.AddRelatedInfo(related)

	// Then: code, text, category, location, and related ordering remain exact.
	code, ok := diagnostic.StringCode()
	if !ok || code != " @flamework/core" || diagnostic.String() != "Valid `@rbxts/t` was not found, guard generation may not work." || diagnostic.Category() != diagnostics.CategoryWarning || diagnostic.Pos() != 206 || diagnostic.Len() != 0 || diagnostic.File() != file {
		t.Fatalf("plugin diagnostic = code %q,%v text %q category %v pos %d len %d file %p", code, ok, diagnostic.String(), diagnostic.Category(), diagnostic.Pos(), diagnostic.Len(), diagnostic.File())
	}
	if got := diagnostic.RelatedInformation(); len(got) != 1 || got[0] != related {
		t.Fatalf("related information = %#v, want exact ordered entry", got)
	}
}

func TestDiagnosticStringCode_supportsNoFileFallback_withoutChangingTypeScriptDiagnostics(t *testing.T) {
	// Given: a no-file plugin failure and an ordinary generated TypeScript diagnostic.
	plugin := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "sidecar-internal", diagnostics.CategoryError, "malformed input")
	typescript := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Identifier_expected)

	// When: consumers inspect both representations.
	pluginCode, pluginOK := plugin.StringCode()
	typescriptCode, typescriptOK := typescript.StringCode()

	// Then: plugin payload survives while the zero-value TS path is unchanged.
	if !pluginOK || pluginCode != "sidecar-internal" || plugin.String() != "malformed input" || plugin.File() != nil || plugin.Pos() != -1 {
		t.Fatalf("no-file plugin diagnostic = code %q,%v text %q file %#v pos %d", pluginCode, pluginOK, plugin.String(), plugin.File(), plugin.Pos())
	}
	if typescriptOK || typescriptCode != "" || typescript.Code() != diagnostics.Identifier_expected.Code() || typescript.String() != "Identifier expected." {
		t.Fatalf("TypeScript diagnostic changed = code %q,%v numeric %d text %q", typescriptCode, typescriptOK, typescript.Code(), typescript.String())
	}
}

func TestDiagnosticIdentity_distinguishesStringCodeRepresentations(t *testing.T) {
	// Given: diagnostics that previously shared numeric code zero, location, and arguments.
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/tmp/diagnostic-identity/index.ts"}, "const value = 1", core.ScriptKindTS)
	loc := core.NewTextRange(0, 5)
	numericZero := ast.NewDiagnosticFromSerialized(nil, core.UndefinedTextRange(), 0, diagnostics.CategoryError, "", nil, nil, nil, false, false, false)

	tests := []struct {
		name  string
		left  *ast.Diagnostic
		right *ast.Diagnostic
	}{
		{
			name:  "different string codes at the same location",
			left:  ast.NewDiagnosticWithStringCode(file, loc, "plugin-a", diagnostics.CategoryWarning, "same text"),
			right: ast.NewDiagnosticWithStringCode(file, loc, "plugin-b", diagnostics.CategoryWarning, "same text"),
		},
		{
			name:  "different raw messages with the same string code",
			left:  ast.NewDiagnosticWithStringCode(file, loc, "plugin", diagnostics.CategoryWarning, "first text"),
			right: ast.NewDiagnosticWithStringCode(file, loc, "plugin", diagnostics.CategoryWarning, "second text"),
		},
		{
			name:  "different no file diagnostics with undefined locations",
			left:  ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin-a", diagnostics.CategoryError, "same text"),
			right: ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin-b", diagnostics.CategoryError, "same text"),
		},
		{
			name:  "numeric zero and string code representations",
			left:  numericZero,
			right: ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryError, "numeric zero lookalike"),
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// When: diagnostic identity is compared in both directions.
			forward := ast.CompareDiagnostics(test.left, test.right)
			reverse := ast.CompareDiagnostics(test.right, test.left)

			// Then: distinct string representations neither compare nor equal alike.
			if forward == 0 || reverse == 0 || forward != -reverse || ast.EqualDiagnosticsNoRelatedInfo(test.left, test.right) || ast.EqualDiagnostics(test.left, test.right) {
				t.Fatalf("identity comparison = forward %d reverse %d equal without related %v equal %v", forward, reverse, ast.EqualDiagnosticsNoRelatedInfo(test.left, test.right), ast.EqualDiagnostics(test.left, test.right))
			}
		})
	}
}

func TestDiagnosticsCollection_LookupKeepsDistinctStringCodeDiagnostics(t *testing.T) {
	// Given: two same-location plugin diagnostics and an identical duplicate.
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: "/tmp/diagnostic-collection/index.ts"}, "const value = 1", core.ScriptKindTS)
	loc := core.NewTextRange(0, 5)
	first := ast.NewDiagnosticWithStringCode(file, loc, "plugin-a", diagnostics.CategoryWarning, "same text")
	second := ast.NewDiagnosticWithStringCode(file, loc, "plugin-b", diagnostics.CategoryWarning, "same text")
	duplicate := ast.NewDiagnosticWithStringCode(file, loc, "plugin-a", diagnostics.CategoryWarning, "same text")
	var collection ast.DiagnosticsCollection
	collection.Add(second)
	collection.Add(first)

	// When: the collection sorts for lookup.
	got := collection.GetDiagnosticsForFile(file.FileName())
	firstLookup := collection.Lookup(first)
	secondLookup := collection.Lookup(second)
	duplicateLookup := collection.Lookup(duplicate)

	// Then: each distinct diagnostic resolves to itself, while an identical tuple resolves to its canonical entry.
	if len(got) != 2 || got[0] != first || got[1] != second || firstLookup != first || secondLookup != second || duplicateLookup != first {
		t.Fatalf("collection entries = %#v; lookups = %p, %p, %p; want sorted distinct entries and canonical duplicate", got, firstLookup, secondLookup, duplicateLookup)
	}
}

func TestDiagnosticIdentity_preservesMessageArgumentsAndRelatedInformation(t *testing.T) {
	// Given: numeric diagnostics that differ in arguments or ordered string-code related information.
	withFirstArgument := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Cannot_find_name_0, "first")
	withSecondArgument := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Cannot_find_name_0, "second")
	withFirstRelated := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Identifier_expected)
	withFirstRelated.AddRelatedInfo(ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryMessage, "first related"))
	withSecondRelated := ast.NewDiagnostic(nil, core.UndefinedTextRange(), diagnostics.Identifier_expected)
	withSecondRelated.AddRelatedInfo(ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryMessage, "second related"))

	// When: established diagnostic dimensions are compared alongside the new representation dimension.
	argumentComparison := ast.CompareDiagnostics(withFirstArgument, withSecondArgument)
	relatedComparison := ast.CompareDiagnostics(withFirstRelated, withSecondRelated)

	// Then: message arguments and related information remain part of the total identity.
	if argumentComparison == 0 || ast.EqualDiagnostics(withFirstArgument, withSecondArgument) || relatedComparison == 0 || ast.EqualDiagnostics(withFirstRelated, withSecondRelated) {
		t.Fatalf("argument comparison = %d related comparison = %d", argumentComparison, relatedComparison)
	}
}
