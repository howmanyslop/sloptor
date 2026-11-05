//! `statementlist.go`: `nodes/transformStatementList.ts`, the statement list driver used by the
//! source file and every block.

use std::collections::HashMap;

use rotor_tsrs::ast::{self as tsast, Kind as TsKind, NodeFlags};
use rotor_tsrs::scanner;

use crate::dispatch::transform_statement;
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::prelude::*;
use crate::util::{is_ancestor_of, node_end, node_pos};
use crate::{control, diagnostics, luau_types};

/// `ExportInfo`: the namespace export assignments appended after the statements that declare
/// exported names (`containerId.<name> = <name>`).
#[derive(Clone, Debug)]
pub struct ExportInfo {
	/// The namespace container's identifier.
	pub id: NodeId,
	/// The exported names each statement declares, in order.
	pub mapping: HashMap<P<Node>, Vec<String>>,
}

/// `TransformStatementList`. Per statement: leading comments, then the hoisted `local a, b`, then
/// prereqs, then the transformed statements, then namespace export assignments. Iteration stops
/// after a final statement (`break`/`continue`/`return`).
///
/// ROTOR EXTENSION (loop labels): a function body is a label barrier; this is the one choke point
/// every function-like body passes through.
pub fn transform_statement_list(
	state: &mut State<'_>,
	parent: Option<P<Node>>,
	statements: &[P<Node>],
	export_info: Option<&ExportInfo>,
) -> Statements {
	if let Some(parent) = parent
		&& tsast::is_block(parent)
		&& parent.parent().is_some_and(tsast::is_function_like)
	{
		return state
			.without_labels(|state| transform_statement_list_worker(state, Some(parent), statements, export_info));
	}
	transform_statement_list_worker(state, parent, statements, export_info)
}

fn transform_statement_list_worker(
	state: &mut State<'_>,
	parent: Option<P<Node>>,
	statements: &[P<Node>],
	export_info: Option<&ExportInfo>,
) -> Statements {
	let result = state.ast.new_list();
	let array_append_plans = control::analyze_array_append_statements(state, statements);

	for (index, &statement) in statements.iter().enumerate() {
		// Hoists registered while transforming statement N (a use before its declaration inside
		// N) are picked up for N itself: the transform runs before createHoistDeclaration.
		let mut transformed = None;
		let prereqs = state.capture_statements(|state| {
			if let Some(plan) = array_append_plans.get(&statement) {
				let zero = state.ast.num(0.0);
				let declaration = state.ast.variable_declaration(plan.target.length_id, Some(zero.into()));
				state.prereq(declaration);
				state.array_append_plans.push(plan.clone());
				transformed = Some(transform_statement(state, statement));
				state.array_append_plans.pop();
			} else {
				transformed = Some(transform_statement(state, statement));
			}
		});
		let transformed = transformed.unwrap_or_else(|| state.ast.new_list());

		if !state.remove_comments() {
			let comments = state.get_leading_comments(node_pos(statement));
			state.ast.list_push_list(result, comments);
		}

		if let Some(hoist) = create_hoist_declaration(state, statement) {
			state.ast.list_unshift(prereqs, hoist);
		}

		state.set_statement_list_source_positions(statement, &[prereqs, transformed]);
		state.ast.list_push_list(result, prereqs);
		let last_node = state.ast.list_tail(transformed);
		state.ast.list_push_list(result, transformed);

		if let Some(last_node) = last_node
			&& state.ast.kind(state.ast.link_value(last_node)).is_final_statement()
		{
			report_unreachable_hoisted_functions(state, &statements[index + 1..]);
			break;
		}

		if let Some(export_info) = export_info
			&& let Some(names) = export_info.mapping.get(&statement)
		{
			for export_name in names {
				let target = state.ast.property_access(export_info.id, export_name.as_str());
				let value = state.ast.id(export_name.as_str());
				let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value);
				let position = state.original_source_position(Some(statement), false);
				state.set_source_position(assignment, position);
				state.ast.list_push(result, assignment);
			}
		}
	}

	if !state.remove_comments()
		&& let Some(last_token_pos) = get_last_token_pos(state.source_file, parent, statements)
	{
		let comments = state.get_leading_comments(last_token_pos);
		state.ast.list_push_list(result, comments);
	}

	result
}

fn report_unreachable_hoisted_functions(state: &mut State<'_>, statements: &[P<Node>]) {
	if state.is_hoisted.is_empty() {
		return;
	}
	for &statement in statements {
		if !tsast::is_function_declaration(statement) {
			continue;
		}
		if statement.body().is_none() {
			continue;
		}
		let Some(name) = statement.name() else {
			continue;
		};
		let symbol = state.ck().get_symbol_at_location(name);
		if symbol.is_some_and(|symbol| state.is_hoisted.get(&symbol).copied().unwrap_or(false)) {
			state
				.diags
				.add(diagnostics::rotor_unreachable_hoisted_function(statement));
		}
	}
}

