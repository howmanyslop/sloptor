//! `transform.go`: the transform's entry point, its state, and the query caches it hoists.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;
use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFlags, SourceFile, Symbol, TokenFlags, get_source_file_of_node, is_expression, is_for_of_statement,
	is_statement,
};
use rotor_tsrs::checker::{Checker, TypeChecker};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::printer::{EmitContext, NodeFactory, new_emit_context};
use rotor_tsrs::tscore::{RESOLUTION_MODE_COMMON_JS, RESOLUTION_MODE_ESM, RESOLUTION_MODE_NONE, TextRange};

/// `Config`: the transformer's tsconfig plugin entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
	/// `jecsPackage`; empty is `@rbxts/jecs`.
	pub jecs_package: String,
	/// `silent`
	pub silent: bool,
}

/// `Input`: one transform over a program's source files.
pub struct Input<'a> {
	/// The program.
	pub program: &'static Program,
	/// The checker the transform asks.
	pub checker: &'a mut Checker,
	/// The files, in caller order.
	pub files: Vec<P<SourceFile>>,
	/// The plugin entry.
	pub config: Config,
	/// Receives the warnings the JavaScript transformer prints.
	pub warn: Option<&'a dyn Fn(&str)>,
}

/// `Result`: every input file, transformed or not, in input order.
pub struct TransformResult {
	/// The files.
	pub files: Vec<P<SourceFile>>,
	/// Whether each file changed.
	pub changed: Vec<bool>,
	/// The emit context the transformed files print with.
	pub emit_context: P<EmitContext>,
}

/// Nodes in order: statements, declarations or terms.
pub(crate) type Statements = Vec<P<Node>>;

/// A JavaScript `throw` or failed `assert`: its message.
pub(crate) type Fail = String;

/// Go's `fail`.
pub(crate) fn fail<T>(message: impl Into<String>) -> Result<T, Fail> {
	Err(message.into())
}

/// `queryPattern`. Go's `\s` is `[\t\n\f\r ]` and its `\b` is ASCII.
static QUERY_PATTERN: LazyLock<Option<Regex>> = LazyLock::new(|| {
	Regex::new(r#"(?:\.[\t\n\x0C\r ]*query(?-u:\b)|\[[\t\n\x0C\r ]*["']query["'][\t\n\x0C\r ]*\])"#).ok()
});

/// `NeedsTransform`: the patched transformer's cheap text test. A file that fails it cannot hold
/// a query or a registration, so it is handed back without a type-checker visit.
#[must_use]
pub fn needs_transform(text: &str) -> bool {
	text.contains("registerComponent<")
		|| text.contains("createBinarySerializer")
		|| QUERY_PATTERN.as_ref().is_some_and(|pattern| pattern.is_match(text))
}

/// `Transform`: runs the transformer over `input.files`.
///
/// # Errors
///
/// The message of the JavaScript transformer's `throw` or failed `assert`.
pub fn transform(input: Input<'_>) -> Result<TransformResult, String> {
	let mut config = input.config;
	if config.jecs_package.is_empty() {
		"@rbxts/jecs".clone_into(&mut config.jecs_package);
	}
	let emit_context = new_emit_context();
	let noop = |_: &str| {};
	let mut state = State {
		program: input.program,
		checker: input.checker,
		emit_context,
		factory: emit_context.factory.clone(),
		config,
		warn: input.warn.unwrap_or(&noop),
		jecs: None,
		prereq_stack: Vec::new(),
		cache_stack: Vec::new(),
		cached_queries: HashMap::new(),
	};
	let mut result = TransformResult {
		files: Vec::with_capacity(input.files.len()),
		changed: Vec::with_capacity(input.files.len()),
		emit_context,
	};
	for file in input.files {
		if file.is_declaration_file() || !needs_transform(file.text()) {
			result.files.push(file);
			result.changed.push(false);
			continue;
		}
		let transformed = state.transform_file(file)?;
		result.files.push(transformed);
		result.changed.push(transformed != file);
	}
	Ok(result)
}

/// `querySymbols`
#[derive(Clone, Copy)]
pub(crate) struct QuerySymbols {
	pub(crate) symbol: P<Symbol>,
	pub(crate) with: P<Symbol>,
	pub(crate) without: P<Symbol>,
	pub(crate) cached: P<Symbol>,
}

/// `jecsSymbols`
#[derive(Clone, Copy)]
pub(crate) struct JecsSymbols {
	pub(crate) query: QuerySymbols,
	pub(crate) cached_query: P<Symbol>,
	pub(crate) wildcard: P<Symbol>,
	pub(crate) world_query: P<Symbol>,
}

/// `cachedQuery`
#[derive(Clone)]
pub(crate) struct CachedQuery {
	pub(crate) archetypes: P<Node>,
	pub(crate) components: Vec<P<Node>>,
}

/// `cacheFrame`
struct CacheFrame {
	node: P<Node>,
	order: Vec<CacheRef>,
	by_world: HashMap<P<Symbol>, CacheRef>,
}

/// A shared `*cache`.
pub(crate) type CacheRef = Rc<RefCell<Cache>>;

/// `state`
pub(crate) struct State<'a> {
	pub(crate) program: &'static Program,
	checker: &'a mut Checker,
	pub(crate) emit_context: P<EmitContext>,
	pub(crate) factory: NodeFactory,
	pub(crate) config: Config,
	pub(crate) warn: &'a dyn Fn(&str),
	jecs: Option<JecsSymbols>,
	prereq_stack: Vec<Vec<P<Node>>>,
	cache_stack: Vec<CacheFrame>,
	pub(crate) cached_queries: HashMap<P<Node>, CachedQuery>,
}

