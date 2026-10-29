package transformer_test

import (
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// The alias registry seen through whole-file output: which `type` statements
// a file gets, where they go (at the declaration or hoisted after the
// header), what they are called, which carry `export`, and which losses the
// bodies record. Every fixture is also handed to the Luau analyzer.

// renderLuauTypesLosses is renderLuauTypes plus the loss kinds the file
// recorded, by name.
func renderLuauTypesLosses(t *testing.T, relPath string) (string, []string) {
	t.Helper()
	projDir := filepath.Join("testdata", "luautypes")
	s := buildState(t, projDir, relPath)
	s.LuauTypes = true
	s.Rojo = luauTypesRojoContext(t, projDir)
	statements := transformer.TransformSourceFile(s)
	if ds := s.Diags.Flush(); len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %v", ds)
	}
	var kinds []string
	for _, loss := range s.LuauTypeLosses() {
		if name := loss.Kind.String(); !slices.Contains(kinds, name) {
			kinds = append(kinds, name)
		}
	}
	return render.RenderAST(statements), kinds
}

// assertLinesInOrder requires every line of want to appear in got, as a
// whole line, in the given order.
func assertLinesInOrder(t *testing.T, got string, want []string) {
	t.Helper()
	lines := strings.Split(got, "\n")
	next := 0
	for _, line := range want {
		index := slices.Index(lines[next:], line)
		if index < 0 {
			if slices.Contains(lines, line) {
				t.Errorf("line %q is out of order", line)
			} else {
				t.Errorf("line %q is missing", line)
			}
			continue
		}
		next += index + 1
	}
	if t.Failed() {
		t.Logf("rendered:\n%s", got)
	}
}

func TestLuauTypesAliasesAtDeclaration(t *testing.T) {
	got, losses := renderLuauTypesLosses(t, "src/aliases.ts")

	// Top-level declarations stay in place and in source order, with
	// `export` for the exported ones (`export type { Listed }` included),
	// defaults on generic aliases, a dropped keyword-named parameter whose
	// references widen to any, recursion by name, a merged interface emitted
	// once, union / literal / callable bodies, a Roblox-name collision
	// suffixed, and a loss comment before the alias whose recursion used
	// other arguments.
	assertLinesInOrder(t, got, []string{
		"export type Point = { x: number, y: number }",
		"type Hidden = { secret: number }",
		"export type Listed = { id: string }",
		"export type Boxed<T = number> = { value: T }",
		"export type Pair<A, B = A> = { first: A, second: B }",
		"export type Labeled<T = any> = { label: any, value: T }",
		"export type Tree = { value: number, children: { Tree } }",
		"export type Chain<T> = { value: T, next: Chain<T>? }",
		"-- luauTypes: Weird<T[]> spelled as any: a recursive alias must be referenced with its own type arguments",
		"export type Weird<T> = { value: T, nested: any }",
		"export type Merged = { a: number, b: string }",
		`export type Mode = "r" | "w"`,
		`export type Literal = "x"`,
		"export type Handler = (value: number) -> ()",
		"export type Part_2 = { id: number }",
		"export type Holder = {",
		"\tpoint: Point,",
		"\thidden: Hidden,",
		"\tboxed: Boxed<number>,",
		"\tpair: Pair<number, number>,",
		"\tlabeled: Labeled<number>,",
		"\tweird: Weird<number>,",
		"\tmode: Mode,",
		`	literal: "x",`,
		"\tpart: Part_2,",
		"\tsize: Geometry_Size,",
		"}",
	})

	// The nested namespace member is hoisted after the header, before the
	// file's own leading comment, under its flattened name and still
	// exported.
	header := strings.Index(got, "\nexport type Geometry_Size = { width: number, height: number }\n")
	comment := strings.Index(got, "-- Aliases: exported and local interfaces")
	first := strings.Index(got, "export type Point =")
	if header < 0 || comment < 0 || header > comment || header > first {
		t.Errorf("Geometry_Size must be hoisted after the header, before the first statement:\n%s", got)
	}

	for _, unwanted := range []string{"type Tuple", "type Geometry =", "type Part =", "type Size ="} {
		if strings.Contains(got, unwanted) {
			t.Errorf("output must not contain %q:\n%s", unwanted, got)
		}
	}
	if count := strings.Count(got, "type Merged ="); count != 1 {
		t.Errorf("a merged interface emits one alias, got %d:\n%s", count, got)
	}
	if count := strings.Count(got, "type Point ="); count != 1 {
		t.Errorf("an alias referenced before and after its declaration is emitted once, got %d:\n%s", count, got)
	}

	// RecursiveAliasArgs from Weird<T[]>, Unspellable from the keyword-named
	// parameter of Labeled; the LuaTuple alias is declined without a loss
	// and its only use is a return, spelled as a pack.
	for _, want := range []string{"RecursiveAliasArgs", "Unspellable"} {
		if !slices.Contains(losses, want) {
			t.Errorf("losses %v must include %s", losses, want)
		}
	}
	for _, unwanted := range []string{"MappedType", "ConditionalType", "DepthExceeded"} {
		if slices.Contains(losses, unwanted) {
			t.Errorf("losses %v must not include %s", losses, unwanted)
		}
	}

	findings := analyzeLuauStrict(t, findLuauAnalyzer(t), got, true)
	if len(findings) != 0 {
		t.Errorf("aliases.ts does not analyze clean:\n%s\n--- rendered ---\n%s", strings.Join(findings, "\n"), got)
	}
}

