//! Port of upstream `LuauRenderer/solveTempIds.ts`: assigns rendered names to
//! temporary identifiers, scope-aware so they never collide with source
//! identifiers or other temps.

use std::collections::{HashMap, HashSet};

use crate::list::element;
use crate::render::visit::{self, Visitor};
use crate::render::{RenderResult, RenderState, fail};
use crate::{Ast, List, NodeData, NodeId, NodeOrList, SyntaxKind};

type ScopeId = usize;

/// Upstream `Scope`. Maps start empty and never allocate until written, since
/// most scopes never register a name.
#[derive(Default)]
struct TempScope {
	ids: HashSet<String>,
	last_try: HashMap<String, usize>,
	/// Per base: the `_` separator cursor, then the empty separator cursor.
	next_try: HashMap<String, [Option<usize>; 2]>,
	parent: Option<ScopeId>,
}

struct Scopes(Vec<TempScope>);

impl Scopes {
	fn push(&mut self, parent: Option<ScopeId>) -> ScopeId {
		self.0.push(TempScope {
			parent,
			..TempScope::default()
		});
		self.0.len() - 1
	}

	/// Upstream `scopeHasId`: checks this scope and all ancestors.
	fn has(&self, mut scope: ScopeId, id: &str) -> bool {
		loop {
			let current = &self.0[scope];
			if current.ids.contains(id) {
				return true;
			}
			match current.parent {
				Some(parent) => scope = parent,
				None => return false,
			}
		}
	}

	fn add_id(&mut self, scope: ScopeId, id: &str) {
		let ids = &mut self.0[scope].ids;
		if !ids.contains(id) {
			ids.insert(id.to_owned());
		}
	}

	fn next_id(&mut self, scope: ScopeId, base: &str, separator: &str) -> String {
		let family = usize::from(separator.is_empty());
		let mut i = self.0[scope]
			.next_try
			.get(base)
			.and_then(|cursors| cursors[family])
			.unwrap_or_else(|| {
				// A final-name counter is written only when that name is
				// reserved, so the legacy starting suffix for this base cannot
				// later change.
				self.0[scope].last_try.get(base).copied().unwrap_or(1)
			});
		let mut input = base.to_owned();
		while self.has(scope, &input) {
			input = format!("{base}{separator}{i}");
			i += 1;
		}
		let current = &mut self.0[scope];
		// Preserve upstream's final-name counter: overlapping hints can read
		// it.
		current.last_try.insert(input.clone(), i);
		current.ids.insert(input.clone());
		// All skipped candidates remain reserved in this scope or an
		// ancestor. Keep separate cursors for anonymous "_1" and exact "__1"
		// suffixes.
		current.next_try.entry(base.to_owned()).or_default()[family] = Some(i);
		input
	}
}

const fn is_fully_scoped_node(kind: SyntaxKind) -> bool {
	matches!(kind, SyntaxKind::ForStatement | SyntaxKind::NumericForStatement) || kind.is_function_like()
}

fn identifier_list_contains(ast: &Ast, identifiers: List<element::AnyIdentifier>, target: NodeId) -> bool {
	ast.list_iter(identifiers).any(|value| value == target)
}

fn is_identifier_declaration(ast: &Ast, identifier: NodeId) -> bool {
	let Some(parent) = ast.parent(identifier) else {
		return false;
	};
	match ast.data(parent) {
		NodeData::VariableDeclaration(declaration) => match declaration.left {
			NodeOrList::Node(left) => ast.kind(left) == SyntaxKind::Identifier && left == identifier,
			NodeOrList::List(list) => list
				.typed::<element::AnyIdentifier>()
				.is_some_and(|list| identifier_list_contains(ast, list, identifier)),
		},
		NodeData::FunctionDeclaration(declaration) => {
			declaration.localize && declaration.name == identifier
				|| identifier_list_contains(ast, declaration.parameters, identifier)
		}
		NodeData::FunctionExpression(function) => identifier_list_contains(ast, function.parameters, identifier),
		NodeData::MethodDeclaration(method) => identifier_list_contains(ast, method.parameters, identifier),
		NodeData::TypeFunctionDeclaration(function) => identifier_list_contains(ast, function.parameters, identifier),
		NodeData::ForStatement(statement) => identifier_list_contains(ast, statement.ids, identifier),
		NodeData::NumericForStatement(statement) => statement.id == identifier,
		_ => false,
	}
}

/// Mirrors upstream: `node` is the head (`head`) or tail statement of its
/// parent's statement list, or of an if statement's else list.
fn is_scope_edge(ast: &Ast, node: NodeId, head: bool) -> bool {
	let Some(parent) = ast.parent(node) else {
		return false;
	};
	let edge_of = |list: List<element::Statement>| {
		let link = if head { ast.list_head(list) } else { ast.list_tail(list) };
		link.map(|link| ast.link_value(link))
	};
	if ast.kind(parent).has_statements()
		&& let Some(statements) = ast.statements_of(parent)
		&& edge_of(statements) == Some(node)
	{
		return true;
	}
	// A non-list else body would have the else body itself as a parent,
	// which would be an if statement and handled above.
	if let NodeData::IfStatement(statement) = ast.data(parent)
		&& let NodeOrList::List(list) = statement.else_body
		&& let Some(list) = list.typed::<element::Statement>()
		&& edge_of(list) == Some(node)
	{
		return true;
	}
	false
}