/// Go's `visitor.VisitEachChild(node)` for an emit context's visitor whose callback is `visit`.
///
/// tsrs's visitor callbacks are `'static`, so the children it visits are listed first, mapped in
/// that order by `visit`, and replayed through a second visitor that rebuilds the node as Go's
/// does.
pub(crate) fn visit_each_child(
	emit_context: P<EmitContext>,
	node: P<Node>,
	mut visit: impl FnMut(P<Node>) -> P<Node>,
) -> P<Node> {
	let seen: Rc<RefCell<Vec<P<Node>>>> = Rc::default();
	let sink = Rc::clone(&seen);
	let mut recorder = emit_context.new_node_visitor(Rc::new(move |_, child: P<Node>| {
		sink.borrow_mut().push(child);
		Some(child)
	}));
	recorder.visit_each_child(Some(node));
	let children = std::mem::take(&mut *seen.borrow_mut());
	if children.is_empty() {
		return node;
	}
	let replacements: VecDeque<P<Node>> = children.iter().map(|&child| visit(child)).collect();
	if replacements
		.iter()
		.zip(&children)
		.all(|(replacement, child)| replacement == child)
	{
		return node;
	}
	let queue = Rc::new(RefCell::new(replacements));
	let mut replayer = emit_context.new_node_visitor(Rc::new(move |_, child: P<Node>| {
		Some(queue.borrow_mut().pop_front().unwrap_or(child))
	}));
	replayer.visit_each_child(Some(node)).unwrap_or(node)
}

