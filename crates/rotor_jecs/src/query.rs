//! `query.go`: which queries can be cached, and the hoisted cache that replaces them.

use std::collections::HashSet;

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFlags, Symbol, SymbolFlags, TokenFlags, is_call_expression, is_for_of_statement, is_identifier,
	is_if_statement, is_object_literal_expression, is_parameter_declaration, is_private_identifier,
	is_property_access_expression, is_property_assignment, is_statement, is_variable_declaration,
};

use crate::transform::{
	CachedQuery, Fail, State, Statements, anchored, fail, generic_symbols_are_equal, kind_string, node_text,
};

/// `findMatchingChild` keeps the LAST match in a pre-order walk that does not descend into a
/// match, which is what the JavaScript version's `forEachChild` callback (always returning
/// undefined) amounts to.
fn find_matching_child(node: P<Node>, predicate: &mut dyn FnMut(P<Node>) -> bool) -> Option<P<Node>> {
	fn visit(child: P<Node>, predicate: &mut dyn FnMut(P<Node>) -> bool, found: &mut Option<P<Node>>) -> bool {
		if predicate(child) {
			*found = Some(child);
			return false;
		}
		child.for_each_child(&mut |grandchild| visit(grandchild, predicate, found));
		false
	}
	let mut found = None;
	node.for_each_child(&mut |child| visit(child, predicate, &mut found));
	found
}

/// `queryPlan`
pub(crate) struct QueryPlan {
	pub(crate) world: P<Node>,
	pub(crate) required: Vec<P<Node>>,
	pub(crate) excluded: Vec<P<Node>>,
	pub(crate) dont_fragment: bool,
	pub(crate) inline: bool,
}

