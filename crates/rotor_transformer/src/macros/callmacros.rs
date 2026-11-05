//! `callmacros.go`: `macros/callMacros.ts` and `macros/identifierMacros.ts`. Neither table is
//! comment-wrapped (`wrapComments` applies only to the property-call macros).

#![expect(
	clippy::needless_pass_by_value,
	reason = "the macro tables fix the `CallMacro` signature, which takes the arguments by value"
)]

use rotor_tsrs::ast as tsast;

use super::arg;
use crate::call::convert_to_indexable_expression;
use crate::prelude::*;
use crate::truthiness::create_truthiness_checks;
use crate::{diagnostics, extras};

/// `primitiveLuauTypes` (`PRIMITIVE_LUAU_TYPES`): the strings Luau's `type()` can return, for
/// which `typeIs` compiles to the cheaper `type(value)`.
const PRIMITIVE_LUAU_TYPES: &[&str] = &[
	"nil", "boolean", "string", "number", "table", "userdata", "function", "thread", "vector", "buffer",
];

/// `Promise` (`IDENTIFIER_MACROS`): a bare `Promise` reads the runtime library's class.
pub(super) fn promise(state: &mut State<'_>, node: P<Node>) -> NodeId {
	state.runtime_lib(Some(node), "Promise")
}

/// `assert(value, message?)`: the condition gets the full truthiness expansion.
pub(super) fn assert(state: &mut State<'_>, node: P<Node>, _expression: NodeId, mut args: Vec<NodeId>) -> NodeId {
	let Some(&condition_node) = node.as_call_expression().arguments.nodes().first() else {
		state.ice("transformer: assert macro without an argument");
		return state.ast.none();
	};
	let t = state.get_type(condition_node);
	let first = arg(state, &args, 0);
	let checked = create_truthiness_checks(state, first, condition_node, t);
	if let Some(slot) = args.first_mut() {
		*slot = checked;
	}
	let callee = state.ast.global_id("assert");
	let args = state.ast.list_from(args);
	state.ast.call(callee, args)
}

/// `typeOf(...)`: `typeof(args...)`.
pub(super) fn type_of(state: &mut State<'_>, _node: P<Node>, _expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let callee = state.ast.global_id("typeof");
	let args = state.ast.list_from(args);
	state.ast.call(callee, args)
}

/// `typeIs(value, typeStr)`: `type(value) == typeStr` for a primitive Luau type string literal,
/// else `typeof(value) == typeStr`.
pub(super) fn type_is(state: &mut State<'_>, _node: P<Node>, _expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let value = arg(state, &args, 0);
	let type_str = arg(state, &args, 1);
	let is_primitive = state
		.ast
		.get::<rotor_luau::StringLiteral>(type_str)
		.is_some_and(|literal| PRIMITIVE_LUAU_TYPES.contains(&literal.value.as_str()));
	let type_func = if is_primitive { "type" } else { "typeof" };
	let callee = state.ast.global_id(type_func);
	let call_args = state.ast.list_from([value]);
	let call = state.ast.call(callee, call_args);
	state.ast.binary(call, BinaryOperator::Equal, type_str)
}

/// `classIs(instance, className)`: `value.ClassName == typeStr`.
pub(super) fn class_is(state: &mut State<'_>, _node: P<Node>, _expression: NodeId, args: Vec<NodeId>) -> NodeId {
	let value = arg(state, &args, 0);
	let type_str = arg(state, &args, 1);
	let indexable = convert_to_indexable_expression(&mut state.ast, value);
	let class_name = state.ast.property_access(indexable, "ClassName");
	state.ast.binary(class_name, BinaryOperator::Equal, type_str)
}

/// `identity(v)`: its argument, verbatim.
pub(super) fn identity(state: &mut State<'_>, _node: P<Node>, _expression: NodeId, args: Vec<NodeId>) -> NodeId {
	arg(state, &args, 0)
}

/// `$range(...)`: the for-of transform intercepts the valid position, so reaching the call
/// macro is an error.
pub(super) fn range(state: &mut State<'_>, node: P<Node>, _expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	state.diags.add(diagnostics::no_range_macro_outside_for_of(
		node.as_call_expression().expression,
	));
	state.ast.none()
}

/// `$tuple(...)`: return statements intercept the valid position, so reaching the call macro is
/// an error.
pub(super) fn tuple(state: &mut State<'_>, node: P<Node>, _expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	state.diags.add(diagnostics::no_tuple_macro_outside_return(node));
	state.ast.none()
}

/// `$getModuleTree(specifier)`: `{ root, { "rest", "of", "path" } }`. Reads the raw TS argument,
/// like upstream: the macro needs the module specifier, not its transformed value.
pub(super) fn get_module_tree(state: &mut State<'_>, node: P<Node>, _expression: NodeId, _args: Vec<NodeId>) -> NodeId {
	let Some(&specifier) = node.as_call_expression().arguments.nodes().first() else {
		state.ice("transformer: $getModuleTree macro without an argument");
		return state.ast.none();
	};
	let source_file = tsast::get_source_file_of_node(node).unwrap_or(state.source_file);
	let parts = extras::get_module_tree_import_parts(state, source_file, specifier);
	let Some((&root, rest_parts)) = parts.split_first() else {
		// Go indexes `parts[0]`, which getImportPartsImpl never leaves empty
		state.ice("transformer: $getModuleTree: no import parts");
		return state.ast.none();
	};
	let rest = state.ast.list_from(rest_parts.iter().copied());
	let rest = state.ast.array(rest);
	let members = state.ast.list_from([root, rest]);
	state.ast.array(members)
}