impl State<'_> {
	/// The checker under rotor's Go-named API.
	pub(crate) const fn ck(&mut self) -> TypeChecker<'_> {
		TypeChecker::new(self.checker)
	}

	/// The loaded jecs symbols. `transformFile` loads them before any visit.
	pub(crate) fn jecs(&self) -> JecsSymbols {
		self.jecs
			.unwrap_or_else(|| unreachable!("jecs symbols are loaded before any visit"))
	}

	/// `visitor.VisitEachChild(node)` with `visit` as the callback; the first failure stops the
	/// visit.
	pub(crate) fn visit_children(
		&mut self,
		node: P<Node>,
		mut visit: impl FnMut(&mut Self, P<Node>) -> Result<P<Node>, Fail>,
	) -> Result<P<Node>, Fail> {
		let mut failure: Option<Fail> = None;
		let emit_context = self.emit_context;
		let visited = visit_each_child(emit_context, node, |child| {
			if failure.is_some() {
				return child;
			}
			match visit(self, child) {
				Ok(visited) => visited,
				Err(error) => {
					failure = Some(error);
					child
				}
			}
		});
		failure.map_or(Ok(visited), Err)
	}

	/// `transform`: TransformState.transform, every child visited with `transformNode`.
	pub(crate) fn transform(&mut self, node: P<Node>) -> Result<P<Node>, Fail> {
		self.visit_children(node, Self::transform_node)
	}

	fn transform_node(&mut self, node: P<Node>) -> Result<P<Node>, Fail> {
		if is_expression(node) {
			return self.transform_expression(node);
		}
		if is_statement(node) {
			let statements = self.transform_statement(node)?;
			return Ok(self.statements(statements));
		}
		self.transform(node)
	}

	/// `statements`: how a JavaScript visitor's array result reaches a tsgo visitor. One node
	/// stands alone; several are spliced as a `SyntaxList`.
	pub(crate) fn statements(&self, nodes: Vec<P<Node>>) -> P<Node> {
		if nodes.len() == 1 {
			return nodes[0];
		}
		self.factory.new_syntax_list(rotor_tsrs::tscore::alloc_vec(nodes))
	}

	pub(crate) fn transform_expression(&mut self, expression: P<Node>) -> Result<P<Node>, Fail> {
		if rotor_tsrs::ast::is_call_expression(expression)
			&& let Some(replaced) = self.cache_query(expression)?
		{
			return Ok(replaced);
		}
		self.transform(expression)
	}

	fn transform_statement(&mut self, statement: P<Node>) -> Result<Vec<P<Node>>, Fail> {
		self.prereq_stack.push(Vec::new());
		let result = (|| {
			let node = if is_for_of_statement(statement) {
				self.inline_query_iterators(statement)?
			} else {
				None
			};
			node.map_or_else(|| self.transform(statement), Ok)
		})();
		let mut prereqs = self.prereq_stack.pop().unwrap_or_default();
		prereqs.push(result?);
		Ok(prereqs)
	}

	/// `prereq`
	pub(crate) fn prereq(&mut self, statement: P<Node>) {
		if let Some(top) = self.prereq_stack.last_mut() {
			top.push(statement);
		}
	}

	fn transform_file(&mut self, file: P<SourceFile>) -> Result<P<SourceFile>, Fail> {
		self.load_jecs_symbols(file)?;
		let original = file.statements().nodes();
		let (statements, changed) = self.transform_statement_list(file.as_node(), original)?;
		if !changed {
			return Ok(file);
		}
		let list = self.factory.new_node_list(statements);
		list.loc.set(file.statements().loc.get());
		Ok(self
			.factory
			.update_source_file(file.as_node(), list, file.end_of_file_token)
			.as_source_file_p())
	}

	fn transform_statement_list(
		&mut self,
		node: P<Node>,
		to_transform: &[P<Node>],
	) -> Result<(Vec<P<Node>>, bool), Fail> {
		let mut end_indices: HashMap<P<Node>, usize> = HashMap::new();
		let mut changed = false;
		self.cache_stack.push(CacheFrame {
			node,
			order: Vec::new(),
			by_world: HashMap::new(),
		});
		let mut statements = Vec::new();
		let mut failure = None;
		for &original in to_transform {
			let transformed = match self.transform_statement(original) {
				Ok(transformed) => transformed,
				Err(error) => {
					failure = Some(error);
					break;
				}
			};
			if transformed.len() != 1 || transformed[0] != original {
				changed = true;
			}
			statements.extend(transformed);
			end_indices.insert(original, statements.len());
		}
		let frame = self.cache_stack.pop();
		if let Some(error) = failure {
			return Err(error);
		}
		let caches = frame.map(|frame| frame.order).unwrap_or_default();
		if !changed && caches.is_empty() {
			return Ok((to_transform.to_vec(), false));
		}

		let mut result = statements;
		let mut offset = 0;
		for cache in &caches {
			if cache.borrow().has_markers() {
				for statement in &mut result {
					*statement = self.replace_markers(cache, *statement);
				}
			}
			let outer_results = cache.borrow().outer_statements.clone();
			let mut insert_index = 0;
			for required in &cache.borrow().requires {
				let end_index = end_indices.get(required).copied().unwrap_or(0);
				if end_index > insert_index {
					insert_index = end_index + offset;
					offset += outer_results.len();
				}
			}
			// Array.prototype.slice clamps, and the offset bookkeeping above can run past the end.
			insert_index = insert_index.min(result.len());
			let mut next = Vec::with_capacity(result.len() + outer_results.len());
			next.extend_from_slice(&result[..insert_index]);
			next.extend(outer_results);
			next.extend_from_slice(&result[insert_index..]);
			result = next;
		}
		Ok((result, true))
	}

	/// The marker visit of `transformStatementList`: every child of `statement`, each marker of
	/// `cache` replaced by its inner results.
	fn replace_markers(&self, cache: &CacheRef, statement: P<Node>) -> P<Node> {
		let emit_context = self.emit_context;
		visit_each_child(emit_context, statement, |child| {
			if let Some(inner) = self.to_inner_results(cache, child) {
				return self.statements(inner);
			}
			self.replace_markers(cache, child)
		})
	}

	fn get_cache(&mut self, position: usize, world: P<Node>) -> Result<CacheRef, Fail> {
		let Some(world_symbol) = self.ck().get_symbol_at_location(world) else {
			return fail("Unable to find symbol for world");
		};
		let frame = &mut self.cache_stack[position];
		if let Some(cache) = frame.by_world.get(&world_symbol) {
			return Ok(Rc::clone(cache));
		}
		let cache = CacheRef::default();
		frame.by_world.insert(world_symbol, Rc::clone(&cache));
		frame.order.push(Rc::clone(&cache));
		Ok(cache)
	}

	pub(crate) fn current_cache(&mut self, world: P<Node>) -> Result<CacheRef, Fail> {
		let position = self.cache_stack.len() - 1;
		self.get_cache(position, world)
	}

	pub(crate) fn current_cache_node(&self) -> Option<P<Node>> {
		self.cache_stack.last().map(|frame| frame.node)
	}

	pub(crate) fn file_cache(&mut self, world: P<Node>) -> Result<CacheRef, Fail> {
		self.get_cache(0, world)
	}

	/// `jecsType`: `import("<jecs>").<type_name><args>`.
	pub(crate) fn jecs_type(&self, type_name: &str, args: Vec<P<Node>>) -> P<Node> {
		let f = &self.factory;
		let type_arguments = (!args.is_empty()).then(|| f.new_node_list(args));
		f.new_import_type_node(
			false,
			f.new_literal_type_node(f.new_string_literal(
				rotor_tsrs::tscore::alloc_str(&self.config.jecs_package),
				TokenFlags::None,
			)),
			None,
			Some(f.new_identifier(rotor_tsrs::tscore::alloc_str(type_name))),
			type_arguments,
		)
	}

	fn load_jecs_symbols(&mut self, file: P<SourceFile>) -> Result<(), Fail> {
		if self.jecs.is_some() {
			return Ok(());
		}
		let package = self.config.jecs_package.clone();
		let Some(jecs_symbol) = self.package_symbol(file, &package) else {
			return fail("Unable to extract type information from Jecs");
		};
		let exports = self.ck().get_exports_of_module(jecs_symbol);
		let find = |name: &str, message: &str| -> Result<P<Symbol>, Fail> {
			exports
				.iter()
				.copied()
				.find(|symbol| symbol.name() == name)
				.map_or_else(|| fail(message), Ok)
		};
		let query_symbol = find("Query", "Unable to find Query type information from Jecs")?;
		let query_type = self.ck().get_declared_type_of_symbol(query_symbol);
		let world_symbol = find("World", "Unable to find World type information from Jecs")?;
		let world_type = self.ck().get_declared_type_of_symbol(world_symbol);
		let mut property = |t, name: &str, message: &str| -> Result<P<Symbol>, Fail> {
			self.ck()
				.get_property_of_type(t, name)
				.map_or_else(|| fail(message), Ok)
		};
		let with = property(
			query_type,
			"with",
			"Unable to find Query.with type information from Jecs",
		)?;
		let without = property(
			query_type,
			"without",
			"Unable to find Query.without type information from Jecs",
		)?;
		let cached = property(
			query_type,
			"cached",
			"Unable to find Query.cached type information from Jecs",
		)?;
		let cached_query = find("CachedQuery", "Unable to find CachedQuery type information from Jecs")?;
		let wildcard = find("Wildcard", "Unable to find Wildcard type information from Jecs")?;
		let world_query = property(
			world_type,
			"query",
			"Unable to find World.query type information from Jecs",
		)?;
		self.jecs = Some(JecsSymbols {
			query: QuerySymbols {
				symbol: query_symbol,
				with,
				without,
				cached,
			},
			cached_query,
			wildcard,
			world_query,
		});
		Ok(())
	}

	/// `packageSymbol` stands in for `ts.resolveModuleName`: the program has already resolved the
	/// package for every file that imports it, so the file's own resolution wins and any other
	/// file's resolution of the same name is the fallback.
	fn package_symbol(&mut self, file: P<SourceFile>, package_name: &str) -> Option<P<Symbol>> {
		let mut resolved_file_name = "";
		for mode in [RESOLUTION_MODE_NONE, RESOLUTION_MODE_ESM, RESOLUTION_MODE_COMMON_JS] {
			if let Some(resolved) = self.program.get_resolved_module(file, package_name, mode)
				&& resolved.is_resolved()
			{
				resolved_file_name = resolved.resolved_file_name;
				break;
			}
		}
		if resolved_file_name.is_empty() {
			// Go ranges over its maps; the fallback here walks them in path and key order, so the
			// first resolution found does not change between runs.
			let modules = self.program.get_resolved_modules();
			let mut paths: Vec<_> = modules.keys().collect();
			paths.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
			'files: for path in paths {
				let mut keys: Vec<_> = modules[path].iter().collect();
				keys.sort_unstable_by_key(|(key, _)| (key.name, key.mode));
				for (key, resolved) in keys {
					if key.name == package_name && resolved.is_resolved() {
						resolved_file_name = resolved.resolved_file_name;
						break 'files;
					}
				}
			}
		}
		if resolved_file_name.is_empty() {
			return None;
		}
		let module_file = self.program.get_source_file(resolved_file_name)?;
		self.ck()
			.get_symbol_at_location(module_file.as_node())
			.or_else(|| module_file.symbol())
	}
}

