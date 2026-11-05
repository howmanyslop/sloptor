//! `luautypes_alias_internal_test.go` and `luautypes_alias_test.go`.

use std::collections::HashSet;

use rotor_luau::{TypeAccess, TypeAlias, TypeFunctionParameter, TypeGeneric};

use super::super::luautypes::{AliasPlacement, FidelityLossKind as L, types};
use super::super::luautypes_alias::{
	drop_leading_defaults, is_luau_exported_type, luau_alias_base_name, luau_declaration_statements,
	luau_first_declaration, luau_hoisted_type_statements, type_node_references_name, unique_luau_type_name,
};
use super::super::luautypes_builder::{declared_arguments, recursive_args_match};
use super::{assert_lines_in_order, render_luau_types, render_luau_types_losses, with_fixture};
use crate::prelude::*;
use crate::tests::render_list;

fn spell_generics(fixture: &super::Fixture<'_, '_>, generics: &[TypeGeneric]) -> String {
	generics
		.iter()
		.map(|generic| {
			generic.default.map_or_else(
				|| generic.name.clone(),
				|default| format!("{} = {}", generic.name, fixture.text(default)),
			)
		})
		.collect::<Vec<_>>()
		.join(", ")
}

#[test]
fn luau_types_alias_entry_fields() {
	with_fixture("src/aliases.ts", |fixture| {
		for (name, exported, placement) in [
			("Point", true, AliasPlacement::AtDeclaration),
			("Hidden", false, AliasPlacement::AtDeclaration),
			("Listed", true, AliasPlacement::AtDeclaration),
			("Merged", true, AliasPlacement::AtDeclaration),
			("Handler", true, AliasPlacement::AtDeclaration),
		] {
			let id = fixture.alias(name);
			let entry = fixture.entry(id);
			assert!(
				entry.exported == exported && entry.placement == placement,
				"{name}: Exported={} Placement={:?}, want {exported} {placement:?}",
				entry.exported,
				entry.placement
			);
			let statement = entry
				.statement
				.and_then(|statement| fixture.state.ast.get::<TypeAlias>(statement));
			assert!(
				entry.type_function.is_none() && statement.is_some_and(|alias| alias.body.is_some()) && !entry.building,
				"{name}: entry must carry a finished statement, got {entry:?}"
			);
			// The export marker is applied when the statement is placed.
			assert!(
				statement.is_some_and(|alias| !alias.export),
				"{name}: Statement.Export must stay unset until the alias is placed"
			);
		}

		// A nested namespace member is hoisted under its flattened name and keeps the
		// namespace's export.
		fixture.function_type("size");
		let names = fixture.alias_names();
		let entries = types(fixture.state).aliases();
		let size = entries
			.into_iter()
			.map(|id| fixture.entry(id))
			.find(|entry| entry.name == "Geometry_Size")
			.unwrap_or_else(|| panic!("Geometry.Size was not registered: {names}"));
		assert!(
			size.exported && size.placement == AliasPlacement::Hoisted,
			"Geometry_Size: Exported={} Placement={:?}, want true hoisted",
			size.exported,
			size.placement
		);

		// Declined symbols: a LuaTuple alias has no entry.
		let tuple = fixture.symbol("Tuple");
		assert!(
			super::super::luautypes::type_alias_for(fixture.state, Some(tuple)).is_none(),
			"a LuaTuple alias must not be registered"
		);
	});
}

