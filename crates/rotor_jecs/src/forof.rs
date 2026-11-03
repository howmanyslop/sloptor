//! `forof.go`: `for (const [entity, ...] of query)` inlined into archetype loops.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFlags, SymbolFlags, TokenFlags, get_source_file_of_node, is_array_binding_pattern,
	is_binding_element, is_break_statement, is_call_expression, is_identifier, is_iteration_statement,
	is_omitted_expression, is_property_access_expression, is_switch_statement, is_variable_declaration,
	is_variable_declaration_list,
};

use crate::query::{QueryPlan, Visited};
use crate::transform::{CachedQuery, Fail, State, Statements, generic_symbols_are_equal};

/// jecs `ECS_ENTITY_T_LO`: an entity's low 24 bits key its sparse-pool slot.
const ENTITY_KEY_MODULUS: &str = "16777216";
/// Each sparse-pool map slot is a u32.
const MAP_SLOT_BYTES: &str = "4";

/// `componentBinding`
#[derive(Clone, Copy)]
struct ComponentBinding {
	identifier: P<Node>,
	component: P<Node>,
}

/// `componentRow`
struct ComponentRow {
	component: Option<P<Node>>,
	list: Option<P<Node>>,
	statement: P<Node>,
}

/// `sparseProbes`: `require`, `exclude` and `read` are its methods; `hoist` appends to `prelude`.
struct SparseProbes {
	key_declaration: P<Node>,
	records: P<Node>,
	key: P<Node>,
}

