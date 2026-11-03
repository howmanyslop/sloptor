//! `jsx.go`: the roblox-ts 3.0 JSX surface (`transformJsxElement.ts`,
//! `transformJsxSelfClosingElement.ts`, `transformJsxFragment.ts`, `transformJsxExpression.ts`,
//! `nodes/jsx/transformJsx.ts`, `transformJsxTagName.ts`, `transformJsxAttributes.ts` and
//! `transformJsxChildren.ts`).

use rotor_tsrs::ast::{self as tsast};
use rotor_tsrs::tscore::TextRange;
use rotor_tsrs::types::TypeCheck;
use rotor_tsrs::{parser, scanner};

use super::imports::transform_entity_name;
use super::jsxtext::fixup_whitespace_and_decode_entities;
use crate::call::convert_to_indexable_expression;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::ensureorder::ensure_transform_order_with;
use crate::pointer::{MapPointer, assign_to_map_pointer, create_map_pointer, disable_map_inline};
use crate::prelude::*;
use crate::truthiness::create_truthiness_checks;
use crate::types::{get_first_defined_symbol, is_definitely_type};

const OBJECT_CHECKS: [TypeCheck; 2] = [TypeCheck::Object, TypeCheck::NonPrimitive];

/// `transformJsxElement` (L5-7).
pub fn transform_jsx_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let element = node.as_jsx_element();
	let opening = element.opening_element.as_jsx_opening_element();
	transform_jsx(
		state,
		node,
		opening.tag_name,
		opening.attributes,
		element.children.nodes(),
	)
}

/// `transformJsxSelfClosingElement` (L5-7): no children.
pub fn transform_jsx_self_closing_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let element = node.as_jsx_self_closing_element();
	transform_jsx(state, node, element.tag_name, element.attributes, &[])
}

/// The factory entity's callee: `React.createElement`.
fn create_element_expression(state: &mut State<'_>, node: P<Node>) -> Option<NodeId> {
	// jsxFactoryEntity seems to always be defined and will default to `React.createElement`
	let Some(jsx_factory_entity) = state.ck().get_jsx_factory_entity(node) else {
		state.ice("Expected jsxFactoryEntity to be defined");
		return None;
	};
	let expression = transform_entity_name(state, jsx_factory_entity);
	Some(convert_to_indexable_expression(&mut state.ast, expression))
}

/// `transformJsx` (`nodes/jsx/transformJsx.ts` L12-46): `<factory>(tag, attrs-or-nil,
/// ...children)`. The `nil` placeholder appears only with children and no attributes.
fn transform_jsx(
	state: &mut State<'_>,
	node: P<Node>,
	tag_name: P<Node>,
	attributes: P<Node>,
	children: &[P<Node>],
) -> NodeId {
	let Some(create_element) = create_element_expression(state, node) else {
		return state.ast.none();
	};

	let tag_name_exp = transform_jsx_tag_name(state, tag_name);

	let attributes_ptr = if attributes.properties().is_empty() {
		None
	} else {
		let mut pointer = create_map_pointer(state, "attributes");
		transform_jsx_attributes(state, attributes, &mut pointer);
		Some(pointer)
	};

	let transformed_children = transform_jsx_children(state, children);

	let args = state.ast.list_from([tag_name_exp]);

	if let Some(attributes_ptr) = &attributes_ptr {
		state.ast.list_push(args, attributes_ptr.value);
	} else if !transformed_children.is_empty() {
		let nil = state.ast.nil();
		state.ast.list_push(args, nil);
	}

	for child in transformed_children {
		state.ast.list_push(args, child);
	}

	state.ast.call(create_element, args)
}

/// `transformJsxFragment` (L9-34). Fragments never have attributes, so they always push `nil`
/// before children.
pub fn transform_jsx_fragment(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let Some(create_element) = create_element_expression(state, node) else {
		return state.ast.none();
	};

	// getJsxFragmentFactoryEntity() doesn't seem to default to "Fragment".. but the typechecker
	// does, so we should follow that behavior
	let jsx_fragment_factory_entity = state.ck().get_jsx_fragment_factory_entity(node).or_else(|| {
		// upstream ts.parseIsolatedEntityName("Fragment", ESNext), marked synthetic like
		// checker.markAsSynthetic so TransformIdentifier takes its synthetic early-out.
		let entity = parser::parse_isolated_entity_name("Fragment");
		if let Some(entity) = entity {
			mark_entity_as_synthetic(entity);
		}
		entity
	});
	let Some(jsx_fragment_factory_entity) = jsx_fragment_factory_entity else {
		state.ice("Unable to find valid jsxFragmentFactoryEntity");
		return state.ast.none();
	};

	// Not convertToIndexableExpression: the fragment factory is an argument.
	let fragment = transform_entity_name(state, jsx_fragment_factory_entity);
	let args = state.ast.list_from([fragment]);

	let transformed_children = transform_jsx_children(state, node.as_jsx_fragment().children.nodes());

	// props parameter
	if !transformed_children.is_empty() {
		let nil = state.ast.nil();
		state.ast.list_push(args, nil);
	}

	for child in transformed_children {
		state.ast.list_push(args, child);
	}

	state.ast.call(create_element, args)
}

