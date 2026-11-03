//! `transform_class.go`: a planned class's metadata static block and decorator statements.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, ModifierList, Node, NodeFactory, TokenFlags, get_property_name_for_property_name_node,
	get_source_file_of_node, has_static_modifier, is_class_declaration, is_constructor_declaration,
	is_element_access_expression, is_get_accessor_declaration, is_identifier, is_method_declaration,
	is_property_declaration, is_set_accessor_declaration, set_parent_in_children,
};

use crate::plan::FilePlan;
use crate::transform_analysis::{analyze_flamework_class, planned_class_by_name};
use crate::transform_analysis_discovery::has_member_flamework_decorator;
use crate::transform_class_diagnostics::add_class_transform_diagnostic;
use crate::transform_decorators::{
	DecoratorTarget, analyze_node_decorators, is_flamework_decorator, new_decorator_statement,
	new_decorator_statement_for_target, strip_flamework_decorators,
};
use crate::transform_expressions::map_children;
use crate::transform_imports::reflect_identifier_for_node;
use crate::transform_macro_types::{MacroImport, alloc};
use crate::transform_reflection::{MetadataCall, new_metadata_statement, string_array};
use crate::transform_reflection_methods::{
	MemberReflectionInput, constructor_reflection_statements, member_reflection_statements,
};
use crate::transform_state::TransformState;
use crate::{Error, ErrorKind};

/// `transformFlameworkClass`: an invalid decorator becomes a diagnostic and leaves the class.
pub(crate) fn transform_flamework_class(
	state: &mut TransformState<'_>,
	plan: &FilePlan,
	node: P<Node>,
) -> Result<P<Node>, Error> {
	match transform_flamework_class_worker(state, plan, node) {
		Err(error) if add_class_transform_diagnostic(state, node, &error) => Ok(node),
		result => result,
	}
}