/// `cacheGroup`
#[derive(Default)]
pub(crate) struct CacheGroup {
	statements: Vec<P<Node>>,
	anchor: Option<TextRange>,
	marker: Option<P<Node>>,
	world_key: Option<P<Node>>,
	condition: Option<P<Node>>,
}

/// `cache`
#[derive(Default)]
pub(crate) struct Cache {
	pub(crate) requires: Vec<P<Node>>,
	pub(crate) outer_statements: Vec<P<Node>>,
	groups: Vec<CacheGroup>,
}

impl Cache {
	/// `require`
	pub(crate) fn require(&mut self, declarations: &[P<Node>]) {
		self.requires.extend_from_slice(declarations);
	}

	/// `outerResult`
	pub(crate) fn outer_result(&mut self, statement: P<Node>) {
		self.outer_statements.push(statement);
	}

	/// `newGroup`
	pub(crate) fn new_group(&mut self, anchor: Option<TextRange>) {
		self.groups.push(CacheGroup {
			anchor,
			..CacheGroup::default()
		});
	}

	fn current_group(&mut self) -> &mut CacheGroup {
		if self.groups.is_empty() {
			self.new_group(None);
		}
		let last = self.groups.len() - 1;
		&mut self.groups[last]
	}

	fn has_markers(&self) -> bool {
		self.groups.iter().any(|group| group.marker.is_some())
	}
}

