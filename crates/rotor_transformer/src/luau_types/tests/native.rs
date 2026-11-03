//! `nativetypes_test.go`: the `//!native` annotations through whole-file output over
//! `testdata/functions`. Every fixture compiles functions, so these wait for slice ii.

use crate::sourcefile::transform_source_file;
use crate::tests::{render_list, transformer_testdata, with_state};

/// `renderSourceFileWithNativeTypes`: a fixture rendered with the option on or off.
fn render_with_native_types(rel_path: &str, native_types: bool) -> String {
	with_state(&transformer_testdata("functions"), rel_path, |state| {
		state.native_types = native_types;
		let statements = transform_source_file(state);
		let diagnostics = state.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		render_list(state, statements)
	})
}

/// `renderNativeSourceFile`
fn render_native(rel_path: &str) -> String {
	render_with_native_types(rel_path, true)
}

/// Every `want` appears in `got`.
fn require_contains(got: &str, want: &[&str]) {
	let missing: Vec<String> = want
		.iter()
		.filter(|want| !got.contains(*want))
		.map(|want| format!("missing {want:?}"))
		.collect();
	assert!(missing.is_empty(), "{}\nin:\n{got}", missing.join("\n"));
}

/// With nativeTypes off, `//!native` is inert: the directive comment is still emitted but no
/// annotation, alias, or generic reaches the output.
#[test]
fn native_types_off_emits_directive_only() {
	let got = render_with_native_types("src/native.ts", false);
	assert!(
		got.contains("--!native\n"),
		"the directive comment must still be emitted:\n{got}"
	);
	for unwanted in [": number", ": vector", "type ", "local function addVector3(a: Vector3"] {
		assert!(
			!got.contains(unwanted),
			"nativeTypes is off but output contains {unwanted:?}:\n{got}"
		);
	}
	let plain = render_with_native_types("src/nonative.ts", false);
	let stripped: Vec<&str> = got.split('\n').filter(|&line| line != "--!native").collect();
	assert_eq!(
		stripped.join("\n"),
		plain,
		"directive-only output differs from the non-native twin beyond the directive line"
	);
}

/// With nativeTypes on, the same file is annotated exactly as before the option existed.
#[test]
fn native_types_on_annotates_directive_files() {
	let got = render_with_native_types("src/native.ts", true);
	assert!(
		got.contains("local function addVector(a: vector, b: vector): vector\n"),
		"nativeTypes is on but the file is not annotated:\n{got}"
	);
	// The opt-in gates the directive, not the project.
	let plain = render_with_native_types("src/nonative.ts", true);
	assert!(
		!plain.contains(": number"),
		"a file without //!native must stay unannotated:\n{plain}"
	);
}

#[test]
fn native_files_annotate_signatures() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			"--!native\n",
			"local function addVector(a: vector, b: vector): vector\n",
			"local function maybeAddVector(a: vector, b: vector?): vector\n",
			"local function addVector3(a: Vector3, b: Vector3): Vector3\n",
			// Both names carry the same vector tag, so the union stays annotated.
			"local function lengthOf(value: Vector3 | vector): number\n",
			"local function scale(x: number, factor: number?, label: string?): number\n",
			"local function nameOf(value: string?): string?\n",
			// A constrained generic uses its constraint, which is the perf type.
			"local function sumGeneric<T>(value: number): number\n",
			"local arrow = function(n: number): number\n",
			"function Counter:constructor(start: number)\n",
			"function Counter:next(step: number, other): number\n",
		],
	);
}

#[test]
fn native_files_annotate_tables_structurally() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
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
		],
	);
}

#[test]
fn native_files_alias_named_types() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
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
		],
	);
	assert_eq!(
		got.matches("type Foo = ").count(),
		1,
		"Foo should be aliased exactly once:\n{got}"
	);
	assert!(!got.contains("read "), "readonly must be dropped in:\n{got}");
}