#[expect(
	clippy::too_many_lines,
	reason = "Go's transformFlameworkClassWorker, statement for statement"
)]
fn transform_flamework_class_worker(
	state: &mut TransformState<'_>,
	plan: &FilePlan,
	node: P<Node>,
) -> Result<P<Node>, Error> {
	let semantic_class = original_class_node(state, node);
	let analysis = analyze_flamework_class(state, plan, semantic_class)?;
	let class_plan = planned_class_by_name(plan, &analysis.name).unwrap_or_default();
	let mut prerequisites = Vec::new();
	let mut imports: Vec<MacroImport> = Vec::new();
	let mut class_decorator_statements = Vec::with_capacity(analysis.decorators.len());
	for decorator in analysis.decorators.iter().rev() {
		prerequisites.extend_from_slice(&decorator.prerequisites);
		imports.extend_from_slice(&decorator.imports);
		class_decorator_statements.push(new_decorator_statement(state, &analysis.name, decorator)?);
	}
	let mut static_statements = Vec::new();
	let contains_legacy_decorator = analysis.contains_legacy_decorator
		|| !analysis.decorators.is_empty()
		|| has_member_flamework_decorator(state, semantic_class);
	if contains_legacy_decorator || analysis.metadata.requested("identifier") {
		let value = state
			.factory
			.new_string_literal(alloc(&analysis.identifier), TokenFlags::None);
		static_statements.push(new_metadata_statement(
			&state.factory,
			&MetadataCall {
				class_name: analysis.name.clone(),
				key: "identifier".to_owned(),
				value,
			},
		));
	}
	let semantic_members = semantic_class.members();
	if let Some(&constructor) = semantic_members
		.iter()
		.find(|member| is_constructor_declaration(**member))
	{
		static_statements.extend(constructor_reflection_statements(state, &analysis.name, constructor)?);
	}
	if analysis.metadata.requested("flamework:implements") && !analysis.implemented_ids.is_empty() {
		let value = string_array(&state.factory, &analysis.implemented_ids);
		static_statements.push(new_metadata_statement(
			&state.factory,
			&MetadataCall {
				class_name: analysis.name.clone(),
				key: "flamework:implements".to_owned(),
				value,
			},
		));
	}
	add_section_comment(state, &static_statements, &analysis.name, "", "metadata");
	let reflect_identifier = reflect_identifier_for_node(Some(node));
	let class_members = node.members();
	let mut members = Vec::with_capacity(class_members.len() + 1);
	let mut member_decorator_statements = Vec::new();
	for (index, &member) in class_members.iter().enumerate() {
		let semantic_member = semantic_members.get(index).copied().unwrap_or(member);
		let reflection = member_reflection_statements(
			state,
			&MemberReflectionInput {
				plan,
				class_name: analysis.name.clone(),
				member: semantic_member,
			},
		)?;
		let property_name = semantic_member.name().map_or_else(String::new, |name| {
			get_property_name_for_property_name_node(name).into_owned()
		});
		add_section_comment(state, &reflection, &analysis.name, &property_name, "metadata");
		static_statements.extend(reflection);
		let target = DecoratorTarget {
			class_name: analysis.name.clone(),
			plan: class_plan.clone(),
			..DecoratorTarget::default()
		};
		let (updated_member, statements, member_prerequisites, member_imports) =
			transform_flamework_class_member(state, target, member, semantic_member)?;
		add_section_comment(state, &statements, &analysis.name, &property_name, "decorators");
		members.push(updated_member);
		member_decorator_statements.extend(statements);
		prerequisites.extend(member_prerequisites);
		imports.extend(member_imports);
	}
	add_section_comment(state, &class_decorator_statements, &analysis.name, "", "decorators");
	let factory = state.factory.clone();
	let rewritten = rewrite_reflect_statements(&factory, static_statements, &reflect_identifier);
	let static_block =
		factory.new_class_static_block_declaration(None, factory.new_block(factory.new_node_list(rewritten), true));
	members.push(static_block);
	let class = node.as_class_declaration();
	let modifiers = strip_flamework_decorators(state, node.modifiers());
	let updated_class = factory.update_class_declaration(
		node,
		modifiers,
		node.name(),
		class.type_parameters(),
		class.heritage_clauses(),
		factory.new_node_list(members),
	);
	let mut statements = crate::transform_expressions::macro_import_statements(&factory, &imports);
	statements.extend(prerequisites);
	statements.push(updated_class);
	statements.extend(class_decorator_statements);
	statements.extend(member_decorator_statements);
	Ok(factory.new_syntax_list(rotor_tsrs::tscore::alloc_vec(statements)))
}

/// `addSectionComment`: labels a group's first statement (`// (Flamework) Class.member label`).
fn add_section_comment(
	state: &TransformState<'_>,
	statements: &[P<Node>],
	class_name: &str,
	property_name: &str,
	label: &str,
) {
	let Some(&first) = statements.first() else {
		return;
	};
	let element = if property_name.is_empty() {
		class_name.to_owned()
	} else {
		format!("{class_name}.{property_name}")
	};
	state.emit_context.add_synthetic_leading_comment(
		first,
		Kind::SingleLineCommentTrivia,
		&format!(" (Flamework) {element} {label}"),
		true,
	);
}

/// `originalClassNode`: the program's class at the same range, for semantic queries.
fn original_class_node(state: &TransformState<'_>, node: P<Node>) -> P<Node> {
	let Some(source_file) = get_source_file_of_node(node) else {
		return node;
	};
	let Some(original) = state.program.get_source_file(source_file.file_name()) else {
		return node;
	};
	set_parent_in_children(original.as_node());
	find_class_at(original.as_node(), node).unwrap_or(node)
}

fn find_class_at(current: P<Node>, node: P<Node>) -> Option<P<Node>> {
	if is_class_declaration(current) && current.loc() == node.loc() {
		return Some(current);
	}
	current
		.iter_children()
		.into_iter()
		.find_map(|child| find_class_at(child, node))
}