impl State<'_> {
	/// `cache.innerResult`
	pub(crate) fn inner_result(&mut self, cache: &CacheRef, statements: &[P<Node>]) {
		let mut cache = cache.borrow_mut();
		let group = cache.current_group();
		if group.marker.is_none() {
			let marker = self.factory.new_empty_statement();
			group.marker = Some(marker);
			self.prereq(marker);
		}
		group.statements.extend_from_slice(statements);
	}

	/// `cache.conditioned`
	pub(crate) fn conditioned(&mut self, cache: &CacheRef, world: P<Node>) {
		let (world_key, anchor) = {
			let mut borrowed = cache.borrow_mut();
			let group = borrowed.current_group();
			if group.condition.is_some() {
				return;
			}
			let world_key = self.factory.new_unique_name("worldKey");
			group.world_key = Some(world_key);
			group.condition = Some(self.factory.new_strict_inequality_expression(world_key, world));
			(world_key, group.anchor)
		};
		let f = &self.factory;
		let declaration = anchored(
			f.new_variable_statement(
				None,
				f.new_variable_declaration_list(
					f.new_node_list(vec![f.new_variable_declaration(
						world_key,
						None,
						Some(self.jecs_type("World", Vec::new())),
						None,
					)]),
					NodeFlags::Let,
				),
			),
			anchor,
		);
		cache.borrow_mut().outer_result(declaration);
		let assignment = anchored(
			f.new_expression_statement(f.new_assignment_expression(world_key, world)),
			anchor,
		);
		self.inner_result(cache, &[assignment]);
	}

	fn to_inner_results(&self, cache: &CacheRef, marker: P<Node>) -> Option<Vec<P<Node>>> {
		let cache = cache.borrow();
		let group = cache.groups.iter().find(|group| group.marker == Some(marker))?;
		if let Some(condition) = group.condition {
			let f = &self.factory;
			return Some(vec![anchored(
				f.new_if_statement(
					condition,
					f.new_block(f.new_node_list(group.statements.clone()), true),
					None,
				),
				group.anchor,
			)]);
		}
		Some(group.statements.clone())
	}

	/// `textRangeOf`
	pub(crate) fn text_range_of(node: P<Node>) -> TextRange {
		let Some(file) = get_source_file_of_node(node) else {
			return TextRange::from_text_pos(node.pos(), node.end());
		};
		TextRange::from_text_pos(
			rotor_tsrs::tscore::TextPos::source(rotor_tsrs::scanner::get_token_pos_of_node(node, file, false)),
			node.end(),
		)
	}

	/// `leadingTrivia`
	pub(crate) fn leading_trivia(node: P<Node>) -> &'static str {
		let Some(file) = get_source_file_of_node(node) else {
			return "";
		};
		let Some(pos) = node.pos().source_offset() else {
			return "";
		};
		let start = rotor_tsrs::scanner::get_token_pos_of_node(node, file, false);
		file.text().get(pos as usize..start as usize).unwrap_or_default()
	}
}

/// `anchored`: gives a synthesized statement the text range of the query it serves, so the
/// emitter does not attribute it to whatever it emitted last.
pub(crate) fn anchored(node: P<Node>, anchor: Option<TextRange>) -> P<Node> {
	if let Some(anchor) = anchor {
		node.set_loc(anchor);
	}
	node
}

/// `nodeText`
pub(crate) fn node_text(node: P<Node>) -> String {
	get_source_file_of_node(node).map_or_else(String::new, |file| {
		rotor_tsrs::scanner::get_source_text_of_node_from_source_file(file, node, false)
	})
}

/// `genericSymbolsAreEqual`: symbols compared by first declaration. Query's members are
/// generic, so the checker hands back a different symbol per instantiation.
pub(crate) fn generic_symbols_are_equal(a: Option<P<Symbol>>, b: Option<P<Symbol>>) -> bool {
	let (Some(a), Some(b)) = (a, b) else {
		return false;
	};
	match (a.declarations().first(), b.declarations().first()) {
		(Some(left), Some(right)) => left == right,
		_ => false,
	}
}

/// Go `Kind.String()`.
pub(crate) fn kind_string(kind: Kind) -> String {
	rotor_tsrs::ast::kind_string(kind)
}
