//! `literals.go`: number, string and template literals, array and object literals, property
//! names and parenthesized expressions.

use rotor_luau::{Array, Map};
use rotor_tsrs::ast as tsast;
use rotor_tsrs::scanner;

use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order;
use crate::methodassignment::validate_method_assignment;
use crate::pointer::{
	MapPointer, assign_to_map_pointer, create_array_pointer, create_map_pointer, disable_array_inline,
	disable_map_inline,
};
use crate::prelude::*;
use crate::{control, expressions};

/// `getText` (`ts.Node.getText()`): the raw source from the token start to the node end.
pub fn get_text(state: &State<'_>, node: P<Node>) -> String {
	scanner::get_text_of_node_from_source_text(state.source_file_text, node, false)
}

/// `transformNumericLiteral`: the raw source text passes through (`0xff`, `0b101`,
/// `1_000_000`); the renderer owns any normalization.
pub fn transform_numeric_literal(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let text = get_text(state, node);
	state.ast.number_literal(text)
}

/// `createStringFromLiteral` (`util/createStringFromLiteral.ts`): slices the raw text so escapes
/// survive verbatim.
pub fn create_string_from_literal(state: &State<'_>, node: P<Node>) -> String {
	let text = get_text(state, node);
	let (start, end_trim) = match node.kind() {
		// ts.stripQuotes: drop the surrounding quote or backtick pair; a tail drops `}` and `` ` ``
		Kind::StringLiteral | Kind::NoSubstitutionTemplateLiteral | Kind::TemplateTail => (1, 1),
		// a head drops `` ` `` and `${`, a middle `}` and `${`
		Kind::TemplateHead | Kind::TemplateMiddle => (1, 2),
		_ => return text,
	};
	text.get(start..text.len().saturating_sub(end_trim))
		.unwrap_or_default()
		.to_owned()
}

/// `transformStringLiteral`: the raw inner text; the renderer picks the quote style.
pub fn transform_string_literal(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let text = create_string_from_literal(state, node);
	state.ast.str(text)
}

/// `transformInterpolatedStringPart`: the raw text; the renderer escapes braces and newlines.
pub fn transform_interpolated_string_part(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let text = create_string_from_literal(state, node);
	state.ast.interpolated_string_part(text)
}

/// `transformNoSubstitutionTemplateLiteral`: backtick strings stay interpolated strings.
pub fn transform_no_substitution_template_literal(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let part = transform_interpolated_string_part(state, node);
	let parts = state.ast.list_from([part]);
	state.ast.interpolated_string(parts)
}

/// `transformTemplateExpression`: always an interpolated string. Parts with empty cooked text
/// are skipped; emitted text is the raw slice.
pub fn transform_template_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = node.as_template_expression();
	let parts = state.ast.new_list();

	if !expression.head.text().is_empty() {
		let part = transform_interpolated_string_part(state, expression.head);
		state.ast.list_push(parts, part);
	}

	let spans = expression.template_spans.nodes();
	let span_expressions: Vec<P<Node>> = spans.iter().map(|span| span.as_template_span().expression).collect();
	let ordered_expressions = ensure_transform_order(state, &span_expressions);

	for (index, &span) in spans.iter().enumerate() {
		state.ast.list_push(parts, ordered_expressions[index]);
		let literal = span.as_template_span().literal;
		if !literal.text().is_empty() {
			let part = transform_interpolated_string_part(state, literal);
			state.ast.list_push(parts, part);
		}
	}

	state.ast.interpolated_string(parts)
}

