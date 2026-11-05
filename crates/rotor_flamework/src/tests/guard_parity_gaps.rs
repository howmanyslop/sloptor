//! `guard_parity_gaps_test.go`

use super::{emit_node, guard_outputs, guard_test_type_node, parse_ts, with_guard_state};
use crate::guard::{build_configured_flamework_guard, build_flamework_guard};
use crate::guard_types::guard_is_defined_type;
use crate::{ErrorDetail, ErrorKind};

#[test]
fn guard_is_defined_type_matches_upstream_exact_predicate() {
	let tests = [
		("Empty", true),
		("Property", false),
		("Callable", false),
		("Constructable", false),
		("Indexed", false),
		("Union", false),
	];
	let got = with_guard_state(
		r"
type Empty = {};
type Property = { value: string };
type Callable = { (): void };
type Constructable = { new (): object };
type Indexed = { [key: string]: unknown };
type Union = {} | undefined;
",
		|state, source_file| {
			tests
				.iter()
				.map(|(name, _)| {
					let type_value = state
						.ck()
						.get_type_from_type_node(guard_test_type_node(source_file, name));
					guard_is_defined_type(type_value, &mut state.ck())
				})
				.collect::<Vec<_>>()
		},
	);

	for ((name, want), got) in tests.iter().zip(got) {
		assert_eq!(got, *want, "{name}: guardIsDefinedType()");
	}
}

#[test]
fn build_flamework_guard_groups_complete_roblox_enum_union() {
	let tests = [
		("EnumComplete", r#"t["enum"](Enum["Material"])"#),
		("EnumPartial", r#"t["literal"]("fallback", Enum["Material"]["Air"])"#),
	];
	let results = guard_outputs(
		r#"
interface Enum {}
declare namespace Enum {
	namespace Material {
		interface Air { readonly Name: "Air" }
		interface Grass { readonly Name: "Grass" }
		function GetEnumItems(): readonly (Air | Grass)[];
	}
}
type EnumComplete = Enum.Material.Air | Enum.Material.Grass;
type EnumPartial = Enum.Material.Air | "fallback";
"#,
		&["EnumComplete", "EnumPartial"],
	);

	for ((name, want), got) in tests.iter().zip(results) {
		let got = got.unwrap_or_else(|error| panic!("{name}: buildFlameworkGuard() error = {error}"));
		assert_eq!(got, *want, "{name}");
		let reparsed = parse_ts("/guard-enum-reparse.ts", &format!("const guard = {got};"));
		assert!(reparsed.diagnostics().is_empty(), "{name}: reparse diagnostics");
	}
}

#[test]
fn build_flamework_guard_resolves_nominal_type_at_declaration() {
	let results = guard_outputs(
		r"
interface Instance { readonly _nominal_Instance: unique symbol }
interface Part extends Instance { readonly _nominal_Part: unique symbol }
type NominalAlias = Part & { child: string };
",
		&["NominalAlias"],
	);

	assert_eq!(
		results[0].as_deref().ok(),
		Some(r#"t["intersection"](t["instanceIsA"]("Part"), t["children"]({ "child": t["string"] }))"#),
		"{:?}",
		results[0]
	);
}

#[test]
fn build_configured_flamework_guard_preserves_dedup_traversal_order() {
	// Declaration order deliberately differs from first traversal order.
	let statements = with_guard_state(
		r"
interface DeclaredFirst { declared: boolean }
interface TraversedFirst { traversed: string }
interface Target {
	a: TraversedFirst;
	b: TraversedFirst;
	c: DeclaredFirst;
	d: DeclaredFirst;
}
type DedupOrder = Target;
",
		|state, source_file| {
			state.project.config.optimizations.guard_generation_dedup_limit = Some(2);
			let type_node = guard_test_type_node(source_file, "DedupOrder");
			let type_value = state.ck().get_type_from_type_node(type_node);
			build_configured_flamework_guard(state, type_value, type_node)
				.expect("buildConfiguredFlameworkGuard")
				.statements
				.iter()
				.map(|statement| emit_node(*statement, source_file))
				.collect::<Vec<_>>()
		},
	);

	assert_eq!(
		statements,
		[
			r#"const dedup = t["interface"]({ "traversed": t["string"] });"#,
			r#"const dedup_1 = t["interface"]({ "declared": t["boolean"] });"#,
		]
	);
}

#[test]
fn build_flamework_guard_reports_primary_and_related_ast_locations() {
	let (error, file_name, semantic_start, end) = with_guard_state(
		"\ninterface Nested { broken: `unsupported-${string}` }\ntype DiagnosticTarget = { nested: Nested };\n",
		|state, source_file| {
			let type_node = guard_test_type_node(source_file, "DiagnosticTarget");
			let type_value = state.ck().get_type_from_type_node(type_node);
			let error = build_flamework_guard(state, type_value, type_node).expect_err("buildFlameworkGuard");
			let semantic_start = rotor_tsrs::scanner::get_token_pos_of_node(type_node, source_file, false);
			(
				error,
				source_file.file_name().to_owned(),
				i32::try_from(semantic_start).expect("start"),
				crate::transform_diagnostics::text_pos(type_node.end()),
			)
		},
	);

	assert!(
		error.is(ErrorKind::UnsupportedGuardType),
		"error = {error}, want ErrUnsupportedGuardType"
	);
	let Some(ErrorDetail::Guard(generation)) = error.detail() else {
		panic!("error = {error:?}, want GuardGenerationError");
	};
	assert_eq!(
		(generation.file_name.as_str(), generation.start, generation.end),
		(file_name.as_str(), semantic_start, end),
		"primary location"
	);
	assert!(
		generation.related_information.len() >= 2,
		"related information = {:?}, want at least type and property locations",
		generation.related_information
	);
	for related in &generation.related_information {
		assert!(
			!related.file_name.is_empty() && related.start < related.end && !related.type_name.is_empty(),
			"invalid related information = {related:?}"
		);
	}
}

#[test]
fn build_flamework_guard_rejects_recursive_cycle_with_typed_location() {
	let error = with_guard_state(
		"\ninterface Recursive { next: Recursive }\ntype RecursiveTarget = Recursive;\n",
		|state, source_file| {
			let type_node = guard_test_type_node(source_file, "RecursiveTarget");
			state.project.config.optimizations.guard_generation_dedup_limit = Some(2);
			let type_value = state.ck().get_type_from_type_node(type_node);
			build_configured_flamework_guard(state, type_value, type_node).err()
		},
	);

	let located = error.as_ref().is_some_and(
		|error| matches!(error.detail(), Some(ErrorDetail::Guard(generation)) if !generation.file_name.is_empty()),
	);
	assert!(located, "error = {error:?}, want located GuardGenerationError");
}