#[test]
fn luau_types_alias_generics_defaults() {
	with_fixture("src/aliases.ts", |fixture| {
		for (name, want, indexes, params) in [
			("Boxed", "T = number", vec![0], 1),
			("Pair", "A, B = A", vec![0, 1], 2),
			// `until` is a Luau keyword: left out, and the default that names it widens to any.
			("Labeled", "T = any", vec![1], 2),
			("Chain", "T", vec![0], 1),
			("Point", "", vec![], 0),
		] {
			let id = fixture.alias(name);
			let entry = fixture.entry(id);
			assert_eq!(spell_generics(fixture, &entry.generics), want, "{name} generics");
			assert!(
				entry.generic_indexes == indexes && entry.params.len() == params,
				"{name} GenericIndexes={:?} Params={}, want {indexes:?} {params}",
				entry.generic_indexes,
				entry.params.len()
			);
			let statement_generics = entry
				.statement
				.and_then(|statement| fixture.state.ast.get::<TypeAlias>(statement))
				.map(|alias| alias.generics.clone())
				.unwrap_or_default();
			assert_eq!(
				spell_generics(fixture, &statement_generics),
				want,
				"{name} statement generics"
			);
		}

		// A reference drops the arguments of left-out parameters.
		let labeled = fixture.alias("Labeled");
		let labeled = fixture.entry(labeled);
		let arguments: Vec<P<Type>> = labeled
			.params
			.iter()
			.map(|param| param.expect("a parameter type"))
			.collect();
		let got = declared_arguments(&labeled, &arguments);
		assert!(
			got.len() == 1 && Some(got[0]) == labeled.params[1],
			"declaredArguments kept {} arguments, want the second only",
			got.len()
		);
		let pair = fixture.alias("Pair");
		let pair = fixture.entry(pair);
		let arguments: Vec<P<Type>> = pair
			.params
			.iter()
			.map(|param| param.expect("a parameter type"))
			.collect();
		assert_eq!(
			declared_arguments(&pair, &arguments).len(),
			2,
			"declaredArguments must keep every argument of a fully declared alias"
		);
	});
}

/// Defaults are trailing: a hole drops every default before it.
#[test]
fn luau_types_alias_drop_leading_defaults() {
	let mut ast = Ast::new();
	let generics = vec![
		TypeGeneric::new("A").with_default(ast.type_name("number")),
		TypeGeneric::new("B"),
		TypeGeneric::new("C").with_default(ast.type_name("string")),
		TypeGeneric::new("D"),
		TypeGeneric::new("E").with_default(ast.type_name("boolean")),
	];
	let got = drop_leading_defaults(generics);
	for (index, generic) in got.iter().enumerate() {
		assert_eq!(
			generic.default.is_some(),
			index == 4,
			"generic {}: default present",
			generic.name
		);
	}
	let trailing = drop_leading_defaults(vec![
		TypeGeneric::new("A"),
		TypeGeneric::new("B").with_default(ast.type_name("number")),
	]);
	assert!(trailing[1].default.is_some(), "a trailing default must be kept");
}

#[test]
fn luau_types_alias_type_node_references_name() {
	let mut ast = Ast::new();
	let names: HashSet<String> = HashSet::from(["U".to_owned()]);
	let u = ast.type_name("U");
	let empty = ast.new_list::<element::TypeNode>();
	let pack = ast.type_pack(empty);
	let inner = ast.type_function(Vec::new(), vec![TypeFunctionParameter::new("value", u)], None, pack);
	let optional = ast.optional_type(inner);
	let prop = ast.type_table_prop(TypeAccess::None, "run", optional);
	let fields = ast.list_from::<element::TypeTableField>([prop]);
	let table = ast.type_table(fields);
	assert!(
		type_node_references_name(&ast, table, &names),
		"a parameter typed U inside a table member must be found"
	);
	let t = ast.type_name("T");
	let arguments = ast.list_from::<element::TypeNode>([t]);
	let reference = ast.type_reference("Box", Some(arguments));
	assert!(
		!type_node_references_name(&ast, reference, &names),
		"Box<T> does not reference U"
	);
	let qualified = ast.type_qualified_reference("M", "U", None);
	assert!(
		!type_node_references_name(&ast, qualified, &names),
		"a qualified M.U is not the bare parameter U"
	);
}

#[test]
fn luau_types_alias_recursion() {
	with_fixture("src/aliases.ts", |fixture| {
		let chain_id = fixture.alias("Chain");
		let chain = fixture.entry(chain_id);
		let own: Vec<P<Type>> = chain
			.params
			.iter()
			.map(|param| param.expect("a parameter type"))
			.collect();
		assert!(
			recursive_args_match(&chain, &own),
			"the alias's own parameters must match"
		);
		assert!(!recursive_args_match(&chain, &[]), "other arguments must not match");
		assert_eq!(
			fixture.text(chain.statement.expect("a statement")).trim(),
			"type Chain<T> = { value: T, next: Chain<T>? }"
		);

		let from = fixture.loss_count();
		let weird_id = fixture.alias("Weird");
		let losses = fixture.distinct_losses(from);
		assert!(
			losses.contains(&L::RecursiveAliasArgs),
			"Weird<T[]> inside Weird<T> must record RecursiveAliasArgs, got {losses:?}"
		);
		let weird = fixture.entry(weird_id);
		assert!(
			weird
				.losses
				.first()
				.is_some_and(|loss| loss.kind == L::RecursiveAliasArgs),
			"the loss must be attributed to the Weird entry, got {:?}",
			weird.losses
		);
		let comments = super::super::luautypes_alias::loss_comments(&mut types(fixture.state), weird_id);
		assert_eq!(
			comments,
			vec![
				" luauTypes: Weird<T[]> spelled as any: a recursive alias must be referenced with its own type arguments"
			],
			"loss comments"
		);
		assert!(
			chain.losses.is_empty(),
			"Chain records no declaration-level loss, got {:?}",
			chain.losses
		);
	});
}

