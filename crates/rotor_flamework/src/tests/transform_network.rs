//! `transform_network_test.go`

use std::cell::RefCell;
use std::rc::Rc;

use rotor_config::config::FlameworkConfig;

use super::{emit_node, find_node, with_macro_state};
use crate::transform_macro_types::MacroRuntime;
use crate::transform_network::{obfuscate_middleware_object, shuffled_indexes};

fn obfuscated_config() -> FlameworkConfig {
	FlameworkConfig {
		obfuscation: true,
		id_generation_mode: "obfuscated".to_owned(),
		..FlameworkConfig::default()
	}
}

/// `firstObjectLiteral`
fn first_object_literal(
	file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>,
) -> Option<rotor_tsrs::P<rotor_tsrs::ast::Node>> {
	file.as_node()
		.iter_children()
		.into_iter()
		.find_map(|child| find_node(child, &|node| rotor_tsrs::ast::is_object_literal_expression(node)))
}

#[test]
fn shuffled_indexes_uses_injected_randomness_when_obfuscation_enabled() {
	let indexes = with_macro_state(obfuscated_config(), "export {};", |state, _| {
		let draws = Rc::new(RefCell::new(vec![0, 1, 0]));
		let runtime = MacroRuntime {
			random_index: Some(Rc::new(move |_| Ok(draws.borrow_mut().remove(0)))),
			..MacroRuntime::default()
		};
		shuffled_indexes(state, 4, &runtime).expect("shuffledIndexes")
	});

	assert_eq!(indexes, [2, 3, 1, 0]);
}

#[test]
fn obfuscate_middleware_object_preserves_nested_keys_when_obfuscation_disabled() {
	let printed = with_macro_state(
		FlameworkConfig::default(),
		"const value = { submitted: { audit: handler } };",
		|state, file| {
			let object = first_object_literal(file).expect("object literal not found");
			let transformed = obfuscate_middleware_object(state, object, &MacroRuntime::default())
				.expect("obfuscateMiddlewareObject");
			emit_node(transformed, file)
		},
	);

	assert_eq!(
		printed,
		r#"{ ["submitted" as "submitted"]: { ["audit" as "audit"]: handler } }"#
	);
}

#[test]
fn obfuscate_middleware_object_stably_hashes_and_shuffles_when_obfuscation_enabled() {
	let (first, second) = with_macro_state(
		obfuscated_config(),
		"const value = { submitted: handler, accepted: handler };",
		|state, file| {
			let object = first_object_literal(file).expect("object literal not found");
			let runtime = MacroRuntime {
				random_index: Some(Rc::new(|_| Ok(0))),
				..MacroRuntime::default()
			};
			let first = obfuscate_middleware_object(state, object, &runtime).expect("first obfuscateMiddlewareObject");
			let second =
				obfuscate_middleware_object(state, object, &runtime).expect("second obfuscateMiddlewareObject");
			(emit_node(first, file), emit_node(second, file))
		},
	);

	assert_eq!(first, second, "controlled obfuscation changed between calls");
	assert!(
		!first.contains(r#""submitted" as"#) && !first.contains(r#""accepted" as"#),
		"controlled obfuscation retained a remote key: {first}"
	);
	assert!(
		first.ends_with(": handler }"),
		"controlled obfuscation output = {first}"
	);
}