/// `transformTaggedTemplateExpression`: the tag is called with the array of raw string parts,
/// then the ordered substitutions.
pub fn transform_tagged_template_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let tagged = node.as_tagged_template_expression();
	let tag = transform_expression(state, tagged.tag);

	let mut string_parts = Vec::new();
	let mut args = Vec::new();

	let template = tagged.template;
	if tsast::is_template_expression(template) {
		let template_expression = template.as_template_expression();
		string_parts.push(state.ast.str(template_expression.head.text()));
		let spans = template_expression.template_spans.nodes();
		let mut expressions = Vec::with_capacity(spans.len());
		for &span in spans {
			let span = span.as_template_span();
			string_parts.push(state.ast.str(span.literal.text()));
			expressions.push(span.expression);
		}
		args = ensure_transform_order(state, &expressions);
	} else {
		string_parts.push(state.ast.str(template.text()));
	}

	let members = state.ast.list_from(string_parts);
	let mut call_args = vec![state.ast.array(members)];
	call_args.extend(args);
	let callee = convert_to_indexable_expression(&mut state.ast, tag);
	let call_args = state.ast.list_from(call_args);
	state.ast.call(callee, call_args)
}

/// `transformArrayLiteralExpression`. Without spreads, `{ e1, e2 }` inline. With spreads, an
/// array pointer materializes into `local _array = { ... }` and `local _length = #_array` at the
/// first spread (or the first element with prereqs); spreads append through the shared builder
/// and later elements assign to `_array[_length + n]`. `amtElementsSinceUpdate` is not reset by
/// spreads (verbatim upstream).
pub fn transform_array_literal_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let elements = node.as_array_literal_expression().elements.nodes();
	if !elements.iter().any(|&element| tsast::is_spread_element(element)) {
		let ordered = ensure_transform_order(state, elements);
		let members = state.ast.list_from(ordered);
		return state.ast.array(members);
	}

	let mut pointer = create_array_pointer(state, "array");
	let length_id = state.ast.temp_id("length");
	let mut length_initialized = false;
	let mut amount_since_update = 0_usize;

	let update_length_id =
		|state: &mut State<'_>, pointer_value: NodeId, length_initialized: &mut bool, amount: &mut usize| {
			let right = state.ast.unary(UnaryOperator::Length, pointer_value);
			if *length_initialized {
				let assignment = state.ast.assignment(length_id, AssignmentOperator::Assign, right);
				state.prereq(assignment);
			} else {
				let declaration = state.ast.variable_declaration(length_id, Some(right.into()));
				state.prereq(declaration);
				*length_initialized = true;
			}
			*amount = 0;
		};

	for (index, &element) in elements.iter().enumerate() {
		if tsast::is_spread_element(element) {
			if state.ast.get::<Array>(pointer.value).is_some() {
				disable_array_inline(state, &mut pointer);
				update_length_id(state, pointer.value, &mut length_initialized, &mut amount_since_update);
			}
			let array_id = pointer.value;
			if !state.ast.kind(array_id).is_any_identifier() {
				state.ice("transformer: transformArrayLiteralExpression: pointer is not an identifier");
				return state.ast.none();
			}

			let Some(spread_expression) = element.expression() else {
				continue;
			};
			let t = state.get_type(spread_expression);
			let builder = expressions::get_add_iterable_to_array_builder(state, spread_expression, t);
			let spread = transform_expression(state, spread_expression);
			let should_update_length_id = index < elements.len() - 1;
			let statements = builder(
				state,
				spread,
				array_id,
				length_id,
				amount_since_update,
				should_update_length_id,
			);
			state.prereq_list(statements);
		} else {
			let (expression, prereqs) = state.capture(|state| transform_expression(state, element));
			if state.ast.get::<Array>(pointer.value).is_some() && state.ast.list_is_non_empty(prereqs) {
				disable_array_inline(state, &mut pointer);
				update_length_id(state, pointer.value, &mut length_initialized, &mut amount_since_update);
			}
			if let Some(array) = state.ast.get::<Array>(pointer.value) {
				let members = array.members;
				state.ast.list_push(members, expression);
			} else {
				state.prereq_list(prereqs);
				let offset = u32::try_from(amount_since_update + 1).unwrap_or(u32::MAX);
				let offset = state.ast.num(f64::from(offset));
				let position = state.ast.binary(length_id, BinaryOperator::Add, offset);
				let target = state.ast.computed_index(pointer.value, position);
				let assignment = state.ast.assignment(target, AssignmentOperator::Assign, expression);
				state.prereq(assignment);
			}
			amount_since_update += 1;
		}
	}

	pointer.value
}