/// `markEntityAsSynthetic`: every node of the parsed entity gets the synthetic range.
fn mark_entity_as_synthetic(node: P<Node>) -> bool {
	node.set_loc(TextRange::synthetic());
	node.for_each_child(&mut mark_entity_as_synthetic);
	false
}

/// `transformJsxExpression` (L6-15): a `{expr}` or `{...expr}` child. Note global `unpack`, not
/// `table.unpack`.
pub fn transform_jsx_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = node.as_jsx_expression();
	if let Some(inner) = expression.expression {
		let exp = transform_expression(state, inner);
		if expression.dot_dot_dot_token.is_some() {
			let unpack = state.ast.global_id("unpack");
			let args = state.ast.list_from([exp]);
			return state.ast.call(unpack, args);
		}
		return exp;
	}
	state.ast.none()
}

// ---------------------------------------------------------------------------------------------
// Tag names (nodes/jsx/transformJsxTagName.ts)
// ---------------------------------------------------------------------------------------------

/// `getTextOfJsxNamespacedName`: `namespace ":" name`.
fn get_text_of_jsx_namespaced_name(node: P<Node>) -> String {
	let namespaced = node.as_jsx_namespaced_name();
	format!("{}:{}", namespaced.namespace.text(), namespaced.name.text())
}

/// `transformJsxTagNameExpression` (L9-28).
fn transform_jsx_tag_name_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	if tsast::is_identifier(node) && scanner::is_intrinsic_jsx_name(node.text()) {
		return state.ast.str(node.text());
	}

	if tsast::is_property_access_expression(node) {
		let access = node.as_property_access_expression();
		if tsast::is_private_identifier(access.name) {
			state.diags.add(diagnostics::no_private_identifier(access.name));
		}
		let expression = transform_expression(state, access.expression);
		let expression = convert_to_indexable_expression(&mut state.ast, expression);
		state.ast.property_access(expression, access.name.text())
	} else if tsast::is_jsx_namespaced_name(node) {
		state.ast.str(get_text_of_jsx_namespaced_name(node))
	} else {
		transform_expression(state, node)
	}
}

/// `transformJsxTagName` (L30-38): the tag expression is pinned to a `tagName` temp only when
/// it captured prereqs.
fn transform_jsx_tag_name(state: &mut State<'_>, tag_name: P<Node>) -> NodeId {
	let (expression, prereqs) = state.capture(|state| transform_jsx_tag_name_expression(state, tag_name));
	if state.ast.list_is_non_empty(prereqs) {
		state.prereq_list(prereqs);
		return state.push_to_var_if_complex(expression, "tagName");
	}
	expression
}

// ---------------------------------------------------------------------------------------------
// Attributes (nodes/jsx/transformJsxAttributes.ts)
// ---------------------------------------------------------------------------------------------

/// `createJsxAttributeLoop` (L11-48): `for _k, _v in <exp> do _attributes[_k] = _v end`, wrapped
/// in a truthiness `if` (with the expression pinned to an `attribute` temp) when the spread
/// operand is not definitely an object.
fn create_jsx_attribute_loop(
	state: &mut State<'_>,
	attributes_ptr_value: NodeId,
	mut expression: NodeId,
	ts_expression: P<Node>,
) -> NodeId {
	let t = state.get_type(ts_expression);
	let definitely_object = is_definitely_type(state, t, &OBJECT_CHECKS);
	if !definitely_object {
		expression = state.push_to_var_if_complex(expression, "attribute");
	}

	let key_id = state.ast.temp_id("k");
	let value_id = state.ast.temp_id("v");
	let ids = state.ast.list_from([key_id, value_id]);
	let target = state.ast.computed_index(attributes_ptr_value, key_id);
	let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value_id);
	let body = state.ast.list_from([assignment]);
	let mut statement = state.ast.for_statement(ids, expression, body);

	if !definitely_object {
		let t = state.get_type(ts_expression);
		let condition = create_truthiness_checks(state, expression, ts_expression, t);
		let body = state.ast.list_from([statement]);
		statement = state.ast.if_statement(condition, body, None);
	}

	statement
}

