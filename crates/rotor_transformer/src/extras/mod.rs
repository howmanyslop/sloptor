//! Stage 4b slice (vi), JSX and the rest: `jsx.go`, `jsxtext.go`, `async.go`, `decorators.go`,
//! `trystatement.go`, `labeledstatement.go`, `imports.go`, `importexpr.go` and `sourcemap.go`.
//!
//! Stubs with the signatures core code calls, except the two `trystatement.go` predicates core
//! statements need, which are ported. The slice port replaces the bodies and may add submodules
//! below this file. See `SLICES.md`.

use rotor_tsrs::ast as tsast;

use crate::prelude::*;

/// Per-file state owned by this slice.
#[derive(Debug, Default)]
pub struct SliceState {}

/// Pass-wide state owned by this slice.
#[derive(Debug, Default)]
pub struct MultiSliceState {}

fn expression_stub(state: &mut State<'_>, node: P<Node>, what: &str) -> NodeId {
	state.not_yet_ported(node, what);
	state.ast.none()
}

fn statement_stub(state: &mut State<'_>, node: P<Node>, what: &str) -> Statements {
	state.not_yet_ported(node, what);
	state.ast.new_list()
}

/// `isReturnBlockedByTryStatement` (`trystatement.go`; `util/isBlockedByTryStatement.ts`): a
/// try statement is reached before any function-like.
#[must_use]
pub fn is_return_blocked_by_try_statement(node: P<Node>) -> bool {
	tsast::find_ancestor(node, |ancestor| {
		tsast::is_try_statement(ancestor) || tsast::is_function_like_declaration(ancestor)
	})
	.is_some_and(tsast::is_try_statement)
}

/// `isBreakBlockedByTryStatement` (`trystatement.go`): a try statement is reached before any
/// loop or switch.
#[must_use]
pub fn is_break_blocked_by_try_statement(node: P<Node>) -> bool {
	tsast::find_ancestor(node, |ancestor| {
		tsast::is_try_statement(ancestor)
			|| tsast::is_iteration_statement(ancestor, false)
			|| tsast::is_switch_statement(ancestor)
	})
	.is_some_and(tsast::is_try_statement)
}

/// `transformImportExpression` (`importexpr.go`): `import("...")`.
pub fn transform_import_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "import expressions")
}

/// `transformAwaitExpression` (`async.go`).
pub fn transform_await_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "await expressions")
}

/// `transformYieldExpression` (`async.go`).
pub fn transform_yield_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "yield expressions")
}

/// `transformJsxElement` (`jsx.go`).
pub fn transform_jsx_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "JSX")
}

/// `transformJsxExpression` (`jsx.go`).
pub fn transform_jsx_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "JSX")
}

/// `transformJsxFragment` (`jsx.go`).
pub fn transform_jsx_fragment(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "JSX")
}

/// `transformJsxSelfClosingElement` (`jsx.go`).
pub fn transform_jsx_self_closing_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	expression_stub(state, node, "JSX")
}

/// `transformExportAssignment` (`imports.go`): `export =` and `export default`.
pub fn transform_export_assignment(state: &mut State<'_>, node: P<Node>) -> Statements {
	statement_stub(state, node, "export assignments")
}

/// `transformExportDeclaration` (`imports.go`).
pub fn transform_export_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	statement_stub(state, node, "export declarations")
}

/// `transformImportDeclaration` (`imports.go`).
pub fn transform_import_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	statement_stub(state, node, "import declarations")
}

/// `transformImportEqualsDeclaration` (`imports.go`).
pub fn transform_import_equals_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	statement_stub(state, node, "import-equals declarations")
}

/// `transformLabeledStatement` (`labeledstatement.go`).
pub fn transform_labeled_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	statement_stub(state, node, "labeled statements")
}

/// `transformTryStatement` (`trystatement.go`).
pub fn transform_try_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	statement_stub(state, node, "try statements")
}

/// `wrapStatementsAsGenerator` (`async.go`), called by the function port (slice ii).
pub fn wrap_statements_as_generator(state: &mut State<'_>, node: P<Node>, statements: Statements) -> Statements {
	state.not_yet_ported(node, "generator functions");
	statements
}

/// `transformDecorators` (`decorators.go`), called for every class by the class port (slice iii).
/// A class with no decorators anywhere gets no statements, as in Go; any decorator reports.
pub fn transform_decorators(state: &mut State<'_>, node: P<Node>, _class_id: NodeId) -> Statements {
	let decorated = tsast::has_decorators(node)
		|| node.members().iter().any(|&member| {
			tsast::has_decorators(member)
				|| (tsast::is_constructor_declaration(member)
					&& member
						.parameters()
						.iter()
						.any(|&parameter| tsast::has_decorators(parameter)))
		});
	if decorated {
		state.not_yet_ported(node, "decorators");
	}
	state.ast.new_list()
}

/// `getModuleTreeImportParts` (`importexpr.go`), called by the `$getModuleTree` macro port
/// (slice iv): the import path parts with rotor's folder extension.
pub fn get_module_tree_import_parts(
	state: &mut State<'_>,
	_source_file: P<SourceFile>,
	module_specifier: P<Node>,
) -> Vec<NodeId> {
	state.not_yet_ported(module_specifier, "module tree imports");
	Vec::new()
}