/// `transformObjectLiteralExpression`: builds a map pointer, inline until a member's transform
/// produces prereqs, then a temp with per-field assignments.
pub fn transform_object_literal_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let mut pointer = create_map_pointer(state, "object");
	for &property in node.as_object_literal_expression().properties.nodes() {
		validate_method_assignment(state, property);
		match property.kind() {
			Kind::PropertyAssignment => {
				let Some(name) = property.name() else {
					continue;
				};
				if name.kind() == Kind::PrivateIdentifier {
					state.diags.add(diagnostics::no_private_identifier(name));
					continue;
				}
				let Some(initializer) = property.initializer() else {
					continue;
				};
				transform_property_assignment(state, &mut pointer, name, initializer);
				set_map_field_source_position(state, &pointer, property);
			}
			Kind::ShorthandPropertyAssignment => {
				// `{ a }`: the name doubles as the initializer.
				let Some(name) = property.name() else {
					continue;
				};
				transform_property_assignment(state, &mut pointer, name, name);
				set_map_field_source_position(state, &pointer, property);
			}
			Kind::SpreadAssignment => expressions::transform_spread_assignment(state, &mut pointer, property),
			Kind::MethodDeclaration => {
				let statements = control::transform_method_declaration(state, property, &mut pointer);
				state.prereq_list(statements);
				set_map_field_source_position(state, &pointer, property);
			}
			// must be an accessor, which is banned
			_ => state.diags.add(diagnostics::no_getter_setter(property)),
		}
	}
	pointer.value
}

/// `setMapFieldSourcePosition`
fn set_map_field_source_position(state: &mut State<'_>, pointer: &MapPointer, property: P<Node>) {
	if state.source_map_enabled
		&& let Some(map) = state.ast.get::<Map>(pointer.value)
		&& let Some(tail) = state.ast.list_tail(map.fields)
	{
		let field = state.ast.link_value(tail);
		let position = state.original_source_position(Some(property), false);
		state.set_source_position(field, position);
	}
}

/// `transformPropertyAssignment`: when either side produced prereqs the inline table can no
/// longer hold the field, so the pointer degrades to a temp and the key is pinned.
fn transform_property_assignment(state: &mut State<'_>, pointer: &mut MapPointer, name: P<Node>, initializer: P<Node>) {
	let (mut left, left_prereqs) = state.capture(|state| transform_property_name(state, name));
	let (right, right_prereqs) = state.capture(|state| transform_expression(state, initializer));

	if state.ast.list_is_non_empty(left_prereqs) || state.ast.list_is_non_empty(right_prereqs) {
		disable_map_inline(state, pointer);
		state.prereq_list(left_prereqs);
		// With luauTypes on, a literal key is not pinned, so Luau keeps the spilled table's
		// shape; only a computed key needs the pin for its evaluation order.
		if !state.is_luau_typed || !state.ast.kind(left).is_simple_primitive() {
			left = state.push_to_var(Some(left), "left");
		}
	}

	state.prereq_list(right_prereqs);

	assign_to_map_pointer(state, pointer, left, right);
}

/// `transformPropertyName` (`nodes/transformPropertyName.ts`): `a` is `"a"`, `[a]` transforms
/// the expression, literal names transform directly.
pub fn transform_property_name(state: &mut State<'_>, name: P<Node>) -> NodeId {
	if name.kind() == Kind::Identifier {
		return state.ast.str(name.text());
	}
	if tsast::is_computed_property_name(name)
		&& let Some(expression) = name.expression()
	{
		return transform_expression(state, expression);
	}
	transform_expression(state, name)
}

/// `transformParenthesizedExpression`: unwraps, and keeps Luau parens only around non-simple
/// results.
pub fn transform_parenthesized_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let inner = node.expression().map_or(node, tsast::skip_downwards);
	let expression = transform_expression(state, inner);
	if state.ast.kind(expression).is_simple() {
		return expression;
	}
	state.ast.parenthesized(expression)
}
