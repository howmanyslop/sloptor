//! Stage 4b slice (vi), JSX and the rest: `jsx.go`, `jsxtext.go`, `async.go`, `decorators.go`,
//! `trystatement.go`, `labeledstatement.go`, `imports.go`, `importexpr.go` and `sourcemap.go`.
//!
//! The functions here keep the signatures that core code and the other slices call and delegate
//! to one submodule per Go file. The two `trystatement.go` predicates core statements need live
//! here. `sourcemap.go`'s `RenderWithSourceMap` is [`State::render_with_source_map`].

mod async_;
mod decorators;
mod importexpr;
mod imports;
mod jsx;
mod jsxtext;
mod labeledstatement;
mod sourcemap;
mod trystatement;

#[cfg(test)]
mod tests;

use rotor_tsrs::ast as tsast;

use crate::prelude::*;

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
	importexpr::transform_import_expression(state, node)
}

/// `transformAwaitExpression` (`async.go`).
pub fn transform_await_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	async_::transform_await_expression(state, node)
}

/// `transformYieldExpression` (`async.go`).
pub fn transform_yield_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	async_::transform_yield_expression(state, node)
}

/// `transformJsxElement` (`jsx.go`).
pub fn transform_jsx_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	jsx::transform_jsx_element(state, node)
}

/// `transformJsxExpression` (`jsx.go`).
pub fn transform_jsx_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	jsx::transform_jsx_expression(state, node)
}

/// `transformJsxFragment` (`jsx.go`).
pub fn transform_jsx_fragment(state: &mut State<'_>, node: P<Node>) -> NodeId {
	jsx::transform_jsx_fragment(state, node)
}

/// `transformJsxSelfClosingElement` (`jsx.go`).
pub fn transform_jsx_self_closing_element(state: &mut State<'_>, node: P<Node>) -> NodeId {
	jsx::transform_jsx_self_closing_element(state, node)
}

/// `transformExportAssignment` (`imports.go`): `export =` and `export default`.
pub fn transform_export_assignment(state: &mut State<'_>, node: P<Node>) -> Statements {
	imports::transform_export_assignment(state, node)
}

/// `transformExportDeclaration` (`imports.go`).
pub fn transform_export_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	imports::transform_export_declaration(state, node)
}

/// `transformImportDeclaration` (`imports.go`).
pub fn transform_import_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	imports::transform_import_declaration(state, node)
}

/// `transformImportEqualsDeclaration` (`imports.go`).
pub fn transform_import_equals_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	imports::transform_import_equals_declaration(state, node)
}

/// `transformLabeledStatement` (`labeledstatement.go`).
pub fn transform_labeled_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	labeledstatement::transform_labeled_statement(state, node)
}

/// `transformTryStatement` (`trystatement.go`).
pub fn transform_try_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	trystatement::transform_try_statement(state, node)
}

/// `wrapStatementsAsGenerator` (`async.go`), called by the function port (slice ii).
pub fn wrap_statements_as_generator(state: &mut State<'_>, node: P<Node>, statements: Statements) -> Statements {
	async_::wrap_statements_as_generator(state, node, statements)
}

/// `transformDecorators` (`decorators.go`), called for every class by the class port (slice iii).
/// A class with no decorators anywhere gets no statements.
pub fn transform_decorators(state: &mut State<'_>, node: P<Node>, class_id: NodeId) -> Statements {
	decorators::transform_decorators(state, node, class_id)
}

/// `getModuleTreeImportParts` (`importexpr.go`), called by the `$getModuleTree` macro port
/// (slice iv): the import path parts with rotor's folder extension.
pub fn get_module_tree_import_parts(
	state: &mut State<'_>,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
) -> Vec<NodeId> {
	importexpr::get_module_tree_import_parts(state, source_file, module_specifier)
}
