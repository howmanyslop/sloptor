//! `sourcefile.go`: `nodes/transformSourceFile.ts`.

use rotor_luau::Comment;
use rotor_luau::render::{DEFAULT_TYPE_LINE_WIDTH, layout_types};
use rotor_rojo::RbxType;
use rotor_support::version;

use crate::exports::handle_exports;
use crate::luau_types;
use crate::prelude::*;
use crate::statementlist::transform_statement_list;

/// `HeaderComment`: the build comment's text, rendered as `--<text>`.
#[must_use]
pub fn header_comment() -> String {
	format!(" Compiled with sloptor v{}", version::VERSION)
}

/// `TransformSourceFile`: the file's final Luau statements, ready to render.
pub fn transform_source_file(state: &mut State<'_>) -> Statements {
	let node = state.source_file;
	// The `//!native` directive only annotates with nativeTypes on; the comment itself is
	// emitted by the printers regardless.
	state.is_native = state.native_types && luau_types::has_native_directive(state);
	state.is_luau_typed = state.luau_types;

	// The file's module symbol maps to `exports`.
	let Some(symbol) = state.ck().get_symbol_at_location(node.as_node()) else {
		// Upstream assert: roblox-ts validates non-module files elsewhere.
		state.ice("transformer: source file has no module symbol");
		return state.ast.new_list();
	};
	let exports = state.ast.global_id("exports");
	state.set_module_id_by_symbol(symbol, exports);

	let statements = transform_statement_list(state, Some(node.as_node()), node.statements().nodes(), None);

	handle_exports(state, node, symbol, statements);

	// ModuleScripts `return nil` when they export nothing (transformSourceFile.ts:213-220).
	// States without a Rojo context treat every file as a ModuleScript.
	let is_module_script = state.rojo.is_none_or(|rojo| {
		let output_path = rojo.path_translator.get_output_path(node.file_name());
		rojo.resolver.get_rbx_type_from_file_path(&output_path) == RbxType::ModuleScript
	});
	ensure_module_return(&mut state.ast, statements, is_module_script);

	prepend_header(state, statements);

	if state.is_luau_typed {
		// Full-fidelity aliases run long; break wide tables and alias-body unions across lines.
		if let Err(error) = layout_types(&mut state.ast, statements, DEFAULT_TYPE_LINE_WIDTH) {
			state.ice(format!("transformer: type layout: {}", error.message()));
		}
	}

	statements
}

/// `ensureModuleReturn` (L213-220): a plain `return nil` when the output is a `ModuleScript` and
/// the last non-comment statement is not already a return.
pub fn ensure_module_return(ast: &mut Ast, statements: Statements, is_module_script: bool) {
	let last_statement = get_last_non_comment_statement(ast, ast.list_tail(statements));
	let ends_with_return =
		last_statement.is_some_and(|link| ast.kind(ast.link_value(link)) == LuauKind::ReturnStatement);
	if !ends_with_return && is_module_script {
		let nil = ast.nil();
		let statement = ast.return_statement(nil);
		ast.list_push(statements, statement);
	}
}

/// `getLastNonCommentStatement` (L191-196): walks back past comments.
fn get_last_non_comment_statement(
	ast: &Ast,
	mut link: Option<rotor_luau::ListNodeId>,
) -> Option<rotor_luau::ListNodeId> {
	while let Some(current) = link
		&& ast.kind(ast.link_value(current)) == LuauKind::Comment
	{
		link = ast.link_prev(current);
	}
	link
}

/// `prependHeader` (L222-242): the version comment and, when the file used the runtime library,
/// the `local TS = ...` import; leading `--!` directive comments hoist above it. Hoisted type
/// aliases follow the header.
pub fn prepend_header(state: &mut State<'_>, statements: Statements) {
	let header_statements = state.ast.new_list();
	let header = state.ast.comment(header_comment());
	state.ast.list_push(header_statements, header);

	if state.uses_runtime_lib {
		let import = state.create_runtime_lib_import(state.source_file);
		state.ast.list_push(header_statements, import);
	}

	let hoisted = if state.is_luau_typed {
		luau_types::luau_hoisted_type_statements(state)
	} else {
		luau_types::native_hoisted_type_statements(state)
	};
	state.ast.list_push_list(header_statements, hoisted);

	// Only a run of `!` comments at the very head qualifies.
	let directive_comments = state.ast.new_list();
	while let Some(head) = state.ast.list_head(statements) {
		let value = state.ast.link_value(head);
		let is_directive = state
			.ast
			.get::<Comment>(value)
			.is_some_and(|comment| comment.text.starts_with('!'));
		if !is_directive {
			break;
		}
		if let Some(shifted) = state.ast.list_shift(statements) {
			state.ast.list_push(directive_comments, shifted);
		}
	}

	state.ast.list_unshift_list(statements, header_statements);
	state.ast.list_unshift_list(statements, directive_comments);
}
