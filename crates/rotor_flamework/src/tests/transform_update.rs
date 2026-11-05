//! `transform_update_test.go`

use rotor_tsrs::ast::Kind;

use super::{emit_node, find_expression_kind, parse_ts, print_source_file, with_expression_fixture};
use crate::ErrorKind;
use crate::transform_expressions::{transform_flamework_expression, transform_flamework_expressions_in_source_file};

/// The expression of `kind` in the fixture, transformed and printed.
fn transform_expression_of_kind(source: &str, kind: Kind) -> String {
	with_expression_fixture(source, |state, source_file| {
		let expression = find_expression_kind(source_file.as_node(), kind);
		let transformed = transform_flamework_expression(state, expression).expect("transformFlameworkExpression");
		emit_node(transformed, source_file)
	})
}

#[test]
fn transform_flamework_expression_rewrites_compound_attribute_assignment_when_metadata_is_inherited() {
	let printed = transform_expression_of_kind(
		r#"
import { BaseComponent as RenamedComponent } from "./base";
interface Attributes { count: number }
declare function next(): number;
class Counter extends RenamedComponent<Attributes> {
	update() { this.attributes.count += next(); }
}
"#,
		Kind::BinaryExpression,
	);

	let want = r#"this[SYMBOL_ATTRIBUTE_SETTER]("count", this.attributes.count + next())"#;
	assert_eq!(printed.strip_suffix(';').unwrap_or(&printed), want);
}

#[test]
fn transform_flamework_expressions_in_source_file_preserves_attribute_update_forms_when_nested_in_methods() {
	let printed = with_expression_fixture(
		r#"
import { BaseComponent } from "./base";
interface Attributes { count: number; [key: string]: number }
declare function next(): number;
class Counter extends BaseComponent<Attributes> {
	set(key: string) { this.attributes[key] = next(); }
	prefix() { return ++this.attributes.count; }
	postfix() { return this.attributes.count++; }
	remove() { delete this.attributes.count; }
}
"#,
		|state, source_file| {
			let transformed = transform_flamework_expressions_in_source_file(state, source_file)
				.expect("transformFlameworkExpressionsInSourceFile");
			print_source_file(transformed)
		},
	);

	for want in [
		r#"import { SYMBOL_ATTRIBUTE_SETTER } from "@flamework/components/out/baseComponent";"#,
		r"this[SYMBOL_ATTRIBUTE_SETTER](key, next());",
		r#"this[SYMBOL_ATTRIBUTE_SETTER]("count", this.attributes.count + 1)"#,
		r#"this[SYMBOL_ATTRIBUTE_SETTER]("count", this.attributes.count + 1, true)"#,
		r#"this[SYMBOL_ATTRIBUTE_SETTER]("count", undefined);"#,
	] {
		assert!(
			printed.contains(want),
			"transformed source missing {want:?}:\n{printed}"
		);
	}
	let reparsed = parse_ts("/reparsed.ts", &printed);
	assert!(reparsed.diagnostics().is_empty(), "reparse diagnostics: {printed}");
}

#[test]
fn transform_flamework_expression_rewrites_known_networking_key_when_obfuscation_marker_is_present() {
	let printed = transform_expression_of_kind(
		r#"
declare const events: {
	readonly _flamework_key_obfuscation: "remotes";
	readonly submitted: () => void;
};
events.submitted;
"#,
		Kind::PropertyAccessExpression,
	);

	let want = r#"events["submitted" as "submitted"]"#;
	assert_eq!(printed.strip_suffix(';').unwrap_or(&printed), want);
}

#[test]
fn transform_flamework_expression_persists_networking_key_hash_when_obfuscation_is_enabled() {
	let (printed, hash) = with_expression_fixture(
		r#"
declare const events: {
	readonly _flamework_key_obfuscation: "remotes";
	readonly submitted: () => void;
};
events.submitted;
"#,
		|state, source_file| {
			state.project.config.obfuscation = true;
			let expression = find_expression_kind(source_file.as_node(), Kind::PropertyAccessExpression);
			let transformed = transform_flamework_expression(state, expression).expect("transformFlameworkExpression");
			let hash = state
				.project
				.build_info_snapshot()
				.string_hashes
				.and_then(|hashes| hashes.get("remotes:submitted").cloned())
				.unwrap_or_default();
			(emit_node(transformed, source_file), hash)
		},
	);

	assert!(!hash.is_empty(), "networking key hash was not persisted");
	let want = format!(r#"events["{hash}" as "submitted"]"#);
	assert_eq!(printed.strip_suffix(';').unwrap_or(&printed), want);
}

#[test]
fn transform_flamework_expression_evaluates_attribute_receiver_once_when_using_direct_assignment() {
	let printed = transform_expression_of_kind(
		r#"
import { BaseComponent } from "./base";
interface Attributes { count: number }
class Counter extends BaseComponent<Attributes> {}
declare function getCounter(): Counter;
declare function next(): number;
getCounter().attributes.count = next();
"#,
		Kind::BinaryExpression,
	);

	let want = r#"getCounter()[SYMBOL_ATTRIBUTE_SETTER]("count", next())"#;
	assert_eq!(printed.strip_suffix(';').unwrap_or(&printed), want);
	assert_eq!(printed.matches("getCounter()").count(), 1, "{printed}");
}

#[test]
fn transform_flamework_expression_rejects_dynamic_networking_key_when_obfuscation_marker_is_present() {
	let error = with_expression_fixture(
		r#"
declare const events: {
	readonly _flamework_key_obfuscation: "remotes";
	readonly [key: string]: unknown;
};
declare const key: string;
events[key];
"#,
		|state, source_file| {
			let expression = find_expression_kind(source_file.as_node(), Kind::ElementAccessExpression);
			transform_flamework_expression(state, expression).err()
		},
	);

	let error = error.expect("transformFlameworkExpression() succeeded");
	assert!(error.is(ErrorKind::DynamicObfuscatedAccess), "{error}");
}
