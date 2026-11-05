//! `transform_macro_types_test.go`

use super::{emit_node, find_node, with_macro_state};
use crate::transform_macro_types::generate_universal_type_node;

#[test]
fn generate_universal_type_node_emits_literal_union_object_and_call_signature() {
	let result = with_macro_state(
		rotor_config::config::FlameworkConfig::default(),
		r#"declare const input: { readonly value: "a" | 2; callback(text: string): boolean; optional?: number };"#,
		|state, file| {
			let declaration = file
				.as_node()
				.iter_children()
				.into_iter()
				.find_map(|child| find_node(child, &|node| rotor_tsrs::ast::is_variable_declaration(node)))
				.expect("variable declaration not found");
			let name = declaration.name().expect("declaration name");
			let target = state.ck().get_type_at_location(name);
			generate_universal_type_node(state, declaration, target)
				.map(|(type_node, prerequisites)| (emit_node(type_node, file), prerequisites.len()))
		},
	);

	let (printed, prerequisites) = result.expect("generateUniversalTypeNode() did not generate a type");
	assert_eq!(prerequisites, 0);
	assert_eq!(
		printed,
		"{\n    readonly value: \"a\" | 2;\n    callback(text: string): boolean;\n    optional?: number;\n}"
	);
}
