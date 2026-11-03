//! `luautypes_builder_test.go`

use rotor_tsrs::ast::SymbolFlags;

use super::super::luautypes::{AliasPlacement, FidelityLossKind as L, SymKey, TypeContext, TypePosition, types};
use super::super::nativetypes::NativeTypeKind;
use super::super::{pop_type_generics, push_type_generics};
use super::{case, run_type_cases, same_loss_set, with_fixture};

#[test]
fn luau_types_builder_primitives() {
	with_fixture("src/primitives.ts", |fixture| {
		run_type_cases(
			fixture,
			&[
				case("aNumber", "number", &[]),
				case("aString", "string", &[]),
				case("aBoolean", "boolean", &[]),
				case("anAny", "any", &[]),
				case("anUnknown", "unknown", &[]),
				case("aNever", "never", &[]),
				case("anUndefined", "nil", &[]),
				case("aNull", "nil", &[]),
				case("aVoid", "nil", &[]),
				case("aBigInt", "number", &[L::BigInt]),
				case("aSymbol", "any", &[L::SymbolKey]),
				case("anObject", "{ [any]: any }", &[]),
				case("anEmpty", "any", &[]),
				case("stringLiteral", r#""x""#, &[]),
				case("trueLiteral", "true", &[]),
				case("falseLiteral", "false", &[]),
				case("numberLiteral", "number", &[L::NumberLiteral]),
				case("template", "string", &[L::TemplateLiteral]),
				case("escaped", r#""a\"b\\c\n""#, &[]),
				case("aVector", "vector", &[]),
				case("aVector3", "Vector3", &[]),
				case("aCFrame", "CFrame", &[]),
				case("aPart", "Part", &[]),
				case("anInstance", "Instance", &[]),
				case("aMaterial", "Enum.Material", &[]),
				// Luau names the enum, never one of its items.
				case("plastic", "Enum.Material", &[]),
				case("anEnumItem", "EnumItem", &[]),
				case("aSignal", "RBXScriptSignal", &[]),
				case("config", "{ depth: number }", &[]),
			],
			|fixture, name| {
				let node = fixture.variable_type(name).node;
				fixture.text(node)
			},
		);
	});
}

#[test]
fn luau_types_builder_unions() {
	with_fixture("src/unions.ts", |fixture| {
		run_type_cases(
			fixture,
			&[
				case("maybeNumber", "number?", &[]),
				case("nullableString", "string?", &[]),
				case("numberOrString", "string | number", &[]),
				case("optionalUnion", "(string | number)?", &[]),
				case("boolPair", "boolean", &[]),
				case("boolOrUndefined", "boolean?", &[]),
				case("literals", r#""a" | "b""#, &[]),
				case("numberLiterals", "number", &[L::NumberLiteral]),
				case("mixedLiterals", r#""a" | number"#, &[L::NumberLiteral]),
				case("fnOrNil", "((n: number) -> string)?", &[]),
				case("fnOrString", "string | ((n: number) -> string)", &[]),
				case("tableUnion", "{ a: number } | { b: string }", &[]),
				case("arrayUnion", "{ string } | { number }", &[]),
				case("dupes", "{ number }", &[]),
				case("onlyNil", "nil", &[]),
				case("tableIntersection", "{ a: number } & { b: string }", &[]),
				case("emptyIntersection", "{ a: number }", &[]),
				case("brandedString", "any", &[L::Unspellable]),
				case("overloads", "((n: number) -> string) & ((s: string) -> number)", &[]),
				case("overloadedAlias", "Overloaded", &[]),
			],
			|fixture, name| {
				let node = fixture.variable_type(name).node;
				fixture.text(node)
			},
		);
		assert_eq!(
			fixture.alias_statement("Overloaded"),
			"type Overloaded = ((value: number) -> string) & ((value: string) -> number)"
		);
	});
}

#[test]
#[expect(
	clippy::redundant_closure_for_method_calls,
	reason = "the method path is not general over the fixture's lifetimes"
)]
fn luau_types_builder_tables() {
	with_fixture("src/tables.ts", |fixture| {
		run_type_cases(
			fixture,
			&[
				case("Point", "type Point = { x: number, y: number }", &[]),
				// readonly -> read, a method takes the alias as self, optional members and
				// optional methods take `?`, non-identifier and keyword keys are quoted, a `this`
				// parameter is spelled as its type, and the polymorphic this type is the alias.
				case(
					"Shape",
					r#"type Shape = { read kind: string, area: (self: Shape) -> number, optional: string?, ["kebab-case"]: number, ["1"]: boolean, ["end"]: string, onEvent: (value: number) -> (), maybeRun: ((self: Shape) -> ())?, explicitThis: (self: Point, n: number) -> number, clone: (self: Shape) -> Shape }"#,
					&[],
				),
				case("Dict", "type Dict = { [string]: number }", &[]),
				case("NumDict", "type NumDict = { [number]: string }", &[]),
				case("Both", "type Both = { [any]: number }", &[L::IndexerCollapsed]),
				case("BothDiffer", "type BothDiffer = { [any]: any }", &[L::IndexerCollapsed]),
				case("ReadonlyDict", "type ReadonlyDict = { read [string]: number }", &[]),
				case("Node", "type Node = { value: number, next: Node? }", &[]),
				case("Tree", "type Tree<T> = { value: T, children: { Tree<T> } }", &[]),
				case("Pair", "type Pair<T> = { first: T, second: T }", &[]),
				case("Mode", r#"type Mode = "r" | "w""#, &[]),
				case("Callback", "type Callback<T> = (value: T) -> ()", &[]),
				case("Empty", "type Empty = {}", &[]),
				case("Direction", r#"type Direction = "up" | "down""#, &[]),
				case("Codes", "type Codes = number", &[L::NumberLiteral]),
			],
			|fixture, name| fixture.alias_statement(name),
		);

		run_type_cases(
			fixture,
			&[
				case("record", "{ [string]: number }", &[]),
				case("literalRecord", "{ a: number, b: number }", &[]),
				case("partial", "{ x: number?, y: number? }", &[]),
				case("readonlyPoint", "{ read x: number, read y: number }", &[]),
				case("numbers", "{ number }", &[]),
				case("readonlyNumbers", "{ read string }", &[]),
				case("readonlyArraySyntax", "{ read boolean }", &[]),
				case("pair", "{ string | number }", &[L::Tuple]),
				case("readonlyPair", "{ read string | number }", &[L::Tuple]),
				case("samePair", "{ number }", &[L::Tuple]),
				case("map", "{ [string]: Point }", &[]),
				case("readonlyMap", "{ read [string]: number }", &[]),
				case("weakMap", "{ [Point]: number }", &[]),
				case("set", "{ [string]: true }", &[]),
				case("readonlySet", "{ read [number]: true }", &[]),
				case("weakSet", "{ [Point]: true }", &[]),
				case("nested", "{ inner: { deep: { value: number } } }", &[]),
				case(
					"inlineMethod",
					"{ run: (self: any) -> number }",
					&[L::ObjectLiteralSelf],
				),
				case("numberPair", "Pair<number>", &[]),
				case("mode", "Mode", &[]),
				case("callback", "Callback<string>", &[]),
				case("empty", "Empty", &[]),
				case("direction", "Direction", &[]),
				case("code", "Codes", &[]),
				case("up", r#""up""#, &[]),
				case("directionObject", r#"{ read Up: "up", read Down: "down" }"#, &[]),
				// The first reference to Foo registers its instance alias, whose body records the
				// dropped readonly.
				case("foo", "Foo", &[L::ClassReadonlyDropped]),
				// The class value is structural; its recursive `self` static makes it a cycle
				// through an anonymous type, which only a synthesized hoisted alias can spell.
				case("fooClass", "Anonymous_1", &[]),
				case("box", "Box<number>", &[]),
			],
			|fixture, name| {
				let node = fixture.variable_type(name).node;
				fixture.text(node)
			},
		);

		let declaration = fixture.declaration("fooClass");
		let t = fixture
			.state
			.ck()
			.get_type_at_location(declaration.name().expect("a name"));
		let anonymous = types(fixture.state)
			.b()
			.anonymous
			.get(&t)
			.copied()
			.expect("fooClass did not register a synthesized alias");
		let entry = fixture.entry(anonymous);
		// The TS `prototype` artifact is left out: the emitted class table has no such key.
		assert_eq!(
			fixture.text(entry.statement.expect("a statement")).trim(),
			"type Anonymous_1 = { new: (x: number) -> Foo, self: Anonymous_1, make: () -> Foo }"
		);
		assert!(
			entry.placement == AliasPlacement::Hoisted && !entry.exported && entry.kind == NativeTypeKind::Table,
			"synthesized alias entry = {entry:?}, want a hoisted, unexported table alias"
		);
	});
}

#[test]
fn luau_types_builder_functions() {
	with_fixture("src/functions.ts", |fixture| {
		run_type_cases(
			fixture,
			&[
				case("plain", "(a: number, b: string) -> boolean", &[]),
				case("optionalParams", "(a: number, b: string?, c: boolean?) -> ()", &[]),
				case("restParams", "(first: number, ...string) -> number", &[]),
				case("genericFn", "<T>(value: T) -> T", &[]),
				case("constrained", "<T>(value: T) -> T", &[L::ConstraintDropped]),
				case("thisParam", "(self: { x: number }, n: number) -> number", &[]),
				case("returnsUndefined", "() -> ()", &[]),
				case("returnsOptional", "() -> number?", &[]),
				case("returnsFn", "() -> (n: number) -> (s: string) -> boolean", &[]),
				case("returnsUnion", "() -> ({ string } | { number })", &[]),
				case("voidCallback", "(cb: () -> ()) -> ()", &[]),
				case("describe", "(value: unknown) -> never", &[]),
				// `until` is a Luau keyword, so the parameter goes unnamed.
				case("keyword", "(number) -> ()", &[]),
				case("luaTuple", "() -> (number, string)", &[]),
				case("optionalTuple", "() -> (number, string?)", &[]),
				case("restTuple", "() -> (number, ...string)", &[]),
				case("emptyTuple", "() -> ()", &[]),
				case("maybeTuple", "() -> ...any", &[L::OptionalPack]),
				case("looseTuple", "() -> ...any", &[L::Unspellable]),
				case("tupleParam", "(pair: any) -> ()", &[L::LuaTupleOutsideReturn]),
				case("tupleCallback", "(cb: () -> (number, string)) -> ()", &[]),
				case("genericCallback", "(identify: <T>(value: T) -> T) -> ()", &[]),
				case("outerGeneric", "<K>(key: K) -> (other: K) -> ()", &[]),
				case("keys", "<T>(value: T) -> keyof<T>", &[]),
				case(
					"indexed",
					"<T, K>(value: T, key: K) -> index<T, K>",
					&[L::ConstraintDropped],
				),
				case(
					"restGeneric",
					"<A>(...any) -> ()",
					&[L::ConstraintDropped, L::Unspellable],
				),
				case("tupleRest", "(...any) -> ()", &[L::Unspellable]),
			],
			|fixture, name| {
				let node = fixture.function_type(name);
				fixture.text(node)
			},
		);

		let node = fixture.variable_type("overloaded").node;
		assert_eq!(fixture.text(node), "((n: number) -> string) & ((s: string) -> number)");

		// A library interface is mirrored as one alias instead of being re-instantiated
		// structurally at every use; references into its own body with other arguments are
		// `any`.
		let from = fixture.loss_count();
		let promised = fixture.function_type("promised");
		assert_eq!(fixture.text(promised), "() -> Promise<number>");
		assert!(
			fixture.losses_since(from).contains(&L::RecursiveAliasArgs),
			"mirroring Promise<T> must record RecursiveAliasArgs, got {:?}",
			fixture.distinct_losses(from)
		);
		let names = fixture.alias_names();
		assert!(names.contains("Promise"), "Promise alias not registered: {names}");
	});
}

/// Rule R1 stays a query: a generic of the innermost function is "own", one of an enclosing
/// function is "outer" at a parameter position only.
#[test]
fn luau_types_builder_outer_generic() {
	with_fixture("src/functions.ts", |fixture| {
		push_type_generics(fixture.state, vec!["K".to_owned()], false);
		let own = fixture.parameter_type("outerGeneric", 0);
		push_type_generics(fixture.state, Vec::new(), false);
		let outer = fixture.parameter_type("outerGeneric", 0);
		let parameter = fixture.declaration("outerGeneric").parameters()[0];
		let t = fixture
			.state
			.ck()
			.get_type_at_location(parameter.name().expect("a name"));
		let nested = types(fixture.state).build(t, TypeContext::at(TypePosition::Nested, None));
		pop_type_generics(fixture.state);
		pop_type_generics(fixture.state);

		for (name, got, want) in [("own", own, false), ("outer", outer, true), ("nested", nested, false)] {
			assert_eq!(fixture.text(got.node), "K", "{name}");
			assert_eq!(got.outer_generic, want, "{name}: OuterGeneric");
			assert_eq!(got.kind, NativeTypeKind::Generic, "{name}: Kind");
		}

		// With no scope the parameter is not declarable at the site.
		let from = fixture.loss_count();
		let out_of_scope = fixture.parameter_type("genericFn", 0);
		assert!(
			fixture.text(out_of_scope.node) == "any" && out_of_scope.lossy,
			"out-of-scope generic spelled {:?} (lossy {}), want any",
			fixture.text(out_of_scope.node),
			out_of_scope.lossy
		);
		let losses = fixture.distinct_losses(from);
		assert!(
			same_loss_set(&losses, &[L::Unspellable]),
			"out-of-scope losses {losses:?}"
		);
	});
}

/// `typeof x` keeps Luau's typeof only for a top-level const or function of this file declared
/// before the use; everything else resolves.
#[test]
fn luau_types_builder_typeof() {
	with_fixture("src/tables.ts", |fixture| {
		run_type_cases(
			fixture,
			&[
				case("configType", "typeof(config)", &[]),
				case("importedType", "number", &[]),
				case("counterType", "number", &[]),
				case("laterType", "{ x: number }", &[]),
				case("foo", "Foo", &[L::ClassReadonlyDropped]),
			],
			|fixture, name| {
				let declaration = fixture.declaration(name);
				let type_node = declaration
					.type_node()
					.unwrap_or_else(|| panic!("{name} has no type annotation"));
				let node = types(fixture.state)
					.build_type_node(type_node, TypeContext::at(TypePosition::Local, Some(declaration)))
					.node;
				fixture.text(node)
			},
		);
	});
}

/// Every reference spelled from a TS symbol is recorded in Origins, and the table built for an
/// interface body carries the interface.
#[test]
fn luau_types_builder_origins() {
	with_fixture("src/tables.ts", |fixture| {
		for (name, flags) in [
			("foo", SymbolFlags::Class),
			("numberPair", SymbolFlags::TypeAlias),
			("direction", SymbolFlags::Enum),
			("map", SymbolFlags::None),
		] {
			let node = fixture.variable_type(name).node;
			let origin = types(fixture.state).b().origins.get(&node).copied();
			if flags == SymbolFlags::None {
				assert!(origin.is_none(), "{name}: a structural table must not have an origin");
				continue;
			}
			assert!(
				origin.is_some_and(|symbol| symbol.flags().intersects(flags)),
				"{name}: origin {origin:?}, want a symbol with flags {flags:?}"
			);
		}

		let point = fixture.alias("Point");
		let statement = fixture.entry(point).statement.expect("a statement");
		let body = fixture
			.state
			.ast
			.get::<rotor_luau::TypeAlias>(statement)
			.and_then(|alias| alias.body)
			.expect("a body");
		let origin = types(fixture.state).b().origins.get(&body).copied();
		assert!(
			matches!(origin, Some(SymKey::Real(symbol)) if symbol.name() == "Point"),
			"interface body origin = {origin:?}, want Point"
		);
	});
	with_fixture("src/primitives.ts", |fixture| {
		let material = fixture.variable_type("aMaterial").node;
		let origin = types(fixture.state).b().origins.get(&material).copied();
		assert!(
			matches!(origin, Some(SymKey::Real(symbol)) if symbol.name() == "Material"),
			"Enum.Material origin = {origin:?}, want the Material alias"
		);
	});
}

/// Kind classifies the built node the way the native policy expects.
#[test]
fn luau_types_builder_kinds() {
	with_fixture("src/tables.ts", |fixture| {
		for (name, want) in [
			("numbers", NativeTypeKind::Table),
			("foo", NativeTypeKind::Table),
			("mode", NativeTypeKind::String),
			("code", NativeTypeKind::Number),
			("callback", NativeTypeKind::Function),
			("direction", NativeTypeKind::String),
			("fooClass", NativeTypeKind::Table),
		] {
			assert_eq!(fixture.variable_type(name).kind, want, "{name}");
		}
	});
	with_fixture("src/primitives.ts", |fixture| {
		for (name, want) in [
			("aNumber", NativeTypeKind::Number),
			("aString", NativeTypeKind::String),
			("aBoolean", NativeTypeKind::Boolean),
			("anUndefined", NativeTypeKind::Nil),
			("aVector3", NativeTypeKind::Vector),
			("aPart", NativeTypeKind::Vector),
			("aMaterial", NativeTypeKind::Vector),
			("anUnknown", NativeTypeKind::Unspellable),
			("stringLiteral", NativeTypeKind::String),
		] {
			assert_eq!(fixture.variable_type(name).kind, want, "{name}");
		}
	});
	with_fixture("src/unions.ts", |fixture| {
		assert_eq!(fixture.variable_type("boolOrUndefined").kind, NativeTypeKind::Boolean);
		assert_eq!(fixture.variable_type("overloads").kind, NativeTypeKind::Function);
	});
}

/// The receiver and the class-instance flag reach a table built directly.
#[test]
fn luau_types_builder_receiver() {
	with_fixture("src/tables.ts", |fixture| {
		let symbol = fixture.symbol("Shape");
		let shape = fixture.state.ck().get_declared_type_of_symbol(symbol);

		let from = fixture.loss_count();
		let receiver = fixture.state.ast.type_name("Custom");
		let this = fixture.state.ast.type_name("Custom");
		let ctx = TypeContext {
			position: TypePosition::Nested,
			receiver: Some(receiver),
			this: Some(this),
			owner: None,
			class_instance: true,
		};
		let table = types(fixture.state)
			.build_table(shape, ctx)
			.expect("BuildTable returned nil");
		// The checker instantiates `this` in a declared interface's members to the interface
		// itself, so `clone(): this` returns the Shape alias.
		assert_eq!(
			fixture.text(table),
			r#"{ kind: string, area: (self: Custom) -> number, optional: string?, ["kebab-case"]: number, ["1"]: boolean, ["end"]: string, onEvent: (value: number) -> (), maybeRun: ((self: Custom) -> ())?, explicitThis: (self: Point, n: number) -> number, clone: (self: Custom) -> Shape }"#
		);
		let losses = fixture.distinct_losses(from);
		assert!(same_loss_set(&losses, &[L::ClassReadonlyDropped]), "losses {losses:?}");

		let declaration = fixture.declaration("inlineMethod");
		let inline = fixture
			.state
			.ck()
			.get_type_at_location(declaration.name().expect("a name"));
		let from = fixture.loss_count();
		let receiver = fixture.state.ast.type_name("Obj");
		let ctx = TypeContext {
			receiver: Some(receiver),
			..TypeContext::default()
		};
		let named = types(fixture.state)
			.build_table(inline, ctx)
			.expect("BuildTable returned nil");
		assert_eq!(fixture.text(named), "{ run: (self: Obj) -> number }");
		let losses = fixture.distinct_losses(from);
		assert!(
			losses.is_empty(),
			"a named receiver must not record losses, got {losses:?}"
		);
	});
}

/// Aliases register once, in first-use order, and keep their statement.
#[test]
fn luau_types_builder_alias_registry() {
	with_fixture("src/tables.ts", |fixture| {
		fixture.variable_type("map"); // registers Point
		fixture.variable_type("numberPair"); // registers Pair
		fixture.variable_type("map");

		assert_eq!(fixture.alias_names(), "Point,Pair");
		let first = fixture.alias("Point");
		let second = fixture.alias("Point");
		let entry = fixture.entry(first);
		let body = entry
			.statement
			.and_then(|statement| fixture.state.ast.get::<rotor_luau::TypeAlias>(statement))
			.and_then(|alias| alias.body);
		assert!(
			first == second && !entry.building && body.is_some(),
			"typeAliasFor must return the same finished entry"
		);
		let pair = fixture.alias("Pair");
		let pair = fixture.entry(pair);
		assert!(
			pair.params.len() == 1
				&& pair.generics.len() == 1
				&& pair.generics[0].name == "T"
				&& pair.kind == NativeTypeKind::Table,
			"Pair entry = {pair:?}"
		);
		let callback = fixture.alias("Callback");
		let callback = fixture.entry(callback);
		assert!(
			callback.callable && callback.kind == NativeTypeKind::Function,
			"Callback entry must be callable, got {callback:?}"
		);
		assert!(
			super::super::luautypes::type_alias_for(fixture.state, None).is_none(),
			"a nil symbol has no alias"
		);
	});
}