#[test]
fn native_files_annotate_function_types() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			"local function callback(fn: (n: number) -> number): number\n",
			// Nested function-typed return.
			"local function functionReturn(make: (n: number) -> (s: string) -> boolean): (n: number) -> (s: string) -> boolean\n",
			// A non-generic function type alias keeps its name; a generic one is
			// expanded inline.
			"type Notify = (value: number) -> ()\n",
			"local function notify(cb: Notify, generic: (value: string) -> ()): ()\n",
			// void returns `()`.
			"local function handlers(handler: Handler?): ()\n",
		],
	);
	// A method's `run()` is reached through `thing:run()`.
	assert!(
		got.contains("thing:run()"),
		"fixture must call a method through the alias:\n{got}"
	);
}

#[test]
fn native_files_annotate_callable_shapes() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
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
		],
	);
	// A return pack is not a type: aliasing a LuaTuple would emit `type Pair = (number,
	// string)`, which does not parse.
	assert!(
		!got.contains("type Pair ="),
		"a LuaTuple must never become an alias body:\n{got}"
	);
}

#[test]
fn native_files_annotate_roblox_types_nested() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			"local function robloxTypes(lookup: { [string]: Vector3 }, part, nested: { part: Part, flag: boolean }): { [string]: Vector3 }\n",
			"local function instanceOf(value): Instance\n",
		],
	);
}

#[test]
fn native_files_treat_class_type_parameters_as_any() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			"function Box:constructor(value)\n",
			"function Box:get()\n",
			"function Box:boxed(): { inner: any }\n",
			"local function boxed(box): number\n",
		],
	);
}

#[test]
fn native_files_only_annotate_own_generics_at_top_level() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			"local function ownGeneric<K>(key: K): K\n",
			// `other: K` names the enclosing function's generic, so the inner
			// function gets no annotation for it.
			"local function outerGeneric<K>(key: K): (other: K) -> ()\n",
			"return function(other): () end\n",
		],
	);
}

#[test]
fn native_files_leave_unhelpful_types_unannotated() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			// Roblox types other than the two vectors are datatypes, not tables.
			"local function instanceOf(value): Instance\n",
			"function Counter:next(step: number, other): number\n",
		],
	);
	for unwanted in [", part: Part,", "instanceOf(value:"] {
		assert!(
			!got.contains(unwanted),
			"top-level parameter must stay unannotated: {unwanted:?} in:\n{got}"
		);
	}
}

/// A bare `table` annotation is tagged userdata by Luau's compiler.
#[test]
fn native_files_never_emit_bare_table() {
	let got = render_native("src/native.ts");
	for line in got.split('\n') {
		let bare = line.match_indices(": table").any(|(index, _)| {
			line[index + ": table".len()..]
				.chars()
				.next()
				.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
		});
		assert!(!bare, "bare table annotation: {line}");
	}
}