/// `transformJsxAttribute` (L50-68).
fn transform_jsx_attribute(state: &mut State<'_>, attribute: P<Node>, attributes_ptr: &mut MapPointer) {
	let jsx_attribute = attribute.as_jsx_attribute();
	let mut initializer = jsx_attribute.initializer;
	if let Some(jsx_expression) = initializer
		&& tsast::is_jsx_expression(jsx_expression)
	{
		// QUIRK: a `{}` initializer has no expression, folding into the implicit-true branch:
		// `<frame Visible={}/>` emits `Visible = true`.
		initializer = jsx_expression.as_jsx_expression().expression;
	}

	let (init, init_prereqs) = if let Some(initializer) = initializer {
		state.capture(|state| transform_expression(state, initializer))
	} else {
		let value = state.ast.bool(true);
		(value, state.ast.new_list())
	};

	if state.ast.list_is_non_empty(init_prereqs) {
		// disableMapInline before prereqList: `local _attributes = {...so far...}` precedes the
		// initializer's prereqs.
		disable_map_inline(state, attributes_ptr);
		state.prereq_list(init_prereqs);
	}

	let name = jsx_attribute.name;
	let text = if tsast::is_identifier(name) {
		name.text().to_owned()
	} else {
		get_text_of_jsx_namespaced_name(name)
	};
	let key = state.ast.str(text);
	assign_to_map_pointer(state, attributes_ptr, key, init);
}

/// `transformJsxAttributes` (L70-103).
fn transform_jsx_attributes(state: &mut State<'_>, attributes: P<Node>, attributes_ptr: &mut MapPointer) {
	let properties = attributes.properties();
	for (index, &attribute) in properties.iter().enumerate() {
		if tsast::is_jsx_attribute(attribute) {
			transform_jsx_attribute(state, attribute, attributes_ptr);
			continue;
		}

		// spread attributes: `<frame { ...x }/>`
		let ts_expression = attribute.as_jsx_spread_attribute().expression;
		let t = state.get_type(ts_expression);
		let exp_type = state.ck().get_non_optional_type(t);
		let symbol = get_first_defined_symbol(state, exp_type);
		if let Some(symbol) = symbol
			&& state.macros().is_macro_only_class(symbol)
		{
			state.diags.add(diagnostics::no_macro_object_spread(attribute));
		}

		let expression = transform_expression(state, ts_expression);

		if index == 0 && is_definitely_type(state, exp_type, &OBJECT_CHECKS) {
			// First property overall and definitely an object: the clone fast path.
			let clone = state.ast.global_property("table", "clone");
			let args = state.ast.list_from([expression]);
			let call = state.ast.call(clone, args);
			attributes_ptr.value = state.push_to_var(Some(call), &attributes_ptr.name);
			// Explicitly remove metatable because things like classes can be spread
			let setmetatable = state.ast.global_id("setmetatable");
			let nil = state.ast.nil();
			let args = state.ast.list_from([attributes_ptr.value, nil]);
			let call = state.ast.call(setmetatable, args);
			let statement = state.ast.call_statement(call);
			state.prereq(statement);
			continue;
		}

		disable_map_inline(state, attributes_ptr);
		let attribute_loop = create_jsx_attribute_loop(state, attributes_ptr.value, expression, ts_expression);
		state.prereq(attribute_loop);
	}
}

// ---------------------------------------------------------------------------------------------
// Children (nodes/jsx/transformJsxChildren.ts)
// ---------------------------------------------------------------------------------------------

/// `!isJsxText(child) || !containsOnlyTriviaWhiteSpaces`
fn is_significant_jsx_child(child: P<Node>) -> bool {
	!tsast::is_jsx_text(child) || !child.as_jsx_text().contains_only_trivia_white_spaces
}

/// `transformJsxChildren` (L11-39). The spread-must-be-last scan runs on the unfiltered list for
/// indices below the last significant child; the transform then drops whitespace-only text and
/// empty `{}` expressions and runs the standard ensureTransformOrder walk.
fn transform_jsx_children(state: &mut State<'_>, children: &[P<Node>]) -> Vec<NodeId> {
	if children.is_empty() {
		return Vec::new();
	}
	// findLastIndex(children, child => !isJsxText(child) || !child.containsOnlyTriviaWhiteSpaces)
	let last_jsx_child_index = children.iter().rposition(|&child| is_significant_jsx_child(child));

	if let Some(last_jsx_child_index) = last_jsx_child_index {
		for &child in &children[..last_jsx_child_index] {
			if tsast::is_jsx_expression(child) && child.as_jsx_expression().dot_dot_dot_token.is_some() {
				state.diags.add(diagnostics::no_preceding_jsx_spread_element(child));
			}
		}
	}

	let filtered: Vec<P<Node>> = children
		.iter()
		.copied()
		// ignore jsx text that only contains whitespace, and empty jsx expressions (`{}`)
		.filter(|&child| {
			is_significant_jsx_child(child)
				&& !(tsast::is_jsx_expression(child) && child.as_jsx_expression().expression.is_none())
		})
		.collect();

	ensure_transform_order_with(state, &filtered, |state, node| {
		if tsast::is_jsx_text(node) {
			let text = fixup_whitespace_and_decode_entities(node.as_jsx_text().text());
			// QUIRK: double every backslash. Luau string values are emitted raw, so rbxtsc writes
			// `"back\\slash"` for source text `back\slash`.
			return state.ast.str(text.replace('\\', "\\\\"));
		}
		transform_expression(state, node)
	})
}