#[test]
fn luau_types_alias_unique_names() {
	with_fixture("src/aliases.ts", |fixture| {
		for (base, want) in [
			("Fresh", "Fresh"),
			("Fresh", "Fresh_2"),
			("Fresh", "Fresh_3"),
			// Luau builtins and type-function names.
			("number", "number_2"),
			("keyof", "keyof_2"),
			("setmetatable", "setmetatable_2"),
			("typeof", "typeof_2"),
			// Roblox extern types, resolved through the checker.
			("Part", "Part_2"),
			("Instance", "Instance_2"),
			("Enum", "Enum_2"),
			// Type-parameter names used anywhere in the file.
			("T", "T_2"),
			("A", "A_2"),
			("until", "Alias"),
			("kebab-case", "Alias_2"),
			// Value names are a separate namespace.
			("origin", "origin"),
		] {
			assert_eq!(
				unique_luau_type_name(fixture.state, base),
				want,
				"uniqueLuauTypeName({base:?})"
			);
		}
		// A registered alias holds its name.
		fixture.alias("Point");
		assert_eq!(
			types(fixture.state).unique_name("Point"),
			"Point_2",
			"a registered alias name is taken"
		);
		let part = fixture.symbol("Part");
		assert_eq!(
			luau_alias_base_name(fixture.state, part),
			"Part",
			"the base name is the TS name before uniqueness"
		);
	});
	with_fixture("src/aliases.ts", |fixture| {
		let part = fixture.alias("Part");
		assert_eq!(fixture.entry(part).name, "Part_2", "the registered Part alias");
	});
}

#[test]
fn luau_types_alias_imported_names() {
	with_fixture("src/imports.ts", |fixture| {
		let wrapped = fixture.alias("Wrapped");
		let entry = fixture.entry(wrapped);
		assert!(
			entry.exported && entry.placement == AliasPlacement::AtDeclaration,
			"Wrapped must be exported at its declaration, got {entry:?}"
		);
		assert_eq!(
			fixture.alias_names(),
			"Wrapped,Shape,ShapeKind,Container,Promise,Promise_Status",
			"registration order"
		);
		for id in types(fixture.state).aliases() {
			if id == wrapped {
				continue;
			}
			let entry = fixture.entry(id);
			assert!(
				!entry.exported && entry.placement == AliasPlacement::Hoisted,
				"{}: a mirror is hoisted and never exported, got Exported={} Placement={:?}",
				entry.name,
				entry.exported,
				entry.placement
			);
		}
		let symbol = fixture.symbol("Wrapped");
		assert!(is_luau_exported_type(fixture.state, symbol), "Wrapped is exported");
	});
}

#[test]
fn luau_types_alias_export_detection() {
	with_fixture("src/aliases.ts", |fixture| {
		for (name, want) in [
			("Point", true),
			("Hidden", false),
			("Listed", true),
			("Merged", true),
			("Tuple", true),
		] {
			let symbol = fixture.symbol(name);
			assert_eq!(
				is_luau_exported_type(fixture.state, symbol),
				want,
				"isLuauExportedType({name})"
			);
		}
		let merged = fixture.symbol("Merged");
		assert_eq!(
			luau_first_declaration(fixture.state, merged),
			Some(fixture.declaration("Merged")),
			"the first declaration of a merged interface is the fixture's first block"
		);
	});
}

