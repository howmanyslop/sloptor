//! `literals_test.go`. Go parses a lone file without a program; the port compiles the same text
//! as a one-file project.

use rotor_luau::{
	Array, InterpolatedString, InterpolatedStringPart, Map, MapField, NumberLiteral, StringLiteral, TemporaryIdentifier,
};
use rotor_tsrs::ast::skip_downwards;

use super::{render_list, render_node, with_source, with_test_state};
use crate::dispatch::transform_expression;
use crate::pointer::{
	assign_to_map_pointer, create_array_pointer, create_map_pointer, disable_array_inline, disable_map_inline,
};

#[test]
fn template_with_table_expression_wraps_in_parens() {
	with_source("`${[1, 2]}`;", |s, expression| {
		let (result, prereqs) = s.capture(|s| transform_expression(s, expression));
		assert!(
			s.ast.list_is_empty(prereqs),
			"template literal must not produce prereqs"
		);
		// The transformer emits the raw Array part; the RENDERER adds the parens.
		let interpolated = s
			.ast
			.get::<InterpolatedString>(result)
			.expect("an InterpolatedString")
			.clone();
		let head = s.ast.list_to_vec(interpolated.parts)[0];
		assert!(
			s.ast.get::<Array>(head).is_some(),
			"part must be a raw Array (renderer parenthesizes)"
		);
		assert_eq!(render_node(s, result), "`{({ 1, 2 })}`");
		let diagnostics = s.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}

#[test]
fn template_parts_preserve_raw_escapes() {
	// Cooked text would turn \n into a newline; the part keeps the raw two-character escape.
	with_source("`a\\n${[1]}\\t{b}`;", |s, expression| {
		let result = s.no_prereqs(|s| transform_expression(s, expression));
		let interpolated = s
			.ast
			.get::<InterpolatedString>(result)
			.expect("an InterpolatedString")
			.clone();
		let parts = s.ast.list_to_vec(interpolated.parts);
		assert_eq!(parts.len(), 3);
		let text = |part| s.ast.get::<InterpolatedStringPart>(part).map(|part| part.text.clone());
		assert_eq!(text(parts[0]).as_deref(), Some("a\\n"), "head keeps the raw escape");
		assert_eq!(
			text(parts[2]).as_deref(),
			Some("\\t{b}"),
			"the renderer escapes the brace"
		);
	});
}

#[test]
fn object_literal_computed_key() {
	with_source("({ x: 1, [\"computed key\"]: 3 });", |s, expression| {
		let object = skip_downwards(expression);
		let (result, prereqs) = s.capture(|s| transform_expression(s, object));
		assert!(
			s.ast.list_is_empty(prereqs),
			"literal-only object must stay inline (no prereqs)"
		);
		let map = s.ast.get::<Map>(result).expect("an inline Map").clone();
		let fields = s.ast.list_to_vec(map.fields);
		assert_eq!(fields.len(), 2);
		let index = |field| {
			let field = s.ast.get::<MapField>(field).expect("a field");
			s.ast.get::<StringLiteral>(field.index).map(|index| index.value.clone())
		};
		assert_eq!(index(fields[0]).as_deref(), Some("x"));
		assert_eq!(index(fields[1]).as_deref(), Some("computed key"));
		assert_eq!(render_node(s, result), "{\n\tx = 1,\n\t[\"computed key\"] = 3,\n}");
	});
}

#[test]
fn numeric_literal_raw_text_passthrough() {
	for (source, want) in [
		("0xff;", "0xff"),
		("0b101;", "0b101"),
		("1_000_000;", "1_000_000"),
		("3.25;", "3.25"),
	] {
		with_source(source, |s, expression| {
			let result = s.no_prereqs(|s| transform_expression(s, expression));
			let number = s.ast.get::<NumberLiteral>(result).expect("a NumberLiteral");
			assert_eq!(number.value, want, "{source}: the raw source text");
		});
	}
}

#[test]
fn string_literal_raw_text_slicing() {
	// Escapes pass through exactly as written: `\"` and `\\` stay two characters.
	with_source(r#""escape \"quotes\" and \\ backslash";"#, |s, expression| {
		let result = s.no_prereqs(|s| transform_expression(s, expression));
		let string = s.ast.get::<StringLiteral>(result).expect("a StringLiteral");
		assert_eq!(string.value, r#"escape \"quotes\" and \\ backslash"#);
	});
}

/// Fields stay on the inline map until a member with prereqs forces `DisableMapInline`, which
/// materializes `local <name> = {...}` and turns later fields into assignments.
#[test]
fn map_pointer_degradation() {
	with_test_state(|s| {
		let statements = s.capture_statements(|s| {
			let mut pointer = create_map_pointer(s, "object");

			// While inline: fields land in the literal, no prereqs.
			let (a, one) = (s.ast.str("a"), s.ast.num(1.0));
			assign_to_map_pointer(s, &pointer, a, one);
			let map = s.ast.get::<Map>(pointer.value).expect("pointer degraded too early");
			assert_eq!(s.ast.list_size(map.fields), 1);

			// An element with prereqs (simulated) forces materialization.
			disable_map_inline(s, &mut pointer);
			let temp = pointer.value;
			let name = s.ast.get::<TemporaryIdentifier>(temp).map(|temp| temp.name.clone());
			assert_eq!(name.as_deref(), Some("object"), "temp name hint");
			// Disabling twice is a no-op (upstream guard).
			disable_map_inline(s, &mut pointer);
			assert_eq!(pointer.value, temp, "second DisableMapInline must not re-push");

			// After materialization: fields become prereq assignments.
			let (b, two) = (s.ast.str("b"), s.ast.num(2.0));
			assign_to_map_pointer(s, &pointer, b, two);
		});
		// Temp ids render with the `_` prefix.
		assert_eq!(
			render_list(s, statements),
			"local _object = {\n\ta = 1,\n}\n_object.b = 2\n"
		);
	});
}

/// The array twin.
#[test]
fn array_pointer_degradation() {
	with_test_state(|s| {
		let statements = s.capture_statements(|s| {
			let mut pointer = create_array_pointer(s, "array");
			let members = s.ast.get::<Array>(pointer.value).expect("an Array").members;
			let one = s.ast.num(1.0);
			s.ast.list_push(members, one);
			disable_array_inline(s, &mut pointer);
			assert!(s.ast.get::<TemporaryIdentifier>(pointer.value).is_some());
		});
		assert_eq!(render_list(s, statements), "local _array = { 1 }\n");
	});
}