impl State<'_> {
	fn variable_statement(&self, name: P<Node>, initializer: P<Node>, flags: NodeFlags) -> P<Node> {
		let f = &self.factory;
		f.new_variable_statement(
			None,
			f.new_variable_declaration_list(
				f.new_node_list(vec![f.new_variable_declaration(name, None, None, Some(initializer))]),
				flags,
			),
		)
	}

	fn binary(&self, left: P<Node>, operator: Kind, right: P<Node>) -> P<Node> {
		let f = &self.factory;
		f.new_binary_expression(None, left, None, f.new_token(operator), right)
	}

	fn numeric(&self, text: &'static str) -> P<Node> {
		self.factory.new_numeric_literal(text, TokenFlags::None)
	}

	fn access(&self, expression: P<Node>, name: &'static str) -> P<Node> {
		let f = &self.factory;
		f.new_property_access_expression(expression, None, f.new_identifier(name), NodeFlags::None)
	}

	fn call(&self, expression: P<Node>, arguments: Vec<P<Node>>) -> P<Node> {
		let f = &self.factory;
		f.new_call_expression(expression, None, None, f.new_node_list(arguments), NodeFlags::None)
	}

	/// `rowIndex` is `row - 1`: roblox-ts adds 1 to an index, so this cancels it.
	fn row_index(&self, row: P<Node>) -> P<Node> {
		self.binary(row, Kind::MinusToken, self.numeric("1"))
	}

	/// `hoist`
	fn hoist(&self, prelude: &mut Vec<P<Node>>, name: &str, initializer: P<Node>) -> P<Node> {
		let identifier = self.factory.new_unique_name(name);
		prelude.push(self.variable_statement(identifier, initializer, NodeFlags::Const));
		identifier
	}

	#[expect(clippy::too_many_lines, reason = "one function in Go")]
	pub(crate) fn inline_query_iterators(&mut self, node: P<Node>) -> Result<Option<P<Node>>, Fail> {
		let jecs = self.jecs();
		let loop_statement = node.as_for_in_or_of_statement();
		let initializer = loop_statement.initializer;
		let loop_expression = loop_statement.expression;
		if !is_variable_declaration_list(initializer) {
			return Ok(None);
		}
		let Some(&declaration) = initializer.as_variable_declaration_list().declarations.nodes().first() else {
			return Ok(None);
		};
		let Some(binding) = declaration.name() else {
			return Ok(None);
		};
		if !is_array_binding_pattern(binding) {
			return Ok(None);
		}
		if is_call_expression(loop_expression) {
			let signature = self.ck().get_resolved_signature(loop_expression);
			let return_type = self.ck().get_return_type_of_signature(signature);
			let Some(symbol) = return_type.alias().and_then(|alias| alias.symbol()) else {
				return Ok(None);
			};
			if !generic_symbols_are_equal(Some(jecs.query.symbol), Some(symbol))
				&& !generic_symbols_are_equal(Some(jecs.cached_query), Some(symbol))
			{
				return Ok(None);
			}
		}
		// a stored query may come from another scope, so its world is not ours to probe
		let stored = is_identifier(loop_expression);
		let query_expression = if stored {
			self.stored_initializer(loop_expression)
		} else {
			loop_expression
		};
		let Some(plan) = self.classify_query(query_expression) else {
			return Ok(None);
		};
		if !plan.inline || (stored && plan.dont_fragment) {
			return Ok(None);
		}
		let elements = binding.as_binding_pattern().elements.nodes();
		let Some((&entity, component_identifiers)) = elements.split_first() else {
			return Ok(None);
		};
		if plan.dont_fragment && self.shadows_buffer(node, entity) {
			return Ok(None);
		}
		let expression = self.transform_expression(loop_expression)?;
		let cached: Option<CachedQuery> = if is_identifier(expression) {
			self.cached_queries.get(&expression).cloned()
		} else {
			None
		};
		if cached.is_none() && !is_call_expression(expression) && !is_identifier(expression) {
			return Ok(None);
		}
		let components = match &cached {
			Some(cached) => cached.components.clone(),
			None => match self.find_query_components(expression, &mut Visited::new()) {
				Some(found) => found,
				None => return Ok(None),
			},
		};

		let f = self.factory.clone();
		let mut prelude: Vec<P<Node>> = Vec::new();
		let mut archetypes = cached.as_ref().map(|cached| cached.archetypes);
		if archetypes.is_none() {
			let query = if is_identifier(expression) {
				expression
			} else {
				self.hoist(&mut prelude, "query", expression)
			};
			let mut walk = self.call(self.access(query, "archetypes"), Vec::new());
			if !plan.dont_fragment {
				walk = self.hoist(&mut prelude, "archetypes", walk);
				let assertion = self.assert_no_dont_fragment(query, query_expression);
				prelude.push(assertion);
			}
			archetypes = Some(walk);
		}
		let archetypes = archetypes.unwrap_or_else(|| unreachable!("set above"));
		let archetype = f.new_unique_name("archetype");
		let entities = f.new_unique_name("entities");
		let field = f.new_unique_name("field");
		let row = f.new_unique_name("row");
		let declaration_flags = if initializer.flags().intersects(NodeFlags::Let) {
			NodeFlags::Let
		} else {
			NodeFlags::Const
		};
		let entity_name = self.entity_binding(entity, plan.dont_fragment);
		let entity_declaration = entity_name.map(|entity_name| {
			self.variable_statement(
				entity_name,
				f.new_non_null_expression(
					f.new_element_access_expression(entities, None, self.row_index(row), NodeFlags::None),
					NodeFlags::None,
				),
				declaration_flags,
			)
		});
		let probes = match (plan.dont_fragment, entity_name) {
			(true, Some(entity_name)) => Some(self.new_sparse_probes(plan.world, &mut prelude, entity_name)),
			_ => None,
		};
		let bound_count = component_identifiers.len().min(components.len());
		let bindings: Vec<ComponentBinding> = (0..bound_count)
			.map(|index| ComponentBinding {
				identifier: component_identifiers[index],
				component: components[index],
			})
			.collect();
		let mut guards = Vec::new();
		let mut sparse_values = Vec::new();
		if let Some(probes) = &probes {
			let unbound = components.get(component_identifiers.len()..).unwrap_or_default();
			(guards, sparse_values) =
				self.sparse_row_checks(&plan, probes, &mut prelude, &bindings, unbound, declaration_flags)?;
		}
		let mut rows = Vec::new();
		for binding in &bindings {
			if is_omitted(binding.identifier) || (probes.is_some() && self.is_dont_fragment(binding.component)) {
				continue;
			}
			let Some(name) = binding.identifier.name() else {
				continue;
			};
			let list = f.new_generated_name_for_node(binding.component);
			let value = f.new_non_null_expression(
				f.new_element_access_expression(list, None, self.row_index(row), NodeFlags::None),
				NodeFlags::None,
			);
			rows.push(ComponentRow {
				component: Some(binding.component),
				list: Some(list),
				statement: self.variable_statement(name, value, declaration_flags),
			});
		}
		if component_identifiers.len() > components.len() {
			for &identifier in &component_identifiers[components.len()..] {
				if is_omitted(identifier) {
					continue;
				}
				let Some(name) = identifier.name() else {
					continue;
				};
				rows.push(ComponentRow {
					component: None,
					list: None,
					statement: self.variable_statement(name, f.new_identifier("undefined"), declaration_flags),
				});
			}
		}

		let mut broken_variable: Option<P<Node>> = None;
		let statement = self.fix_breaks(loop_statement.statement, &mut broken_variable);
		let mut row_bindings: Vec<P<Node>> = rows.iter().map(|row| row.statement).collect();
		row_bindings.extend(sparse_values);
		let body = self.transform(statement)?;
		let inner = self.inner_loop(row, entities, entity_declaration, guards, row_bindings, body);
		let result = self.outer_loop(entities, archetype, archetypes, field, &rows, inner, broken_variable);
		if !prelude.is_empty() {
			prelude.push(result);
			return Ok(Some(f.new_block(f.new_node_list(prelude), true)));
		}
		Ok(Some(result))
	}

	/// The break visitor of `inlineQueryIterators`, over the children of `node`.
	fn fix_breaks(&self, node: P<Node>, broken_variable: &mut Option<P<Node>>) -> P<Node> {
		let emit_context = self.emit_context;
		crate::transform::visit_each_child(emit_context, node, |child| self.fix_break(child, broken_variable))
	}

	fn fix_break(&self, node: P<Node>, broken_variable: &mut Option<P<Node>>) -> P<Node> {
		// breaks will target them
		if is_iteration_statement(node, true) || is_switch_statement(node) {
			return node;
		}
		if is_break_statement(node) {
			let f = &self.factory;
			let broken = *broken_variable.get_or_insert_with(|| f.new_unique_name("broken"));
			return f.new_block(
				f.new_node_list(vec![
					f.new_expression_statement(f.new_assignment_expression(broken, f.new_true_expression())),
					f.new_break_statement(None),
				]),
				false,
			);
		}
		self.fix_breaks(node, broken_variable)
	}

	/// `entityBinding`: the probes key on the entity, so a `DontFragment` loop binds one even when
	/// the pattern omits it.
	fn entity_binding(&self, entity: P<Node>, dont_fragment: bool) -> Option<P<Node>> {
		if !is_omitted(entity) {
			return entity.name();
		}
		if dont_fragment {
			return Some(self.factory.new_unique_name("entity"));
		}
		None
	}

	/// `sparseRowChecks`: per-row skips for every `DontFragment` term, plus value bindings for the
	/// bound ones.
	fn sparse_row_checks(
		&mut self,
		plan: &QueryPlan,
		probes: &SparseProbes,
		prelude: &mut Vec<P<Node>>,
		bindings: &[ComponentBinding],
		unbound_components: &[P<Node>],
		declaration_flags: NodeFlags,
	) -> Result<(Statements, Statements), Fail> {
		let mut guards = vec![probes.key_declaration];
		for &term in &plan.required {
			guards.push(self.probe_require(probes, prelude, term));
		}
		for &term in &plan.excluded {
			guards.push(self.probe_exclude(probes, prelude, term));
		}
		let mut sparse_values = Vec::new();
		for binding in bindings {
			if !self.is_dont_fragment(binding.component) {
				continue;
			}
			if is_omitted(binding.identifier) {
				guards.push(self.probe_require(probes, prelude, binding.component));
				continue;
			}
			let (slot_guards, value) = self.probe_read(probes, prelude, binding.component)?;
			guards.extend(slot_guards);
			if let Some(name) = binding.identifier.name() {
				sparse_values.push(self.variable_statement(name, value, declaration_flags));
			}
		}
		for &component in unbound_components {
			if self.is_dont_fragment(component) {
				guards.push(self.probe_require(probes, prelude, component));
			}
		}
		Ok((guards, sparse_values))
	}

	/// `shadowsBuffer`: the probes read the `buffer` global, which a user binding of that name
	/// would shadow.
	fn shadows_buffer(&mut self, node: P<Node>, entity: P<Node>) -> bool {
		if !is_omitted(entity)
			&& let Some(name) = entity.name()
			&& is_identifier(name)
			&& name.text() == "buffer"
		{
			return true;
		}
		for symbol in self.ck().get_symbols_in_scope(node, SymbolFlags::Value) {
			if symbol.name() != "buffer" {
				continue;
			}
			for &declaration in symbol.declarations() {
				if let Some(file) = get_source_file_of_node(declaration)
					&& !file.is_declaration_file()
				{
					return true;
				}
			}
		}
		false
	}

	/// `newSparseProbes` mirrors jecs `pool_find`; every row re-reads `map` and `map_max`, since
	/// an insert can reallocate them. A term has no record until first use, and a pair record dies
	/// with its target.
	fn new_sparse_probes(&self, world: P<Node>, prelude: &mut Vec<P<Node>>, entity: P<Node>) -> SparseProbes {
		let f = &self.factory;
		let world_internals = f.new_as_expression(
			f.new_as_expression(world, f.new_keyword_type_node(Kind::UnknownKeyword)),
			self.jecs_type("WorldInternals", Vec::new()),
		);
		let records = self.hoist(prelude, "records", self.access(world_internals, "component_index"));
		let key = f.new_unique_name("key");
		SparseProbes {
			key_declaration: self.variable_statement(
				key,
				self.binary(entity, Kind::PercentToken, self.numeric(ENTITY_KEY_MODULUS)),
				NodeFlags::Const,
			),
			records,
			key,
		}
	}

	fn probe_pool(&self, probes: &SparseProbes, prelude: &mut Vec<P<Node>>, term: P<Node>) -> P<Node> {
		let f = &self.factory;
		let record = self.call(self.access(probes.records, "get"), vec![term]);
		self.hoist(
			prelude,
			"pool",
			f.new_property_access_expression(
				record,
				Some(f.new_token(Kind::QuestionDotToken)),
				f.new_identifier("sparse"),
				NodeFlags::OptionalChain,
			),
		)
	}

	fn probe_present(&self, probes: &SparseProbes, target: P<Node>) -> P<Node> {
		let f = &self.factory;
		self.binary(
			self.binary(
				target,
				Kind::ExclamationEqualsEqualsToken,
				f.new_identifier("undefined"),
			),
			Kind::AmpersandAmpersandToken,
			self.binary(probes.key, Kind::LessThanEqualsToken, self.access(target, "map_max")),
		)
	}

	fn probe_slot(&self, probes: &SparseProbes, target: P<Node>) -> P<Node> {
		let f = &self.factory;
		self.call(
			self.access(f.new_identifier("buffer"), "readu32"),
			vec![
				self.access(target, "map"),
				self.binary(probes.key, Kind::AsteriskToken, self.numeric(MAP_SLOT_BYTES)),
			],
		)
	}

	fn skip_row(&self, condition: P<Node>) -> P<Node> {
		let f = &self.factory;
		f.new_if_statement(condition, f.new_continue_statement(None), None)
	}

	fn probe_require(&self, probes: &SparseProbes, prelude: &mut Vec<P<Node>>, term: P<Node>) -> P<Node> {
		let f = &self.factory;
		let target = self.probe_pool(probes, prelude, term);
		self.skip_row(self.binary(
			f.new_prefix_unary_expression(
				Kind::ExclamationToken,
				f.new_parenthesized_expression(self.probe_present(probes, target)),
			),
			Kind::BarBarToken,
			self.binary(
				self.probe_slot(probes, target),
				Kind::EqualsEqualsEqualsToken,
				self.numeric("0"),
			),
		))
	}

	fn probe_exclude(&self, probes: &SparseProbes, prelude: &mut Vec<P<Node>>, term: P<Node>) -> P<Node> {
		let target = self.probe_pool(probes, prelude, term);
		self.skip_row(self.binary(
			self.probe_present(probes, target),
			Kind::AmpersandAmpersandToken,
			self.binary(
				self.probe_slot(probes, target),
				Kind::ExclamationEqualsEqualsToken,
				self.numeric("0"),
			),
		))
	}

	fn probe_read(
		&self,
		probes: &SparseProbes,
		prelude: &mut Vec<P<Node>>,
		term: P<Node>,
	) -> Result<(Statements, P<Node>), Fail> {
		let f = &self.factory;
		let target = self.probe_pool(probes, prelude, term);
		let index = f.new_unique_name("slot");
		let value = f.new_as_expression(
			self.call(
				self.access(
					self.access(f.new_non_null_expression(target, NodeFlags::None), "values"),
					"get",
				),
				vec![index],
			),
			self.jecs_type("InferComponent", vec![self.static_ct_to_type_node(term)?]),
		);
		Ok((
			vec![
				self.variable_statement(
					index,
					f.new_conditional_expression(
						self.probe_present(probes, target),
						f.new_token(Kind::QuestionToken),
						self.probe_slot(probes, target),
						f.new_token(Kind::ColonToken),
						self.numeric("0"),
					),
					NodeFlags::Const,
				),
				self.skip_row(self.binary(index, Kind::EqualsEqualsEqualsToken, self.numeric("0"))),
			],
			value,
		))
	}

	#[expect(clippy::too_many_arguments, reason = "Go's signature")]
	fn outer_loop(
		&self,
		entities: P<Node>,
		archetype: P<Node>,
		archetypes: P<Node>,
		field: P<Node>,
		rows: &[ComponentRow],
		inner: P<Node>,
		broken_variable: Option<P<Node>>,
	) -> P<Node> {
		let f = &self.factory;
		let mut body = vec![f.new_variable_statement(
			None,
			f.new_variable_declaration_list(
				f.new_node_list(vec![f.new_variable_declaration(
					entities,
					None,
					None,
					Some(self.access(archetype, "entities")),
				)]),
				NodeFlags::Const,
			),
		)];
		if !rows.is_empty() {
			let mut declarations =
				vec![f.new_variable_declaration(field, None, None, Some(self.access(archetype, "columns_map")))];
			for row in rows {
				let (Some(component), Some(list)) = (row.component, row.list) else {
					continue;
				};
				declarations.push(f.new_variable_declaration(
					list,
					None,
					None,
					Some(f.new_non_null_expression(
						f.new_element_access_expression(field, None, component, NodeFlags::None),
						NodeFlags::None,
					)),
				));
			}
			body.push(f.new_variable_statement(
				None,
				f.new_variable_declaration_list(f.new_node_list(declarations), NodeFlags::Const),
			));
		}
		if let Some(broken) = broken_variable {
			body.push(f.new_variable_statement(
				None,
				f.new_variable_declaration_list(
					f.new_node_list(vec![f.new_variable_declaration(
						broken,
						None,
						None,
						Some(f.new_false_expression()),
					)]),
					NodeFlags::Let,
				),
			));
		}
		body.push(inner);
		if let Some(broken) = broken_variable {
			body.push(f.new_if_statement(broken, f.new_break_statement(None), None));
		}
		let initializer = f.new_variable_declaration_list(
			f.new_node_list(vec![f.new_variable_declaration(archetype, None, None, None)]),
			NodeFlags::Const,
		);
		f.new_for_in_or_of_statement(
			Kind::ForOfStatement,
			None,
			initializer,
			archetypes,
			f.new_block(f.new_node_list(body), true),
		)
	}

	fn inner_loop(
		&self,
		row: P<Node>,
		entities: P<Node>,
		entity_declaration: Option<P<Node>>,
		guards: Vec<P<Node>>,
		bindings: Vec<P<Node>>,
		statement: P<Node>,
	) -> P<Node> {
		let f = &self.factory;
		let row_declaration = f.new_variable_declaration_list(
			f.new_node_list(vec![f.new_variable_declaration(row, None, None, None)]),
			NodeFlags::Const,
		);
		// iterate in reverse to prevent iterator invalidation
		let range_expression = self.call(
			f.new_identifier("$range"),
			vec![
				self.call(self.access(entities, "size"), Vec::new()),
				self.numeric("1"),
				f.new_prefix_unary_expression(Kind::MinusToken, self.numeric("1")),
			],
		);
		let mut body = Vec::with_capacity(guards.len() + bindings.len() + 2);
		body.extend(entity_declaration);
		body.extend(guards);
		body.extend(bindings);
		body.push(statement);
		f.new_for_in_or_of_statement(
			Kind::ForOfStatement,
			None,
			row_declaration,
			range_expression,
			f.new_block(f.new_node_list(body), true),
		)
	}

	/// `findQueryComponents` follows identifiers to their declarations, unlike `parseQuery`,
	/// since a for-of over a stored query has no components on its own expression. `None` is
	/// where the JavaScript version returns undefined.
	fn find_query_components(&mut self, expression: P<Node>, visited: &mut Visited) -> Option<Vec<P<Node>>> {
		if !visited.insert(expression) {
			return None;
		}
		let jecs = self.jecs();
		if is_call_expression(expression) {
			let callee = expression.expression()?;
			if is_property_access_expression(callee) {
				let symbol = self.ck().get_symbol_at_location(callee);
				if symbol.is_some()
					&& (generic_symbols_are_equal(Some(jecs.query.with), symbol)
						|| generic_symbols_are_equal(Some(jecs.query.without), symbol)
						|| generic_symbols_are_equal(Some(jecs.query.cached), symbol))
				{
					return self.find_query_components(callee.expression()?, visited);
				}
			}
			let symbol = self.ck().get_symbol_at_location(callee);
			if symbol.is_some() && generic_symbols_are_equal(Some(jecs.world_query), symbol) {
				let mut components = Vec::new();
				for &component in expression.arguments() {
					if self.static_declarations(component, true).is_empty() {
						return None;
					}
					components.push(component);
				}
				return Some(components);
			}
		} else if is_identifier(expression) {
			let symbol = self.ck().get_symbol_at_location(expression)?;
			let declaration = symbol.value_declaration()?;
			if !is_variable_declaration(declaration) {
				return None;
			}
			return self.find_query_components(declaration.initializer()?, visited);
		}
		None
	}
}

/// `isOmitted` is `ts.isOmittedExpression` for an array binding element: tsgo parses a hole in
/// `[, a]` as a `BindingElement` without a name instead.
fn is_omitted(element: P<Node>) -> bool {
	is_omitted_expression(element) || (is_binding_element(element) && element.name().is_none())
}