/// `luauDeclarationStatements` places a top-level alias once, at its first declaration;
/// `luauHoistedTypeStatements` then lists only what is left.
#[test]
fn luau_types_alias_emission() {
	with_fixture("src/aliases.ts", |fixture| {
		fixture.state.is_luau_typed = true;

		fixture.function_type("size"); // registers Geometry_Size (hoisted)
		fixture.alias("Point"); // registered before its declaration is reached

		let declaration = fixture.declaration("Point");
		let point = luau_declaration_statements(fixture.state, declaration);
		assert_eq!(
			render_list(fixture.state, point).trim(),
			"export type Point = { x: number, y: number }"
		);
		let again = luau_declaration_statements(fixture.state, declaration);
		assert!(
			fixture.state.ast.list_is_empty(again),
			"a placed alias is not emitted twice"
		);
		let declaration = fixture.declaration("Hidden");
		let hidden = luau_declaration_statements(fixture.state, declaration);
		assert_eq!(
			render_list(fixture.state, hidden).trim(),
			"type Hidden = { secret: number }"
		);
		let declaration = fixture.declaration("Tuple");
		let tuple = luau_declaration_statements(fixture.state, declaration);
		assert!(
			fixture.state.ast.list_is_empty(tuple),
			"a LuaTuple alias emits nothing, got {:?}",
			render_list(fixture.state, tuple)
		);
		let declaration = fixture.declaration("Weird");
		let weird = luau_declaration_statements(fixture.state, declaration);
		let got = render_list(fixture.state, weird);
		assert!(
			got.starts_with("-- luauTypes: Weird<T[]> spelled as any") && got.contains("\nexport type Weird<T> = "),
			"the loss comment precedes the alias:\n{got}"
		);

		let hoisted = luau_hoisted_type_statements(fixture.state);
		assert_eq!(
			render_list(fixture.state, hoisted).trim(),
			"export type Geometry_Size = { width: number, height: number }"
		);
		let hoisted = luau_hoisted_type_statements(fixture.state);
		assert!(
			fixture.state.ast.list_is_empty(hoisted),
			"hoisted statements are emitted once"
		);

		// A second declaration of a merged symbol emits nothing; the first does.
		let merged: Vec<P<Node>> = fixture
			.state
			.source_file
			.statements()
			.nodes()
			.iter()
			.copied()
			.filter(|statement| statement.name().is_some_and(|name| name.text() == "Merged"))
			.collect();
		assert_eq!(merged.len(), 2, "fixture Merged declarations");
		let first = luau_declaration_statements(fixture.state, merged[0]);
		assert!(
			!fixture.state.ast.list_is_empty(first),
			"the first Merged declaration emits the alias"
		);
		let second = luau_declaration_statements(fixture.state, merged[1]);
		assert!(
			fixture.state.ast.list_is_empty(second),
			"the second Merged declaration emits nothing"
		);

		// Off: nothing.
		fixture.state.is_luau_typed = false;
		let declaration = fixture.declaration("Mode");
		let off = luau_declaration_statements(fixture.state, declaration);
		assert!(fixture.state.ast.list_is_empty(off), "luauTypes off emits nothing");
	});
}

#[test]
fn luau_types_aliases_at_declaration() {
	let (got, losses) = render_luau_types_losses("src/aliases.ts", true);

	// Top-level declarations stay in place and in source order, with `export` for the exported
	// ones, defaults on generic aliases, a dropped keyword-named parameter, recursion by name, a
	// merged interface emitted once, union / literal / callable bodies, a Roblox-name collision
	// suffixed, and a loss comment before the alias whose recursion used other arguments.
	assert_lines_in_order(
		&got,
		&[
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
			r#"export type Mode = "r" | "w""#,
			r#"export type Literal = "x""#,
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
			"\tliteral: \"x\",",
			"\tpart: Part_2,",
			"\tsize: Geometry_Size,",
			"}",
		],
	);

	// The nested namespace member is hoisted after the header, before the file's own leading
	// comment, under its flattened name and still exported.
	let header = got.find("\nexport type Geometry_Size = { width: number, height: number }\n");
	let comment = got.find("-- Aliases: exported and local interfaces");
	let first = got.find("export type Point =");
	assert!(
		matches!((header, comment, first), (Some(header), Some(comment), Some(first)) if header < comment && header < first),
		"Geometry_Size must be hoisted after the header, before the first statement:\n{got}"
	);
	for unwanted in ["type Tuple", "type Geometry =", "type Part =", "type Size ="] {
		assert!(!got.contains(unwanted), "output must not contain {unwanted:?}:\n{got}");
	}
	assert_eq!(
		got.matches("type Merged =").count(),
		1,
		"a merged interface emits one alias:\n{got}"
	);
	assert_eq!(
		got.matches("type Point =").count(),
		1,
		"an alias referenced before and after its declaration is emitted once:\n{got}"
	);
	for want in ["RecursiveAliasArgs", "Unspellable"] {
		assert!(
			losses.iter().any(|loss| loss == want),
			"losses {losses:?} must include {want}"
		);
	}
	for unwanted in ["MappedType", "ConditionalType", "DepthExceeded"] {
		assert!(
			!losses.iter().any(|loss| loss == unwanted),
			"losses {losses:?} must not include {unwanted}"
		);
	}
	analyze_clean("aliases.ts", &got);
}