/// `getLastToken` (`transformStatementList.ts`): the trailing `}` or end-of-file token whose
/// leading trivia holds the block's trailing comments, with upstream's
/// `!isNodeDescendantOf(lastToken, lastStatement)` guard. Only the token's full start is used, so
/// this returns that.
fn get_last_token_pos(source_file: P<SourceFile>, parent: Option<P<Node>>, statements: &[P<Node>]) -> Option<i64> {
	if let Some(&last_statement) = statements.last() {
		let statement_parent = last_statement.parent()?;
		let token = last_token(statement_parent, source_file)?;
		if is_ancestor_of(last_statement, token.owner) {
			return None;
		}
		return Some(token.pos);
	}
	parent
		.and_then(|parent| last_token(parent, source_file))
		.map(|token| token.pos)
}

/// The last token of a node, as `lsutil.GetLastToken` finds it: its full start, and the node that
/// owns it (the token node itself, or the node whose trailing text was scanned).
struct LastToken {
	pos: i64,
	owner: P<Node>,
}

/// Go `lsutil.GetLastToken`, which the tsrs mirror lacks (it has no language service crate).
fn last_token(node: P<Node>, source_file: P<SourceFile>) -> Option<LastToken> {
	if tsast::is_token_kind(node.kind()) || node.kind() == TsKind::Identifier {
		return None;
	}
	let last_child = last_child(node, source_file)?;
	match last_child {
		Child::Token(pos) => Some(LastToken { pos, owner: node }),
		Child::Node(child) if child.kind() < TsKind::FIRST_NODE => Some(LastToken {
			pos: node_pos(child),
			owner: child,
		}),
		Child::Node(child) => last_token(child, source_file),
	}
}

enum Child {
	/// A token scanned from the node's trailing text, by its full start.
	Token(i64),
	/// A visited child node.
	Node(P<Node>),
}

/// Go `lsutil.GetLastChild`: the last token scanned after the last visited child, or that child.
fn last_child(node: P<Node>, source_file: P<SourceFile>) -> Option<Child> {
	let last_visited = last_visited_child(node);
	let token_start = last_visited.map_or_else(|| node_pos(node), node_end);
	let end = node_end(node);
	let mut last_token = None;
	if let Ok(start) = u32::try_from(token_start) {
		let mut scanner = scanner::get_scanner_for_source_file(source_file, start);
		let mut start_pos = token_start;
		while start_pos < end {
			let token_end = i64::from(scanner.token_end());
			last_token = Some(i64::from(scanner.token_full_start()));
			if token_end <= start_pos {
				break;
			}
			start_pos = token_end;
			scanner.scan();
		}
	}
	last_token.map(Child::Token).or_else(|| last_visited.map(Child::Node))
}

/// Go `lsutil.GetLastVisitedChild`: the last non-reparsed child. `JSDoc` precedes its host node, so
/// it never is the last child of a statement list's parent.
fn last_visited_child(node: P<Node>) -> Option<P<Node>> {
	let mut last = None;
	node.for_each_child(&mut |child| {
		if !child.flags().intersects(NodeFlags::Reparsed) {
			last = Some(child);
		}
		false
	});
	last
}

impl State<'_> {
	/// `RemoveComments`: `compilerOptions.removeComments === true`.
	#[must_use]
	pub fn remove_comments(&self) -> bool {
		self.program.options().remove_comments.is_true()
	}

	/// `GetLeadingComments` (`TransformState.ts` L139-153): the comments in the leading trivia at
	/// `pos`. `// foo` keeps ` foo`; `/* foo */` keeps ` foo `.
	pub fn get_leading_comments(&mut self, pos: i64) -> Statements {
		let result = self.ast.new_list();
		let Ok(pos) = u32::try_from(pos) else {
			return result;
		};
		let text = self.source_file_text;
		let ranges: Vec<_> = scanner::get_leading_comment_ranges(text, pos).collect();
		for range in ranges {
			let mut end = range.end() as usize;
			if range.kind != TsKind::SingleLineCommentTrivia {
				end -= 2; // strip the trailing "*/"
			}
			let start = range.pos() as usize + 2;
			let comment = self.ast.comment(text.get(start..end).unwrap_or_default());
			self.ast.list_push(result, comment);
		}
		result
	}
}

/// `createHoistDeclaration` (`util/createHoistDeclaration.ts`): a `local a, b` for the
/// identifiers used before their declaration, emitted right before `statement`.
pub fn create_hoist_declaration(state: &mut State<'_>, statement: P<Node>) -> Option<NodeId> {
	let hoists = state.hoists_by_statement.get(&statement)?.clone();
	if hoists.is_empty() {
		return None;
	}
	let left = state.ast.new_list::<element::AnyIdentifier>();
	for hoist in hoists {
		validate_identifier(state, hoist);
		let mut id = transform_identifier_defined(state, hoist);
		if state.is_luau_typed
			&& let Some(symbol) = state.ck().get_symbol_at_location(hoist)
		{
			// A hoisted `local x` has no initializer to infer from, so it carries the symbol's
			// declared type.
			let local_type = luau_types::get_luau_local_type(state, hoist, None);
			id = luau_types::luau_typed_binding(state, symbol, id, local_type);
		}
		state.ast.list_push(left, id);
	}
	Some(state.ast.variable_declaration(left, None))
}