struct Solve {
	scopes: Scopes,
	stack: Vec<ScopeId>,
	temps: Vec<NodeId>,
	/// The scope each temporary was last seen in, indexed by [`NodeId`].
	node_scopes: Vec<Option<ScopeId>>,
	unsolved_iterators: Vec<NodeId>,
	/// Set when a scope edge pops more scopes than were pushed. Go indexes
	/// the empty stack and panics; the walk records it and fails.
	underflow: bool,
}

impl Solve {
	/// The innermost scope. On an empty stack this records the underflow and
	/// returns the root scope so the walk can finish before failing.
	fn peek(&mut self) -> ScopeId {
		if let Some(&scope) = self.stack.last() {
			return scope;
		}
		self.underflow = true;
		0
	}

	fn push(&mut self) {
		let parent = self.peek();
		let scope = self.scopes.push(Some(parent));
		self.stack.push(scope);
	}

	fn pop(&mut self) {
		if self.stack.pop().is_none() {
			self.underflow = true;
		}
	}

	fn register_id(&mut self, name: &str) {
		let scope = self.peek();
		self.scopes.add_id(scope, name);
	}

	fn reserve_identifier_use(&mut self, name: &str) {
		let mut scope = Some(self.peek());
		while let Some(current) = scope {
			self.scopes.add_id(current, name);
			scope = self.scopes.0[current].parent;
		}
	}
}

impl Visitor for Solve {
	fn before(&mut self, ast: &Ast, node: NodeId) {
		let kind = ast.kind(node);
		if let NodeData::FunctionDeclaration(declaration) = ast.data(node)
			&& declaration.localize
			&& let NodeData::Identifier(identifier) = ast.data(declaration.name)
		{
			self.register_id(&identifier.name);
		}
		if is_fully_scoped_node(kind) {
			self.push();
		}
		if is_scope_edge(ast, node, true) {
			self.push();
		}
		match ast.data(node) {
			NodeData::ForStatement(statement) => self.unsolved_iterators.push(statement.expression),
			NodeData::TemporaryIdentifier(_) => {
				self.node_scopes[node.index()] = Some(self.peek());
				self.temps.push(node);
			}
			NodeData::Identifier(identifier) => {
				if is_identifier_declaration(ast, node) {
					self.register_id(&identifier.name);
				} else {
					self.reserve_identifier_use(&identifier.name);
				}
			}
			NodeData::VariableDeclaration(declaration) => match declaration.left {
				NodeOrList::List(list) => {
					if let Some(list) = list.typed::<element::AnyIdentifier>() {
						for id in ast.list_iter(list) {
							if let NodeData::Identifier(identifier) = ast.data(id) {
								self.register_id(&identifier.name);
							}
						}
					}
				}
				NodeOrList::Node(left) => {
					if let NodeData::Identifier(identifier) = ast.data(left) {
						self.register_id(&identifier.name);
					}
				}
			},
			_ => {
				if let Some(params) = ast.param_data(node) {
					for id in ast.list_iter(params.params) {
						if let NodeData::Identifier(identifier) = ast.data(id) {
							self.register_id(&identifier.name);
						}
					}
				}
			}
		}
	}

	fn after(&mut self, ast: &Ast, node: NodeId) {
		if is_fully_scoped_node(ast.kind(node)) {
			self.pop();
		}
		if is_scope_edge(ast, node, false) {
			self.pop();
		}
	}
}

/// Names every temporary reachable from `root` in `state`. Returns the `for`
/// iterator subtrees the scoped walk skips, for fallback handling.
///
/// Fails when scope edges pop more scopes than they push, which happens when
/// a list's head or tail changed after construction and so has no parent.
pub fn solve_temp_ids(state: &mut RenderState, ast: &Ast, root: NodeOrList) -> RenderResult<Vec<NodeId>> {
	let mut scopes = Scopes(Vec::new());
	let root_scope = scopes.push(None);
	let mut solve = Solve {
		scopes,
		stack: vec![root_scope],
		temps: Vec::new(),
		node_scopes: vec![None; ast.node_count()],
		unsolved_iterators: Vec::new(),
		underflow: false,
	};
	visit::visit_node_or_list(ast, root, &mut solve);
	if solve.underflow {
		return fail("render: solveTempIDs scope stack underflow");
	}

	for &temp_node in &solve.temps {
		let NodeData::TemporaryIdentifier(temp) = ast.data(temp_node) else {
			continue;
		};
		if state.is_temp_solved(temp.id) {
			continue;
		}
		let Some(scope) = solve.node_scopes[temp_node.index()] else {
			continue;
		};
		let (base, separator) = if temp.exact_name {
			(temp.name.clone(), "_")
		} else {
			(format!("_{}", temp.name), if temp.name.is_empty() { "" } else { "_" })
		};
		let name = solve.scopes.next_id(scope, &base, separator);
		state.set_temp_name(temp.id, name);
	}
	Ok(solve.unsolved_iterators)
}