func TestLuauTypesAliasesMirrorImports(t *testing.T) {
	got, losses := renderLuauTypesLosses(t, "src/imports.ts")

	// Every imported type is mirrored once as a hoisted, non-exported alias
	// under its local import name (`Box as Container`), in first-use order,
	// and the lib's Promise<T> is mirrored the same way with its namespace
	// member flattened; the file's own interface is exported at its
	// declaration and references the mirrors.
	assertLinesInOrder(t, got, []string{
		"local TS = _G[script]",
		"type Shape = { kind: ShapeKind, area: (self: Shape) -> number }",
		`type ShapeKind = "circle" | "square"`,
		"type Container<T> = { item: T }",
		"-- luauTypes: Promise<TResult1>, Promise<TResult2>, Promise<TResult1 | TResult2>, Promise<TResult>, Promise<T | TResult>, Promise<R>, Promise<U> spelled as any: a recursive alias must be referenced with its own type arguments",
		"type Promise<T> = {",
		"\ttap: (self: Promise<T>, tapHandler: (value: T) -> ()) -> Promise<T>,",
		"\tcancel: (self: Promise<T>) -> (),",
		"\tawaitStatus: (self: Promise<T>) -> (Promise_Status, unknown),",
		"}",
		`type Promise_Status = "Cancelled" | "Rejected" | "Resolved" | "Started"`,
		"-- Cross-file types: a type-only import, a renamed import of a generic",
		`local _shapes = TS.import(script, script.Parent, "lib", "shapes")`,
		"export type Wrapped = {",
		"\tshape: Shape,",
		"\tkind: ShapeKind,",
		"\tcontainer: Container<Shape>,",
		"\tpending: Promise<number>,",
		"}",
	})

	for _, unwanted := range []string{"export type Shape", "export type ShapeKind", "export type Container", "export type Promise", "type Box<", "type Box ="} {
		if strings.Contains(got, unwanted) {
			t.Errorf("output must not contain %q:\n%s", unwanted, got)
		}
	}
	for _, name := range []string{"type Shape =", "type ShapeKind =", "type Container<T> =", "type Promise<T> =", "type Promise_Status ="} {
		if count := strings.Count(got, name); count != 1 {
			t.Errorf("%q must be mirrored exactly once, got %d:\n%s", name, count, got)
		}
	}
	if !slices.Contains(losses, "RecursiveAliasArgs") {
		t.Errorf("mirroring Promise<T> must record RecursiveAliasArgs, got %v", losses)
	}

	findings := analyzeLuauStrict(t, findLuauAnalyzer(t), got, true)
	if len(findings) != 0 {
		t.Errorf("imports.ts does not analyze clean:\n%s\n--- rendered ---\n%s", strings.Join(findings, "\n"), got)
	}
}

// The module a mirror comes from still exports its own aliases at their
// declarations; a generic interface keeps its generics.
func TestLuauTypesAliasesExportedFromSibling(t *testing.T) {
	got, _ := renderLuauTypesLosses(t, "src/lib/shapes.ts")
	assertLinesInOrder(t, got, []string{
		"export type Shape = { kind: ShapeKind, area: (self: Shape) -> number }",
		`export type ShapeKind = "circle" | "square"`,
		"export type Box<T> = { item: T }",
	})
	if strings.Contains(got, "type Container") {
		t.Errorf("the local import name of another file must not leak into the declaring file:\n%s", got)
	}
}

// Native output is untouched: with luauTypes off, a file without the
// `//!native` directive emits no alias at all.
func TestLuauTypesAliasesOffEmitNothing(t *testing.T) {
	projDir := filepath.Join("testdata", "luautypes")
	s := buildState(t, projDir, "src/aliases.ts")
	s.Rojo = luauTypesRojoContext(t, projDir)
	got := render.RenderAST(transformer.TransformSourceFile(s))
	if strings.Contains(got, "type ") || strings.Contains(got, "luauTypes:") {
		t.Errorf("luauTypes off must emit no alias:\n%s", got)
	}
}
