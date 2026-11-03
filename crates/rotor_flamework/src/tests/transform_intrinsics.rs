//! `transform_intrinsics_test.go`

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, SourceFile};
use rotor_tsrs::checker::Type;

use super::{emit_node, with_macro_state};
use crate::Error;
use crate::transform_intrinsics::build_tuple_guards_intrinsic;
use crate::transform_macro_types::{GuardBuildResult, MacroRuntime};
use crate::transform_state::TransformState;

/// `firstTypeAliasType`
fn first_type_alias_type(state: &mut TransformState<'_>, file: P<SourceFile>) -> Option<P<Type>> {
	let statement = *file
		.statements()
		.nodes()
		.iter()
		.find(|statement| rotor_tsrs::ast::is_type_alias_declaration(**statement))?;
	Some(state.ck().get_type_from_type_node(statement.type_node()?))
}

#[expect(
	clippy::unnecessary_wraps,
	reason = "a GuardBuilder, whose signature returns a Result"
)]
fn named_guard(state: &mut TransformState<'_>, _: P<Node>, target: P<Type>) -> Result<GuardBuildResult, Error> {
	let name = state
		.ck()
		.type_to_string(target)
		.replace("[]", "_array")
		.replace(' ', "_");
	Ok(GuardBuildResult {
		expression: Some(
			state
				.factory
				.new_identifier(rotor_tsrs::tscore::alloc_str(&format!("guard_{name}"))),
		),
		statements: Vec::new(),
	})
}

/// `namedGuardRuntime`
fn named_guard_runtime() -> MacroRuntime {
	MacroRuntime {
		build_guard: Some(named_guard),
		..MacroRuntime::default()
	}
}

/// The tuple guards of the fixture's first type alias, printed, with the prerequisite count.
fn tuple_guards(source: &str) -> (String, usize) {
	with_macro_state(
		rotor_config::config::FlameworkConfig::default(),
		source,
		|state, file| {
			let target = first_type_alias_type(state, file).expect("type alias");
			let (expression, prerequisites) =
				build_tuple_guards_intrinsic(state, file.as_node(), target, &named_guard_runtime())
					.expect("buildTupleGuardsIntrinsic");
			(emit_node(expression, file), prerequisites.len())
		},
	)
}

#[test]
fn build_tuple_guards_intrinsic_splits_fixed_and_rest_guards() {
	let (printed, prerequisites) = tuple_guards("type Input = [name: string, ...values: number[]];");

	assert_eq!(prerequisites, 0);
	assert_eq!(printed, "[\n    [\n        guard_string\n    ],\n    guard_number\n]");
}

#[test]
fn build_tuple_guards_intrinsic_emits_array_as_only_rest_guard() {
	let (printed, _) = tuple_guards("type Input = number[];");

	assert_eq!(printed, "[\n    [],\n    guard_number\n]");
}