/// Hands rendered output to the analyzer, when one is installed, and requires it clean.
fn analyze_clean(name: &str, got: &str) {
	let Some(analyzer) = super::analyze::find_luau_analyzer() else {
		return;
	};
	if let Some(findings) = super::analyze::analyze_luau_strict(&analyzer, got, true) {
		assert!(
			findings.is_empty(),
			"{name} does not analyze clean:\n{}\n--- rendered ---\n{got}",
			findings.join("\n")
		);
	}
}

#[test]
fn luau_types_aliases_mirror_imports() {
	let (got, losses) = render_luau_types_losses("src/imports.ts", true);
	assert_lines_in_order(
		&got,
		&[
			"local TS = _G[script]",
			"type Shape = { kind: ShapeKind, area: (self: Shape) -> number }",
			r#"type ShapeKind = "circle" | "square""#,
			"type Container<T> = { item: T }",
			"-- luauTypes: Promise<TResult1>, Promise<TResult2>, Promise<TResult1 | TResult2>, Promise<TResult>, Promise<T | TResult>, Promise<R>, Promise<U> spelled as any: a recursive alias must be referenced with its own type arguments",
			"type Promise<T> = {",
			"\ttap: (self: Promise<T>, tapHandler: (value: T) -> ()) -> Promise<T>,",
			"\tcancel: (self: Promise<T>) -> (),",
			"\tawaitStatus: (self: Promise<T>) -> (Promise_Status, unknown),",
			"}",
			r#"type Promise_Status = "Cancelled" | "Rejected" | "Resolved" | "Started""#,
			"-- Cross-file types: a type-only import, a renamed import of a generic",
			r#"local _shapes = TS.import(script, script.Parent, "lib", "shapes")"#,
			"export type Wrapped = {",
			"\tshape: Shape,",
			"\tkind: ShapeKind,",
			"\tcontainer: Container<Shape>,",
			"\tpending: Promise<number>,",
			"}",
		],
	);
	for unwanted in [
		"export type Shape",
		"export type ShapeKind",
		"export type Container",
		"export type Promise",
		"type Box<",
		"type Box =",
	] {
		assert!(!got.contains(unwanted), "output must not contain {unwanted:?}:\n{got}");
	}
	for name in [
		"type Shape =",
		"type ShapeKind =",
		"type Container<T> =",
		"type Promise<T> =",
		"type Promise_Status =",
	] {
		assert_eq!(
			got.matches(name).count(),
			1,
			"{name:?} must be mirrored exactly once:\n{got}"
		);
	}
	assert!(
		losses.iter().any(|loss| loss == "RecursiveAliasArgs"),
		"mirroring Promise<T> must record RecursiveAliasArgs, got {losses:?}"
	);
	analyze_clean("imports.ts", &got);
}

/// The module a mirror comes from still exports its own aliases at their declarations; a
/// generic interface keeps its generics.
#[test]
fn luau_types_aliases_exported_from_sibling() {
	let (got, _) = render_luau_types_losses("src/lib/shapes.ts", true);
	assert_lines_in_order(
		&got,
		&[
			"export type Shape = { kind: ShapeKind, area: (self: Shape) -> number }",
			r#"export type ShapeKind = "circle" | "square""#,
			"export type Box<T> = { item: T }",
		],
	);
	assert!(
		!got.contains("type Container"),
		"the local import name of another file must not leak into the declaring file:\n{got}"
	);
}

/// With luauTypes off, a file without the `//!native` directive emits no alias at all.
#[test]
fn luau_types_aliases_off_emit_nothing() {
	let got = render_luau_types("src/aliases.ts", false);
	assert!(
		!got.contains("type ") && !got.contains("luauTypes:"),
		"luauTypes off must emit no alias:\n{got}"
	);
}
