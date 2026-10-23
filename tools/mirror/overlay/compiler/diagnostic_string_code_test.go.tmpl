package compiler

import (
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
)

func TestSortAndDeduplicateDiagnostics_keepsDistinctStringCodeDiagnostics(t *testing.T) {
	tests := []struct {
		name   string
		first  *ast.Diagnostic
		second *ast.Diagnostic
		want   [][2]string
	}{
		{
			name:   "different codes at the same location",
			first:  ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin-b", diagnostics.CategoryWarning, "same text"),
			second: ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin-a", diagnostics.CategoryWarning, "same text"),
			want:   [][2]string{{"plugin-a", "same text"}, {"plugin-b", "same text"}},
		},
		{
			name:   "different messages with the same code",
			first:  ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryWarning, "second text"),
			second: ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryWarning, "first text"),
			want:   [][2]string{{"plugin", "first text"}, {"plugin", "second text"}},
		},
		{
			name:   "no file diagnostics at undefined locations",
			first:  ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "global-b", diagnostics.CategoryError, "same text"),
			second: ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "global-a", diagnostics.CategoryError, "same text"),
			want:   [][2]string{{"global-a", "same text"}, {"global-b", "same text"}},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// Given: the same diagnostics in opposite aggregation orders.
			orders := [][]*ast.Diagnostic{{test.first, test.second}, {test.second, test.first}}

			for _, input := range orders {
				// When: the real compiler aggregation compacts the diagnostics.
				got := SortAndDeduplicateDiagnostics(input)

				// Then: both tuples survive in a stable total order.
				if len(got) != len(test.want) {
					t.Fatalf("compacted diagnostics length = %d, want %d", len(got), len(test.want))
				}
				for i, want := range test.want {
					code, ok := got[i].StringCode()
					if !ok || code != want[0] || got[i].String() != want[1] {
						t.Fatalf("compacted diagnostic %d = code %q present %v text %q, want %q %q", i, code, ok, got[i].String(), want[0], want[1])
					}
				}
			}
		})
	}
}

func TestSortAndDeduplicateDiagnostics_deduplicatesIdenticalStringCodeDiagnostic(t *testing.T) {
	// Given: equal plugin diagnostic tuples at the same no-file location.
	first := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryWarning, "same text")
	duplicate := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryWarning, "same text")

	// When: the compiler aggregation compacts them.
	got := SortAndDeduplicateDiagnostics([]*ast.Diagnostic{first, duplicate})

	// Then: one canonical diagnostic remains and direct identity agrees.
	if ast.CompareDiagnostics(first, duplicate) != 0 || !ast.EqualDiagnostics(first, duplicate) || len(got) != 1 || got[0] != first {
		t.Fatalf("deduplicated diagnostics = %#v", got)
	}
}

func TestSortAndDeduplicateDiagnostics_keepsCategoryDistinctStringCodeDiagnostics(t *testing.T) {
	// Given: warning and error plugin diagnostics with the same file, range, code, and text.
	warning := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryWarning, "same text")
	errorDiagnostic := ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), "plugin", diagnostics.CategoryError, "same text")

	for _, input := range [][]*ast.Diagnostic{{warning, errorDiagnostic}, {errorDiagnostic, warning}} {
		// When: the real compiler aggregation sorts and compacts both input orders.
		got := SortAndDeduplicateDiagnostics(input)

		// Then: category-distinct records both survive in deterministic category order.
		if len(got) != 2 || got[0].Category() != diagnostics.CategoryWarning || got[1].Category() != diagnostics.CategoryError {
			t.Fatalf("category-distinct diagnostics = %#v, want warning then error", got)
		}
	}
}