/// A top-level interface or type alias of the compiled file emits its Luau alias at the
/// declaration, not at the top of the file.
#[test]
fn native_files_emit_aliases_at_the_declaration() {
	let got = render_native("src/native.ts");
	let position = |text: &str| got.find(text);

	// The owner's merged `Hi`: one alias with both declarations' members.
	let alias = "type Hi = { hi: string, there: \"there\" }\n";
	assert_eq!(
		got.matches(alias).count(),
		1,
		"a merged interface must emit exactly one alias:\n{got}"
	);
	// The doc comment belongs to the first declaration, so it stays attached.
	let leading = format!("-- Where the hit landed, merged across two declarations.\n{alias}");
	assert!(
		got.contains(&leading),
		"the alias must follow its declaration's leading comment:\n{got}"
	);
	let alias_index = position(alias).expect("the alias");
	assert!(
		position("local function nameOf").is_some_and(|earlier| alias_index >= earlier),
		"the alias must not be hoisted above an unrelated earlier statement:\n{got}"
	);
	assert!(
		position("local function getHi(): Hi").is_some_and(|use_index| alias_index <= use_index),
		"the alias must precede the function that uses it:\n{got}"
	);

	// Declared after the use: the alias is still at the declaration.
	let later_alias = position("type Later = { value: number }");
	let later_use = position("local function usesLater(): Later");
	assert!(
		matches!((later_alias, later_use), (Some(alias), Some(use_index)) if alias >= use_index),
		"an alias declared after its use belongs at the declaration:\n{got}"
	);

	// Declared before its only use: the alias sits between the surrounding statements.
	let foo_alias = position("type Foo = { a: number, b: string? }");
	let before = position("local function collectionTypes");
	let after = position("local function structs");
	assert!(
		matches!((foo_alias, before, after), (Some(foo), Some(before), Some(after)) if foo >= before && foo <= after),
		"an alias belongs at its declaration, not the header:\n{got}"
	);

	// Fallbacks stay hoisted to the header: a namespace-scoped interface...
	let first_statement = position("local function addVector");
	let hoisted = position("type Node_2 = { text: string }");
	assert!(
		matches!((hoisted, first_statement), (Some(hoisted), Some(first)) if hoisted <= first),
		"a namespace-scoped interface must stay hoisted:\n{got}"
	);
	// ...and one declared inside a function body.
	let local = position("type Local = { value: number }");
	let local_use = position("local function localInterface(): number");
	assert!(
		matches!((local, first_statement, local_use), (Some(local), Some(first), Some(use_index)) if local <= first && local <= use_index),
		"a function-scoped interface must stay hoisted:\n{got}"
	);
}

#[test]
fn native_files_mirror_literal_types() {
	let got = render_native("src/native.ts");
	require_contains(
		&got,
		&[
			// String literals, their optional form, boolean literals, `true |
			// false` collapsing to `boolean?`, quoted keys, an escaped literal,
			// literals in a function type, and `number` staying `number`.
			r#"type Literals = { a: "x" | "y", b: ("x" | "y")?, c: true, d: false, e: boolean, f: boolean?, g: number, lit: "solo", count: number, escaped: "a\"b\\c", format: (value: "x" | "y") -> ("x" | "y"), template: string }"#,
			// An enum member keeps the primitive spelling, string or number.
			"type EnumHolder = { direction: string, code: number }\n",
			// A string literal parameter is tagged `string` by Luau's compiler,
			// so the annotation is free; a boolean literal stays unannotated at a
			// top-level parameter.
			r#"local function literals(value: Literals, holder: EnumHolder, a: "x" | "y", b: ("x" | "y")?, c): string"#,
			r#"local function literalReturn(): "yes""#,
		],
	);
}

#[test]
#[ignore = "needs slice vi: export assignments"]
fn native_files_annotate_recursive_generic_cache() {
	let got = render_native("src/lru.ts");
	require_contains(
		&got,
		&[
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
		],
	);
	assert!(!got.contains("read "), "readonly must be dropped in:\n{got}");
	assert!(
		!got.contains("{ [any]: any }"),
		"the recursive interface must use its alias, not a table fallback:\n{got}"
	);
}

