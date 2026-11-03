//! `stringmacros.go`: the `String` and `ArrayLike` region of `macros/propertyCallMacros.ts`
//! (`makeStringCallback`, `STRING_CALLBACKS`, `argumentsWithDefaults`, `ARRAY_LIKE_METHODS`).

use crate::macromanager::PropertyCallMacro;
use crate::prelude::*;
use crate::valuetoidstr::value_to_id_str;

/// `makeStringCallback`: `string.<name>(expression, ...args)`, the base first and the arguments
/// verbatim (the Luau string API is 1-based, so nothing is offset).
fn make_string_callback(state: &mut State<'_>, name: &str, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let callee = state.ast.global_property("string", name);
	let args = state.ast.list_from(std::iter::once(expression).chain(args));
	state.ast.call(callee, args)
}

/// `sizeMacro`: `#expression` (`STRING_CALLBACKS.size` and `ARRAY_LIKE_METHODS.size`).
pub(super) fn size_macro(state: &mut State<'_>, _node: P<Node>, expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	state.ast.unary(UnaryOperator::Length, expression)
}

macro_rules! string_callbacks {
	($($fn_name:ident => $name:literal,)*) => {
		$(
			#[doc = concat!("`string.", $name, "`")]
			fn $fn_name(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
				make_string_callback(state, $name, expression, args)
			}
		)*
	};
}

string_callbacks! {
	string_byte => "byte",
	string_find => "find",
	string_format => "format",
	string_gmatch => "gmatch",
	string_gsub => "gsub",
	string_lower => "lower",
	string_match => "match",
	string_rep => "rep",
	string_reverse => "reverse",
	string_split => "split",
	string_sub => "sub",
	string_upper => "upper",
}

/// `stringCallbacks` (`STRING_CALLBACKS`). None of these produce prereqs.
pub(super) const STRING_CALLBACKS: &[(&str, PropertyCallMacro)] = &[
	("size", size_macro),
	("byte", string_byte),
	("find", string_find),
	("format", string_format),
	("gmatch", string_gmatch),
	("gsub", string_gsub),
	("lower", string_lower),
	("match", string_match),
	("rep", string_rep),
	("reverse", string_reverse),
	("split", string_split),
	("sub", string_sub),
	("upper", string_upper),
];

/// `arrayLikeMethods` (`ARRAY_LIKE_METHODS`).
pub(super) const ARRAY_LIKE_METHODS: &[(&str, PropertyCallMacro)] = &[("size", size_macro)];

/// `argumentsWithDefaults`: each provided argument that is not a simple primitive is pushed to a
/// temp guarded by `if <temp> == nil then <temp> = <default> end`; each missing trailing argument
/// becomes its default.
pub(super) fn arguments_with_defaults(
	state: &mut State<'_>,
	mut args: Vec<NodeId>,
	defaults: &[NodeId],
) -> Vec<NodeId> {
	// potentially nil arguments
	for index in 0..args.len() {
		if !state.ast.kind(args[index]).is_simple_primitive() {
			let mut name = value_to_id_str(&state.ast, args[index]);
			if name.is_empty() {
				name = format!("arg{index}");
			}
			let temp = state.push_to_var(Some(args[index]), &name);
			args[index] = temp;
			let nil = state.ast.nil();
			let condition = state.ast.binary(temp, BinaryOperator::Equal, nil);
			let Some(&default) = defaults.get(index) else {
				// Go indexes `defaults[i]` and panics past its end
				state.ice("transformer: argumentsWithDefaults: more arguments than defaults");
				return args;
			};
			let assignment = state.ast.assignment(temp, AssignmentOperator::Assign, default);
			let statements = state.ast.list_from([assignment]);
			let check = state.ast.if_statement(condition, statements, None);
			state.prereq(check);
		}
	}

	// not specified
	if let Some(missing) = defaults.get(args.len()..) {
		args.extend_from_slice(missing);
	}

	args
}
