package transformer_test

import (
	"strings"
	"testing"
)

// Under `--!native` Luau only gets a speedup from parameter annotations it
// understands; these cases pin which TS types are annotated and how.
func TestNativeFilesAnnotateSignatures(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"--!native\n",
		"local function addVector(a: vector, b: vector): vector\n",
		"local function maybeAddVector(a: vector, b: vector?): vector\n",
		"local function addVector3(a: Vector3, b: Vector3): Vector3\n",
		"local function scale(x: number, factor: number?, label: string?): number\n",
		"local function nameOf(value: string?): string?\n",
		"local function sumGeneric(value: number): number\n",
		"local arrow = function(n: number): number\n",
		"function Counter:constructor(start: number)\n",
		"function Counter:next(step: number, other): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

// Table types are spelled structurally so Luau's compiler tags them TABLE.
func TestNativeFilesAnnotateTablesStructurally(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function sum(values: { number }, lookup: { [string]: number }, options: { size: number }): number\n",
		// Array, ReadonlyArray, tuple, and an array return.
		"local function arrayTypes(values: { number }, readonlyValues: { string }, pair: { string | number }): { number }\n",
		// Map, Set, Record.
		"local function collectionTypes(lookup: { [string]: number }, keys: { [string]: true }, counts: { [string]: number }): number\n",
		// Interface with an optional property, object literal type with a
		// Roblox member, and an optional parameter of a table type.
		"local function structs(foo: { a: number, b: string? }, balls: { x: number, y: Vector3 }, maybe: { a: number, b: string? }?): { a: number, b: string? }\n",
		// Nested table types.
		"local function nested(index: { [string]: { number } }): { [string]: { number } }\n",
		// A self-referential interface stops at the table fallback instead of
		// recursing, and a method-typed member widens to `any`.
		"local function linked(node: { value: number, next: { [any]: any }? }, thing: { value: number, run: any }): number\n",
		// A mixed element union widens to `any`.
		"local function mixed(value, flag, anything): { any }\n",
		// Property names that are not Luau identifiers are quoted.
		"local function quirky(value: { [\"kebab-case\"]: number, [\"1\"]: string }): { [\"kebab-case\"]: number, [\"1\"]: string }\n",
		// A number index signature.
		"local function recordTypes(numbers: { [number]: string }): { [number]: string }\n",
		// Luau allows one table indexer, so both TS signatures collapse.
		"local function bothIndex(index: { [any]: number }): { [any]: number }\n",
		// A union of table types keeps every member; an optional one is
		// parenthesised so the `?` covers the whole union.
		"local function unionTables(value: { string } | { number }): { string } | { number }\n",
		"local function unionOptional(value: ({ string } | { number })?): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

func TestNativeFilesLeaveUnhelpfulTypesUnannotated(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function callback(fn): number\n",
		"local loadAsync = TS.async(function(count: number)\n",
		// Roblox types other than the two vectors are datatypes, not tables.
		"local function instanceOf(value)\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

// A bare `table` annotation is tagged userdata by Luau's compiler, which makes
// the native entry guard fail on every call.
func TestNativeFilesNeverEmitBareTable(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")
	for _, line := range strings.Split(got, "\n") {
		if strings.Contains(line, ": table") {
			t.Errorf("bare table annotation: %s", line)
		}
	}
}

func TestFilesWithoutNativeDirectiveAreNotAnnotated(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/nonative.ts")

	for _, want := range []string{
		"local function addVector(a, b)\n",
		"local function scale(x, factor, label)\n",
		"local function sum(values, lookup, options)\n",
		"local function arrayTypes(values, readonlyValues, pair)\n",
		"local function structs(foo, balls, maybe)\n",
		"local function linked(node, thing)\n",
		"local function quirky(value)\n",
		"local function unionOptional(value)\n",
		"local arrow = function(n)\n",
		"function Counter:constructor(start)\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}