#[test]
fn files_without_native_directive_are_not_annotated() {
	let got = render_native("src/nonative.ts");
	require_contains(
		&got,
		&[
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
			// The literal and declaration fixtures stay plain too: no singleton
			// types and no alias, in place or hoisted.
			"local function getHi()\n",
			"local function usesLater()\n",
			"local function literals(value, holder, a, b, c)\n",
			"local function literalReturn()\n",
		],
	);
	assert!(
		!got.contains("type "),
		"type aliases are only emitted for //!native files:\n{got}"
	);
	assert!(
		!got.contains(r#": "solo""#) && !got.contains(") -> ("),
		"literal types are only mirrored for //!native files:\n{got}"
	);
}

/// Every fixture pair is one program: the non-native copy is the native file without its
/// `//!native` line.
#[test]
fn native_fixture_pairs_differ_only_in_directive() {
	for (native, plain) in [
		("src/native.ts", "src/nonative.ts"),
		("src/lru.ts", "src/lru_nonative.ts"),
	] {
		let directory = transformer_testdata("functions");
		let native_text = std::fs::read_to_string(directory.join(native)).expect("the native fixture reads");
		let plain_text = std::fs::read_to_string(directory.join(plain)).expect("the plain fixture reads");
		assert_eq!(
			native_text,
			format!("//!native\n{plain_text}"),
			"{native} must be {plain} plus the directive"
		);
	}
}

/// The `//!native` signature lines of top-level functions, assembled straight from this slice's
/// hooks in source order (the aliases a declaration places are registered where Go's
/// dispatcher reaches them), against the lines the whole-file tests pin.
#[test]
fn native_signature_hooks() {
	let got = with_state(&transformer_testdata("functions"), "src/native.ts", |state| {
		state.native_types = true;
		state.is_native = super::super::has_native_directive(state);
		assert!(state.is_native, "native.ts carries the directive");
		let mut lines = Vec::new();
		let statements = state.source_file.statements().nodes();
		for &statement in statements {
			visit_native(state, statement, &mut lines);
		}
		let diagnostics = state.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		let mut builder = super::super::luautypes::types(state);
		for entry in builder.aliases() {
			if let Some(statement) = builder.b().entry(entry).statement {
				lines.push(crate::tests::render_node(builder.s, statement).trim().to_owned());
			}
		}
		lines.join("\n") + "\n"
	});
	require_contains(
		&got,
		&[
			"type Foo = { a: number, b: string? }\n",
			"type Balls = { x: number, y: Vector3 }\n",
			"type LinkedNode = { value: number, next: LinkedNode? }\n",
			"type WithMethod = { value: number, run: (self: any) -> number }\n",
			"type Quirky = { [\"kebab-case\"]: number, [\"1\"]: string }\n",
			"type Handler = { onEvent: (value: number) -> (), optional: ((value: string) -> number)? }\n",
			"type Node = { value: number }\n",
			"type Node_2 = { text: string }\n",
			"type table_2 = { value: number }\n",
			"type Notify = (value: number) -> ()\n",
			"type TupleHolder = { pair: () -> (number, string), empty: () -> () }\n",
			"type PairHolder = { pair: any }\n",
			"type FooFactory = { new: (x: number) -> Foo }\n",
			"type CtorHolder = { create: { new: (x: number) -> Foo } }\n",
			"type OverloadedMethod = { run: ((self: any, value: number) -> string) & ((self: any, value: string) -> number) }\n",
			"type IdentityHolder = { identify: <T>(value: T) -> T, replace: <T, U>(value: T, next: U) -> U }\n",
			"type EnumHolder = { direction: string, code: number }\n",
			"local function addVector(a: vector, b: vector): vector\n",
			"local function maybeAddVector(a: vector, b: vector?): vector\n",
			"local function addVector3(a: Vector3, b: Vector3): Vector3\n",
			"local function lengthOf(value: Vector3 | vector): number\n",
			"local function scale(x: number, factor: number?, label: string?): number\n",
			"local function nameOf(value: string?): string?\n",
			"local function sumGeneric<T>(value: number): number\n",
			"local function sum(values: { number }, lookup: { [string]: number }, options: { size: number }): number\n",
			"local function arrayTypes(values: { number }, readonlyValues: { string }, pair: { string | number }): { number }\n",
			"local function collectionTypes(lookup: { [string]: number }, keys: { [string]: true }, counts: { [string]: number }): number\n",
			"local function nested(index: { [string]: { number } }): { [string]: { number } }\n",
			"local function mixed(value, flag, anything): { any }\n",
			"local function quirky(value: Quirky): Quirky\n",
			"local function recordTypes(numbers: { [number]: string }): { [number]: string }\n",
			"local function bothIndex(index: Both): Both\n",
			"local function unionTables(value: { string } | { number }): { string } | { number }\n",
			"local function unionOptional(value: ({ string } | { number })?): number\n",
			"local function collision(a: Node, b: Node_2): number\n",
			"local function reserved(value: table_2): number\n",
			"local function structs(foo: Foo, balls: Balls, maybe: Foo?): Foo\n",
			"local function linked(node: LinkedNode, thing: WithMethod): number\n",
			"local function callback(fn: (n: number) -> number): number\n",
			"local function functionReturn(make: (n: number) -> (s: string) -> boolean): (n: number) -> (s: string) -> boolean\n",
			"local function notify(cb: Notify, generic: (value: string) -> ()): ()\n",
			"local function handlers(handler: Handler?): ()\n",
			"local function multiReturn(): (number, string)\n",
			"local function optionalReturn(): (number, string?)\n",
			"local function restReturn(): (number, ...string)\n",
			"local function emptyReturn(): ()\n",
			"local function maybeReturn()\n",
			"local function looseTupleCallback(cb: () -> ...any): number\n",
			"local function tupleCallback(cb: () -> (number, string)): number\n",
			"local function maybeTupleCallback(cb: (() -> (number, string))?): ()\n",
			"local function pairAlias(): (number, string)\n",
			"local function factory(f: FooFactory): Foo\n",
			"local function makeCounter(): { new: (start: number) -> any, make: (start: number) -> any, prototype: any }\n",
			"local function overloadedParam(fn: (...any) -> ...any): number\n",
			"local function overloadedMethod(thing: OverloadedMethod): number\n",
			"local function genericCallback(identify: <T>(value: T) -> T): number\n",
			"local function robloxTypes(lookup: { [string]: Vector3 }, part, nested: { part: Part, flag: boolean }): { [string]: Vector3 }\n",
			"local function instanceOf(value): Instance\n",
			"local function boxed(box): number\n",
			"local function ownGeneric<K>(key: K): K\n",
			"local function outerGeneric<K>(key: K): (other: K) -> ()\n",
		],
	);
}

/// One statement of the walk in [`native_signature_hooks`].
fn visit_native(
	state: &mut crate::State<'_>,
	statement: rotor_tsrs::P<rotor_tsrs::ast::Node>,
	lines: &mut Vec<String>,
) {
	use rotor_tsrs::ast::{self as tsast, Kind};

	use super::super::{
		get_native_parameter_type, native_declaration_alias, pop_type_generics, push_type_generics, signature_generics,
		signature_return_type,
	};
	match statement.kind() {
		Kind::InterfaceDeclaration | Kind::TypeAliasDeclaration => {
			native_declaration_alias(state, statement);
		}
		Kind::ModuleDeclaration => {
			if let Some(body) = statement.body() {
				for &inner in body.statements() {
					visit_native(state, inner, lines);
				}
			}
		}
		Kind::FunctionDeclaration => {
			let generics = signature_generics(state, statement);
			let names: Vec<String> = generics.iter().map(|generic| generic.name.clone()).collect();
			push_type_generics(state, names.clone(), false);
			let mut parameters = Vec::new();
			for &parameter in statement.parameters() {
				let declaration = parameter.as_parameter_declaration();
				if declaration.dot_dot_dot_token().is_some() {
					parameters.push("...".to_owned());
					continue;
				}
				let name = declaration.name;
				let annotation = if tsast::is_identifier(name) {
					get_native_parameter_type(state, parameter)
				} else {
					None
				};
				parameters.push(annotation.map_or_else(
					|| name.text().to_owned(),
					|t| format!("{}: {}", name.text(), crate::tests::render_node(state, t)),
				));
			}
			if let Some(body) = statement.body() {
				for &inner in body.statements() {
					visit_native(state, inner, lines);
				}
			}
			let ret =
				signature_return_type(state, statement).map(|t| format!(": {}", crate::tests::render_node(state, t)));
			pop_type_generics(state);
			let generics = if names.is_empty() {
				String::new()
			} else {
				format!("<{}>", names.join(", "))
			};
			let name = statement.name().map_or("", |name| name.text());
			lines.push(format!(
				"local function {name}{generics}({}){}",
				parameters.join(", "),
				ret.unwrap_or_default()
			));
		}
		_ => {}
	}
}
