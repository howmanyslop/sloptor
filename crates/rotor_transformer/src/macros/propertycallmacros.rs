//! `propertycallmacros.go`: the math region of `macros/propertyCallMacros.ts`
//! (`makeMathMethod`, `makeMathSet`) and the comment logic every property-call macro is wrapped
//! in (`header`, `footer`, `wasExpressionPushed`, `wrapComments`). The table rows live in
//! [`super::PROPERTY_CALL_MACROS`].

#![expect(
	clippy::needless_pass_by_value,
	reason = "the macro tables fix the `CallMacro` signature, which takes the arguments by value"
)]

use rotor_luau::VariableDeclaration;

use super::arg;
use crate::call::run_call_macro;
use crate::macromanager::{PropertyCallMacro, PropertyCallMacroEntry};
use crate::prelude::*;

/// `makeMathMethod`: the method call compiles to a Luau binary operation. A non-simple right
/// operand gets explicit parentheses (which also truncate call multi-returns); left-operand
/// precedence parentheses are the renderer's job.
fn make_math_method(state: &mut State<'_>, expression: NodeId, args: &[NodeId], operator: BinaryOperator) -> NodeId {
	let mut rhs = arg(state, args, 0);
	if !state.ast.kind(rhs).is_simple() {
		rhs = state.ast.parenthesized(rhs);
	}
	state.ast.binary(expression, operator, rhs)
}

/// `add` (`+`)
pub(super) fn math_add(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_math_method(state, expression, &args, BinaryOperator::Add)
}

/// `sub` (`-`)
pub(super) fn math_sub(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_math_method(state, expression, &args, BinaryOperator::Subtract)
}

/// `mul` (`*`)
pub(super) fn math_mul(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_math_method(state, expression, &args, BinaryOperator::Multiply)
}

/// `div` (`/`)
pub(super) fn math_div(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_math_method(state, expression, &args, BinaryOperator::Divide)
}

/// `idiv` (`//`)
pub(super) fn math_idiv(state: &mut State<'_>, _node: P<Node>, expression: NodeId, args: Vec<NodeId>) -> NodeId {
	make_math_method(state, expression, &args, BinaryOperator::FloorDivide)
}

/// Runs a property-call macro through `runCallMacro` with upstream's comment wrapping (Go wraps
/// every table entry in `init`).
pub fn run_property_call_macro(
	entry: PropertyCallMacroEntry,
	state: &mut State<'_>,
	node: P<Node>,
	base_expression: NodeId,
	node_arguments: &[P<Node>],
) -> NodeId {
	let name = entry.name();
	let macro_fn = entry.macro_fn;
	run_call_macro(
		|state, node, expression, args| wrap_comments(&name, macro_fn, state, node, expression, args),
		state,
		node,
		base_expression,
		node_arguments,
	)
}

/// `macroCommentHeader`: `-- ▼ ReadonlyArray.reduce ▼`.
fn macro_comment_header(ast: &mut Ast, text: &str) -> NodeId {
	ast.comment(format!(" ▼ {text} ▼"))
}

/// `macroCommentFooter`: `-- ▲ ReadonlyArray.reduce ▲`.
fn macro_comment_footer(ast: &mut Ast, text: &str) -> NodeId {
	ast.comment(format!(" ▲ {text} ▲"))
}

/// `wasExpressionPushed`: the first prereq is `local <temp> = <the incoming base expression>`,
/// the `pushToVarIfComplex(expression, "exp")` a macro begins with.
fn was_expression_pushed(ast: &Ast, statements: Statements, expression: NodeId) -> bool {
	let Some(head) = ast.list_head(statements) else {
		return false;
	};
	let Some(declaration) = ast.get::<VariableDeclaration>(ast.link_value(head)) else {
		return false;
	};
	declaration
		.left
		.node()
		.is_some_and(|left| ast.kind(left) == LuauKind::TemporaryIdentifier)
		&& declaration.right.and_then(NodeOrList::node) == Some(expression)
}

/// `wrapComments`: captures the macro's prereqs and, when more than one statement remains after
/// the header-exempt base push, brackets them with `-- ▼ X ▼` / `-- ▲ X ▲`.
pub(super) fn wrap_comments(
	method_name: &str,
	callback: PropertyCallMacro,
	state: &mut State<'_>,
	call_node: P<Node>,
	call_exp: NodeId,
	args: Vec<NodeId>,
) -> NodeId {
	let (expression, prereqs) = state.capture(|state| callback(state, call_node, call_exp, args));

	let mut size = state.ast.list_size(prereqs);
	if size > 0 {
		// detect the case of `expression = state.pushToVarIfComplex(expression, "exp");` and put
		// header after
		let was_pushed = was_expression_pushed(&state.ast, prereqs, call_exp);
		let mut push_statement = None;
		if was_pushed {
			push_statement = state.ast.list_shift(prereqs);
			size -= 1;
		}
		if size > 1 {
			let header = macro_comment_header(&mut state.ast, method_name);
			state.ast.list_unshift(prereqs, header);
			if let Some(push_statement) = push_statement {
				state.ast.list_unshift(prereqs, push_statement);
			}
			let footer = macro_comment_footer(&mut state.ast, method_name);
			state.ast.list_push(prereqs, footer);
		} else if let Some(push_statement) = push_statement {
			state.ast.list_unshift(prereqs, push_statement);
		}
	}

	state.prereq_list(prereqs);
	expression
}