type MemberTransform = (P<Node>, Vec<P<Node>>, Vec<P<Node>>, Vec<MacroImport>);

fn transform_flamework_class_member(
	state: &mut TransformState<'_>,
	mut target: DecoratorTarget,
	member: P<Node>,
	semantic_member: P<Node>,
) -> Result<MemberTransform, Error> {
	let Some(name) = semantic_member.name() else {
		for decorator in semantic_member.decorators() {
			if is_flamework_decorator(state, decorator) {
				return Err(Error::kinded(
					ErrorKind::InvalidDecorator,
					&format!(
						": unsupported decorator placement on {}",
						rotor_tsrs::ast::kind_string(member.kind())
					),
				));
			}
		}
		return Ok((member, Vec::new(), Vec::new(), Vec::new()));
	};
	let decorators = analyze_node_decorators(state, &target.plan, semantic_member)?;
	let stripped: Option<P<ModifierList>> = strip_flamework_decorators(state, semantic_member.modifiers());
	let factory = &state.factory;
	let updated = if is_method_declaration(member) {
		let method = member.as_method_declaration();
		factory.update_method_declaration(
			member,
			stripped,
			method.asterisk_token(),
			method.name(),
			method.postfix_token(),
			method.type_parameters(),
			method.parameters(),
			method.type_(),
			method.full_signature(),
			method.body(),
		)
	} else if is_property_declaration(member) {
		let property = member.as_property_declaration();
		factory.update_property_declaration(
			member,
			stripped,
			property.name(),
			property.postfix_token(),
			property.type_(),
			property.initializer(),
		)
	} else if is_get_accessor_declaration(member) {
		let accessor = member.as_get_accessor_declaration();
		factory.update_get_accessor_declaration(
			member,
			stripped,
			accessor.name(),
			accessor.type_parameters(),
			accessor.parameters(),
			accessor.type_(),
			accessor.full_signature(),
			accessor.body(),
		)
	} else if is_set_accessor_declaration(member) {
		let accessor = member.as_set_accessor_declaration();
		factory.update_set_accessor_declaration(
			member,
			stripped,
			accessor.name(),
			accessor.type_parameters(),
			accessor.parameters(),
			accessor.type_(),
			accessor.full_signature(),
			accessor.body(),
		)
	} else {
		member
	};
	target.property_name = get_property_name_for_property_name_node(name).into_owned();
	target.is_static = has_static_modifier(semantic_member);
	target.member = true;
	let mut statements = Vec::with_capacity(decorators.len());
	let mut prerequisites = Vec::new();
	let mut imports = Vec::new();
	for decorator in decorators.iter().rev() {
		prerequisites.extend_from_slice(&decorator.prerequisites);
		imports.extend_from_slice(&decorator.imports);
		statements.push(new_decorator_statement_for_target(state, &target, decorator)?);
	}
	Ok((updated, statements, prerequisites, imports))
}

/// `rewriteReflectStatements`: `Reflect[...]` accesses renamed to the file's `Reflect` import.
fn rewrite_reflect_statements(factory: &NodeFactory, statements: Vec<P<Node>>, identifier: &str) -> Vec<P<Node>> {
	if identifier == "Reflect" {
		return statements;
	}
	statements
		.into_iter()
		.map(|statement| rewrite_reflect(factory, statement, identifier))
		.collect()
}

fn rewrite_reflect(factory: &NodeFactory, node: P<Node>, identifier: &str) -> P<Node> {
	if is_element_access_expression(node)
		&& let Some(expression) = node.expression()
		&& is_identifier(expression)
		&& expression.text() == "Reflect"
	{
		let access = node.as_element_access_expression();
		return factory.update_element_access_expression(
			node,
			factory.new_identifier(alloc(identifier)),
			access.question_dot_token(),
			access.argument_expression,
			node.flags(),
		);
	}
	map_children(factory, node, |child| Some(rewrite_reflect(factory, child, identifier)))
}