impl State<'_> {
	pub(crate) fn static_declarations(&mut self, node: P<Node>, can_have_calls: bool) -> Vec<P<Node>> {
		if is_identifier(node) {
			let Some(symbol) = self.ck().get_symbol_at_location(node) else {
				return Vec::new();
			};
			let Some(&first) = symbol.declarations().first() else {
				return Vec::new();
			};
			let mut statement = first;
			while !is_statement(statement) {
				// function parameters are considered dynamic
				if is_parameter_declaration(statement) {
					return Vec::new();
				}
				// if the variable isn't initialized it is considered dynamic
				if is_variable_declaration(statement) && statement.initializer().is_none() {
					return Vec::new();
				}
				let Some(parent) = statement.parent() else {
					return Vec::new();
				};
				statement = parent;
			}
			let parent = statement.parent();
			if parent != self.current_cache_node() && parent.is_none_or(|parent| parent.kind() != Kind::SourceFile) {
				return Vec::new();
			}
			return vec![statement];
		}
		if is_call_expression(node) && can_have_calls {
			let Some(callee) = node.expression() else {
				return Vec::new();
			};
			let mut expression_declarations = self.static_declarations(callee, true);
			if expression_declarations.is_empty() {
				return Vec::new();
			}
			let mut argument_declarations = Vec::new();
			for &argument in node.arguments() {
				let declarations = self.static_declarations(argument, true);
				if declarations.is_empty() {
					return Vec::new();
				}
				argument_declarations.extend(declarations);
			}
			if argument_declarations.is_empty() {
				return Vec::new();
			}
			expression_declarations.extend(argument_declarations);
			return expression_declarations;
		}
		if is_property_access_expression(node) {
			let (Some(name), Some(expression)) = (node.name(), node.expression()) else {
				return Vec::new();
			};
			if is_private_identifier(name) {
				return Vec::new();
			}
			let mut name_declarations = self.static_declarations(name, true);
			if name_declarations.is_empty() {
				return Vec::new();
			}
			let expression_declarations = self.static_declarations(expression, true);
			if expression_declarations.is_empty() {
				return Vec::new();
			}
			name_declarations.extend(expression_declarations);
			return name_declarations;
		}
		Vec::new()
	}

	pub(crate) fn static_ct_to_type_node(&self, node: P<Node>) -> Result<P<Node>, Fail> {
		let f = &self.factory;
		if is_identifier(node) {
			return Ok(f.new_type_query_node(node, None));
		}
		if is_property_access_expression(node) {
			return Ok(f.new_type_query_node(self.generate_qualified_name(node)?, None));
		}
		if is_call_expression(node) {
			let arguments = node.arguments();
			if arguments.len() < 2 {
				return fail(format!(
					"Unsupported query component type: {}",
					kind_string(node.kind())
				));
			}
			let relation = self.static_ct_to_type_node(arguments[0])?;
			let target = self.static_ct_to_type_node(arguments[1])?;
			return Ok(self.jecs_type(
				"Pair",
				vec![
					self.jecs_type("InferComponent", vec![relation]),
					self.jecs_type("InferComponent", vec![target]),
				],
			));
		}
		fail(format!(
			"Unsupported query component type: {}",
			kind_string(node.kind())
		))
	}

	fn generate_qualified_name(&self, node: P<Node>) -> Result<P<Node>, Fail> {
		let (Some(expression), Some(name)) = (node.expression(), node.name()) else {
			return fail("expected an identifier");
		};
		let left = if is_property_access_expression(expression) {
			self.generate_qualified_name(expression)?
		} else {
			if !is_identifier(expression) {
				return fail("expected an identifier");
			}
			expression
		};
		if !is_identifier(name) {
			return fail("expected an identifier");
		}
		Ok(self.factory.new_qualified_name(left, name))
	}

	/// `classifyQuery`: jecs `.with` and `.without` replace their filter lists, so only the
	/// outermost of each counts. `inline` is false when a per-row archetype walk cannot match the
	/// jecs iterator.
	pub(crate) fn classify_query(&mut self, mut expression: P<Node>) -> Option<QueryPlan> {
		let jecs = self.jecs();
		let mut with_terms: &[P<Node>] = &[];
		let mut without_terms: &[P<Node>] = &[];
		let (mut with_seen, mut without_seen) = (false, false);
		while is_call_expression(expression)
			&& let Some(callee) = expression.expression()
			&& is_property_access_expression(callee)
		{
			let symbol = self.ck().get_symbol_at_location(callee);
			if symbol.is_some() && generic_symbols_are_equal(Some(jecs.world_query), symbol) {
				let world = callee.expression()?;
				return Some(self.plan_query(world, expression.arguments(), with_terms, without_terms));
			}
			if symbol.is_some() && generic_symbols_are_equal(Some(jecs.query.with), symbol) {
				if !with_seen {
					(with_terms, with_seen) = (expression.arguments(), true);
				}
			} else if symbol.is_some() && generic_symbols_are_equal(Some(jecs.query.without), symbol) && !without_seen {
				(without_terms, without_seen) = (expression.arguments(), true);
			}
			expression = callee.expression()?;
		}
		None
	}

	fn plan_query(
		&mut self,
		world: P<Node>,
		query_terms: &[P<Node>],
		with_terms: &[P<Node>],
		without_terms: &[P<Node>],
	) -> QueryPlan {
		let mut filter = |terms: &[P<Node>], keep: bool| -> Vec<P<Node>> {
			terms
				.iter()
				.copied()
				.filter(|&term| self.is_dont_fragment(term) == keep)
				.collect()
		};
		let required = filter(with_terms, true);
		let excluded = filter(without_terms, true);
		let queried_dont_fragment = filter(query_terms, true);
		let mut fragmenting_terms = query_terms.to_vec();
		fragmenting_terms.extend_from_slice(with_terms);
		let fragmenting = filter(&fragmenting_terms, false);
		let mut inline = !fragmenting.is_empty();
		if inline {
			let mut all_terms = fragmenting_terms;
			all_terms.extend_from_slice(without_terms);
			for term in all_terms {
				if self.has_wildcard_relation(term) {
					inline = false;
					break;
				}
			}
		}
		if inline {
			for &term in &queried_dont_fragment {
				if self.has_wildcard_target(term) {
					inline = false;
					break;
				}
			}
		}
		QueryPlan {
			world,
			dont_fragment: required.len() + excluded.len() + queried_dont_fragment.len() > 0,
			required,
			excluded,
			inline,
		}
	}

	fn has_wildcard_relation(&mut self, term: P<Node>) -> bool {
		let wildcard = self.jecs().wildcard;
		is_call_expression(term)
			&& term
				.arguments()
				.first()
				.is_some_and(|&relation| self.resolves_to(relation, wildcard))
	}

	fn has_wildcard_target(&mut self, term: P<Node>) -> bool {
		let wildcard = self.jecs().wildcard;
		is_call_expression(term)
			&& term
				.arguments()
				.get(1)
				.is_some_and(|&target| self.resolves_to(target, wildcard))
	}

	fn resolved_symbol(&mut self, node: P<Node>) -> Option<P<Symbol>> {
		let symbol = self.ck().get_symbol_at_location(node)?;
		if symbol.flags().intersects(SymbolFlags::Alias) {
			return Some(self.ck().get_aliased_symbol(symbol));
		}
		Some(symbol)
	}

	fn resolves_to(&mut self, node: P<Node>, target: P<Symbol>) -> bool {
		let symbol = self.resolved_symbol(node);
		symbol.is_some() && generic_symbols_are_equal(symbol, Some(target))
	}

	/// `isDontFragment`: a component registered with `{ dontFragment: true }`, an alias of one, or
	/// a pair over one.
	pub(crate) fn is_dont_fragment(&mut self, term: P<Node>) -> bool {
		// a pair takes DontFragment from its relation
		if is_call_expression(term) {
			return term
				.arguments()
				.first()
				.is_some_and(|&relation| self.is_dont_fragment(relation));
		}
		let Some(symbol) = self.resolved_symbol(term) else {
			return false;
		};
		let Some(declaration) = symbol.value_declaration() else {
			return false;
		};
		if !is_variable_declaration(declaration) {
			return false;
		}
		let Some(initializer) = declaration.initializer() else {
			return false;
		};
		if is_identifier(initializer) || is_property_access_expression(initializer) {
			return initializer != term && self.is_dont_fragment(initializer);
		}
		if !is_call_expression(initializer) {
			return false;
		}
		for &argument in initializer.arguments() {
			if !is_object_literal_expression(argument) {
				continue;
			}
			for &property in argument.as_object_literal_expression().properties.nodes() {
				if is_property_assignment(property)
					&& let Some(name) = property.name()
					&& is_identifier(name)
					&& name.text() == "dontFragment"
					&& property
						.initializer()
						.is_some_and(|initializer| initializer.kind() == Kind::TrueKeyword)
				{
					return true;
				}
			}
		}
		false
	}

	pub(crate) fn stored_initializer(&mut self, identifier: P<Node>) -> P<Node> {
		let Some(symbol) = self.ck().get_symbol_at_location(identifier) else {
			return identifier;
		};
		let Some(declaration) = symbol.value_declaration() else {
			return identifier;
		};
		if !is_variable_declaration(declaration) {
			return identifier;
		}
		declaration.initializer().unwrap_or(identifier)
	}

	/// `assertNoDontFragment`: the archetype walk skips `DontFragment` probes for this query, so a
	/// term the static scan missed would silently match nothing; jecs flags it as `df`.
	pub(crate) fn assert_no_dont_fragment(&mut self, query: P<Node>, query_expression: P<Node>) -> P<Node> {
		let source = if is_identifier(query_expression) {
			self.stored_initializer(query_expression)
		} else {
			query_expression
		};
		let f = &self.factory;
		let internals = f.new_as_expression(
			f.new_as_expression(query, f.new_keyword_type_node(Kind::UnknownKeyword)),
			self.jecs_type("QueryInternals", Vec::new()),
		);
		let message = format!(
			"query has a DontFragment term the transformer did not detect: {}",
			node_text(source).split_whitespace().collect::<Vec<_>>().join(" ")
		);
		f.new_expression_statement(f.new_call_expression(
			f.new_identifier("assert"),
			None,
			None,
			f.new_node_list(vec![
				f.new_strict_equality_expression(
					f.new_property_access_expression(internals, None, f.new_identifier("df"), NodeFlags::None),
					f.new_identifier("undefined"),
				),
				f.new_string_literal(rotor_tsrs::tscore::alloc_str(&message), TokenFlags::None),
			]),
			NodeFlags::None,
		))
	}

	fn is_query_creation(&mut self, node: P<Node>) -> bool {
		let world_query = self.jecs().world_query;
		is_call_expression(node)
			&& node
				.expression()
				.is_some_and(|callee| self.ck().get_symbol_at_location(callee) == Some(world_query))
	}

	/// `cacheQuery`: the identifier replacing a cacheable query, or `None` to leave the call to
	/// the ordinary visit.
	#[expect(clippy::too_many_lines, reason = "one function in Go")]
	pub(crate) fn cache_query(&mut self, expression: P<Node>) -> Result<Option<P<Node>>, Fail> {
		let jecs = self.jecs();
		if State::leading_trivia(expression).contains("no-cache") {
			return Ok(None);
		}
		let signature = self.ck().get_resolved_signature(expression);
		let return_type = self.ck().get_return_type_of_signature(signature);
		if return_type.alias().and_then(|alias| alias.symbol()) != Some(jecs.query.symbol) {
			return Ok(None);
		}
		let query_creation = if self.is_query_creation(expression) {
			Some(expression)
		} else {
			find_matching_child(expression, &mut |node| self.is_query_creation(node))
		};
		let Some(query_creation) = query_creation else {
			return Ok(None);
		};
		let mut query_expression = query_creation;
		while let Some(parent) = query_expression.parent()
			&& is_property_access_expression(parent)
			&& let Some(grandparent) = parent.parent()
			&& is_call_expression(grandparent)
		{
			let symbol = self.ck().get_symbol_at_location(parent);
			// if the query is manually cached, do not cache it again
			if symbol.is_some() && generic_symbols_are_equal(symbol, Some(jecs.query.cached)) {
				return Ok(None);
			}
			query_expression = grandparent;
		}
		let mut statement = query_expression;
		while !is_statement(statement) {
			let Some(parent) = statement.parent() else {
				return Ok(None);
			};
			statement = parent;
		}
		if !is_for_of_statement(statement) && !is_if_statement(statement) {
			return Ok(None);
		}
		let Some(plan) = self.classify_query(query_expression) else {
			return Ok(None);
		};
		if !plan.inline {
			return Ok(None);
		}
		let (component_declarations, query_components) = match self.parse_query(query_expression) {
			Ok(parsed) => parsed,
			Err(fault) => {
				if !self.config.silent {
					(self.warn)(&format!(
						"'{}' isn't simple. Not caching query: \n\t{}",
						node_text(fault),
						node_text(expression)
					));
				}
				return Ok(None);
			}
		};
		let Some(world) = query_creation.expression() else {
			return Ok(None);
		};
		if !is_property_access_expression(world) {
			(self.warn)(&format!(
				"Unsupported world.query access type: {}",
				kind_string(world.kind())
			));
			return Ok(None);
		}
		let Some(world) = world.expression() else {
			return Ok(None);
		};
		let world_declarations = self.static_declarations(world, true);
		let file_level = world_declarations.iter().any(|declaration| {
			declaration
				.parent()
				.is_some_and(|parent| parent.kind() == Kind::SourceFile)
		});
		let cache = if file_level {
			self.file_cache(world)?
		} else {
			self.current_cache(world)?
		};
		cache.borrow_mut().require(&world_declarations);
		cache.borrow_mut().require(&component_declarations);

		let f = self.factory.clone();
		let query_identifier = f.new_unique_name("query");
		let archetypes_identifier = f.new_unique_name("archetypes");
		self.cached_queries.insert(
			query_identifier,
			CachedQuery {
				archetypes: archetypes_identifier,
				components: query_components.clone(),
			},
		);
		let expression_cached = f.new_call_expression(
			f.new_property_access_expression(expression, None, f.new_identifier("cached"), NodeFlags::None),
			None,
			None,
			f.new_node_list(Vec::new()),
			NodeFlags::None,
		);
		let query_archetypes = f.new_call_expression(
			f.new_property_access_expression(query_identifier, None, f.new_identifier("archetypes"), NodeFlags::None),
			None,
			None,
			f.new_node_list(Vec::new()),
			NodeFlags::None,
		);
		let query_range = Some(State::text_range_of(query_creation));
		if world_declarations.is_empty() {
			let mut component_types = Vec::with_capacity(query_components.len());
			for &component in &query_components {
				component_types.push(self.static_ct_to_type_node(component)?);
			}
			let query_declaration = f.new_variable_declaration(
				query_identifier,
				None,
				Some(self.jecs_type(
					"CachedQuery",
					vec![f.new_tuple_type_node(f.new_node_list(component_types))],
				)),
				None,
			);
			let archetypes_declaration = f.new_variable_declaration(
				archetypes_identifier,
				None,
				Some(f.new_type_reference_node(
					f.new_identifier("ReturnType"),
					Some(f.new_node_list(vec![f.new_indexed_access_type_node(
						f.new_type_query_node(query_identifier, None),
						f.new_literal_type_node(f.new_string_literal("archetypes", TokenFlags::None)),
					)])),
				)),
				None,
			);
			cache.borrow_mut().outer_result(anchored(
				f.new_variable_statement(
					None,
					f.new_variable_declaration_list(
						f.new_node_list(vec![query_declaration, archetypes_declaration]),
						NodeFlags::Let,
					),
				),
				query_range,
			));
			cache.borrow_mut().new_group(query_range);
			self.conditioned(&cache, world);
			self.inner_result(
				&cache,
				&[
					anchored(
						f.new_expression_statement(f.new_assignment_expression(query_identifier, expression_cached)),
						query_range,
					),
					anchored(
						f.new_expression_statement(
							f.new_assignment_expression(archetypes_identifier, query_archetypes),
						),
						query_range,
					),
				],
			);
			if !plan.dont_fragment && is_for_of_statement(statement) {
				let assertion = self.assert_no_dont_fragment(query_identifier, query_expression);
				self.inner_result(&cache, &[anchored(assertion, query_range)]);
			}
		} else {
			let query_declaration = f.new_variable_declaration(query_identifier, None, None, Some(expression_cached));
			let archetypes_declaration =
				f.new_variable_declaration(archetypes_identifier, None, None, Some(query_archetypes));
			cache.borrow_mut().outer_result(anchored(
				f.new_variable_statement(
					None,
					f.new_variable_declaration_list(
						f.new_node_list(vec![query_declaration, archetypes_declaration]),
						NodeFlags::Const,
					),
				),
				query_range,
			));
			if !plan.dont_fragment && is_for_of_statement(statement) {
				let assertion = self.assert_no_dont_fragment(query_identifier, query_expression);
				cache.borrow_mut().outer_result(anchored(assertion, query_range));
			}
		}
		Ok(Some(query_identifier))
	}

	/// `parseQuery`: the declarations and the queried components, or the last term that is not
	/// static.
	fn parse_query(&mut self, expression: P<Node>) -> Result<(Vec<P<Node>>, Statements), P<Node>> {
		let jecs = self.jecs();
		let symbols = [jecs.world_query, jecs.query.with, jecs.query.without];
		let mut fault = None;
		let mut declarations = Vec::new();
		let mut query_components = Vec::new();
		let mut stack = vec![expression];
		// Preorder, as `visit` recurses through `ForEachChild`.
		while let Some(node) = stack.pop() {
			if is_call_expression(node)
				&& let Some(callee) = node.expression()
			{
				let symbol = self.ck().get_symbol_at_location(callee);
				let container = symbol.is_some()
					&& symbols
						.iter()
						.any(|&candidate| generic_symbols_are_equal(Some(candidate), symbol));
				if container {
					for &component in node.arguments() {
						let found = self.static_declarations(component, true);
						if found.is_empty() {
							fault = Some(component);
						}
						declarations.extend(found);
						if symbol == Some(jecs.world_query) {
							query_components.push(component);
						}
					}
				}
			}
			let mut children = Vec::new();
			node.for_each_child(&mut |child| {
				children.push(child);
				false
			});
			stack.extend(children.into_iter().rev());
		}
		fault.map_or(Ok((declarations, query_components)), Err)
	}
}

/// The node sets `findQueryComponents` has visited.
pub(crate) type Visited = HashSet<P<Node>>;
