package transformer_test

import (
	"os"
	"path/filepath"
	"regexp"
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
		// A constrained generic uses its constraint, which is the perf type.
		"local function sumGeneric<T>(value: number): number\n",
		"local arrow = function(n: number): number\n",
		"function Counter:constructor(start: number)\n",
		"function Counter:next(step: number, other): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

// Table types that are not named interfaces are spelled structurally so Luau's
// compiler tags them TABLE.
func TestNativeFilesAnnotateTablesStructurally(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function sum(values: { number }, lookup: { [string]: number }, options: { size: number }): number\n",
		// Array, ReadonlyArray, tuple, and an array return.
		"local function arrayTypes(values: { number }, readonlyValues: { string }, pair: { string | number }): { number }\n",
		// Map, Set, Record.
		"local function collectionTypes(lookup: { [string]: number }, keys: { [string]: true }, counts: { [string]: number }): number\n",
		// Nested table types.
		"local function nested(index: { [string]: { number } }): { [string]: { number } }\n",
		// A mixed element union widens to `any`.
		"local function mixed(value, flag, anything): { any }\n",
		// Property names that are not Luau identifiers are quoted.
		"local function quirky(value: Quirky): Quirky\n",
		// A number index signature.
		"local function recordTypes(numbers: { [number]: string }): { [number]: string }\n",
		// Luau allows one table indexer, so both TS signatures collapse.
		"local function bothIndex(index: Both): Both\n",
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

// Named interfaces and non-generic type aliases of object types become file
// level aliases, which is what keeps a recursive type recursive.
func TestNativeFilesAliasNamedTypes(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		// Optional members use `?` and readonly is dropped: `read` is not
		// legal in a table type.
		"type Foo = { a: number, b: string? }\n",
		"type Balls = { x: number, y: Vector3 }\n",
		// A self reference resolves to the alias itself.
		"type LinkedNode = { value: number, next: LinkedNode? }\n",
		// A method member takes its receiver as the first Luau parameter, so
		// `thing:run()` lines up with the type.
		"type WithMethod = { value: number, run: (self: any) -> number }\n",
		"type Quirky = { [\"kebab-case\"]: number, [\"1\"]: string }\n",
		// A member that is a function type, optionally.
		"type Handler = { onEvent: (value: number) -> (), optional: ((value: string) -> number)? }\n",
		// Two different symbols with the same name keep separate aliases.
		"type Node = { value: number }\n",
		"type Node_2 = { text: string }\n",
		"local function collision(a: Node, b: Node_2): number\n",
		// A name Luau already reads as a type is suffixed.
		"type table_2 = { value: number }\n",
		"local function reserved(value: table_2): number\n",
		// References use the alias.
		"local function structs(foo: Foo, balls: Balls, maybe: Foo?): Foo\n",
		"local function linked(node: LinkedNode, thing: WithMethod): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}

	if strings.Count(got, "type Foo = ") != 1 {
		t.Errorf("Foo should be aliased exactly once, got:\n%s", got)
	}
	if strings.Contains(got, "read ") {
		t.Errorf("readonly must be dropped in:\n%s", got)
	}
}

// Function types are spelled out in parameters and returns.
func TestNativeFilesAnnotateFunctionTypes(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function callback(fn: (n: number) -> number): number\n",
		// Nested function-typed return.
		"local function functionReturn(make: (n: number) -> (s: string) -> boolean): (n: number) -> (s: string) -> boolean\n",
		// A non-generic function type alias keeps its name; a generic one is
		// expanded inline.
		"type Notify = (value: number) -> ()\n",
		"local function notify(cb: Notify, generic: (value: string) -> ()): ()\n",
		// void returns `()`.
		"local function handlers(handler: Handler?): ()\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}

	// A method's `run()` is reached through `thing:run()`.
	if !strings.Contains(got, "thing:run()") {
		t.Errorf("fixture must call a method through the alias:\n%s", got)
	}
}

// Luau spells far more callable shapes than a `(...any) -> ...any` fallback:
// several returns, construct signatures, generic signatures, and overload
// intersections all keep a useful tag or read better than the fallback.
func TestNativeFilesAnnotateCallableShapes(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		// LuaTuple is a return pack, in a declared return and a callback type
		// alike.
		"local function multiReturn(): (number, string)\n",
		"local function optionalReturn(): (number, string?)\n",
		"local function restReturn(): (number, ...string)\n",
		"local function emptyReturn(): ()\n",
		// `?` after a pack means "optional function", never "optional return",
		// so an optional LuaTuple keeps its parameters and loses the pack; a
		// declared return then stays unannotated.
		"local function maybeReturn()\n",
		// A LuaTuple of a plain array cannot be a pack either, so a callback
		// type keeps its parameters and returns `...any`.
		"local function looseTupleCallback(cb: () -> ...any): number\n",
		"local function tupleCallback(cb: () -> (number, string)): number\n",
		"local function maybeTupleCallback(cb: (() -> (number, string))?): ()\n",
		"type TupleHolder = { pair: () -> (number, string), empty: () -> () }\n",
		// A LuaTuple alias cannot be written, so it is inlined at the return
		// and widens everywhere else.
		"type PairHolder = { pair: any }\n",
		"local function pairAlias(): (number, string)\n",
		// A construct signature is the table roblox-ts emits: `new` plus the
		// statics, never an intersection of tables.
		"type FooFactory = { new: (x: number) -> Foo }\n",
		"local function factory(f: FooFactory): Foo\n",
		"type CtorHolder = { create: { new: (x: number) -> Foo } }\n",
		// A class value is spelled structurally, with its instance widened.
		"local function makeCounter(): { new: (start: number) -> any, make: (start: number) -> any, prototype: any }\n",
		// Overloads: the entry guard wins at a top-level parameter (an
		// intersection has no tag), precision wins everywhere else.
		"local function overloadedParam(fn: (...any) -> ...any): number\n",
		"type OverloadedMethod = { run: ((self: any, value: number) -> string) & ((self: any, value: string) -> number) }\n",
		"local function overloadedMethod(thing: OverloadedMethod): number\n",
		// A generic call signature is still a function type, and its type
		// parameters are in scope for the body.
		"local function genericCallback(identify: <T>(value: T) -> T): number\n",
		"type IdentityHolder = { identify: <T>(value: T) -> T, replace: <T, U>(value: T, next: U) -> U }\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}

	// A return pack is not a type: aliasing a LuaTuple would emit
	// `type Pair = (number, string)`, which does not parse.
	if strings.Contains(got, "type Pair =") {
		t.Errorf("a LuaTuple must never become an alias body:\n%s", got)
	}
}

// Nested positions mirror Roblox types by name; a top-level parameter never
// does (an Instance tag is userdata, not a speedup).
func TestNativeFilesAnnotateRobloxTypesNested(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function robloxTypes(lookup: { [string]: Vector3 }, part, nested: { part: Part, flag: boolean }): { [string]: Vector3 }\n",
		"local function instanceOf(value): Instance\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

// A class type parameter cannot be declared in Luau, so it maps to `any`: the
// parameter is left unannotated and a nested use widens to `any`.
func TestNativeFilesTreatClassTypeParametersAsAny(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"function Box:constructor(value)\n",
		"function Box:get()\n",
		"function Box:boxed(): { inner: any }\n",
		"local function boxed(box): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

// A bare generic is only tagged `any` when it belongs to the annotated
// function itself; an outer function's generic tags the argument as userdata,
// so that parameter stays unannotated (rule R1).
func TestNativeFilesOnlyAnnotateOwnGenericsAtTopLevel(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function ownGeneric<K>(key: K): K\n",
		// `other: K` names the enclosing function's generic, so the inner
		// function gets no annotation for it.
		"local function outerGeneric<K>(key: K): (other: K) -> ()\n",
		"return function(other): () end\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

func TestNativeFilesLeaveUnhelpfulTypesUnannotated(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		// Roblox types other than the two vectors are datatypes, not tables.
		"local function instanceOf(value): Instance\n",
		"function Counter:next(step: number, other): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
	for _, unwanted := range []string{
		", part: Part,",
		"instanceOf(value:",
	} {
		if strings.Contains(got, unwanted) {
			t.Errorf("top-level parameter must stay unannotated: %q in:\n%s", unwanted, got)
		}
	}
}

// A bare `table` annotation is tagged userdata by Luau's compiler, which makes
// the native entry guard fail on every call.
func TestNativeFilesNeverEmitBareTable(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")
	for _, line := range strings.Split(got, "\n") {
		if bareTableAnnotation.MatchString(line) {
			t.Errorf("bare table annotation: %s", line)
		}
	}
}

// bareTableAnnotation matches `table` as a whole annotation, not an identifier
// that merely starts with it (an interface named `table` becomes `table_2`).
var bareTableAnnotation = regexp.MustCompile(`: table($|[^A-Za-z0-9_])`)

// The LRU program the owner reviewed: recursive generic interface, function
// types, and a nested function whose parameter names an outer generic.
func TestNativeFilesAnnotateRecursiveGenericCache(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/lru.ts")

	for _, want := range []string{
		"type Node<K, V> = { key: K, nextNode: Node<K, V>?, previousNode: Node<K, V>?, value: V }\n",
		"local function createLruCache<K, V>(callback: (key: K) -> V, capacity: number?, keyTransform: ((key: K) -> K)?): (key: K) -> V\n",
		// `return nil` in a `(): ()` function is fine.
		"local function detach(node: Node<K, V>?): ()\n",
		"local function attachHead(node: Node<K, V>): ()\n",
		"local function evictIfNeeded(): ()\n",
		// R1: K belongs to createLruCache, so the parameter is unannotated,
		// but the return type still names V.
		"local function memoized(key): V\n",
		"local function lruMemoize<K, V>(callback: (key: K) -> V, capacity: number?): (key: K) -> V\n",
		"local function lruMemoizeNumeric<V>(callback: (key: number) -> V, capacity: number?, precision: number?): (key: number) -> V\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
	if strings.Contains(got, "read ") {
		t.Errorf("readonly must be dropped in:\n%s", got)
	}
	if strings.Contains(got, "{ [any]: any }") {
		t.Errorf("the recursive interface must use its alias, not a table fallback:\n%s", got)
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
		"local function ownGeneric(key)\n",
		"local function robloxTypes(lookup, part, nested)\n",
		"local function multiReturn()\n",
		"local function tupleCallback(cb)\n",
		"local function overloadedParam(fn)\n",
		"local function makeCounter()\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
	if strings.Contains(got, "type ") {
		t.Errorf("type aliases are only emitted for //!native files:\n%s", got)
	}
}

// Every fixture pair is one program: the non-native copy is the native file
// without its `//!native` line, which is what pins "no directive, no change".
func TestNativeFixturePairsDifferOnlyInDirective(t *testing.T) {
	for _, pair := range [][2]string{
		{"src/native.ts", "src/nonative.ts"},
		{"src/lru.ts", "src/lru_nonative.ts"},
	} {
		native, err := os.ReadFile(filepath.Join("testdata", "functions", pair[0]))
		if err != nil {
			t.Fatal(err)
		}
		plain, err := os.ReadFile(filepath.Join("testdata", "functions", pair[1]))
		if err != nil {
			t.Fatal(err)
		}
		if got, want := string(native), "//!native\n"+string(plain); got != want {
			t.Errorf("%s must be %s plus the directive", pair[0], pair[1])
		}
	}
}
