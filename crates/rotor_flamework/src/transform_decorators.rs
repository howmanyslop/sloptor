//! `transform_decorators.go`: Flamework decorators and their `Reflect.decorate` statements.

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, ModifierList, Node, NodeFactory, NodeFlags, TokenFlags, get_source_file_of_node, is_call_expression,
	is_class_declaration, is_decorator, is_element_access_expression, is_identifier, is_new_expression,
	is_object_literal_expression, is_property_access_expression, is_string_literal,
};

use crate::metadata::Metadata;
use crate::plan::{ClassPlan, DecoratorPlan};
use crate::transform_analysis::{
	ClassDecoratorAnalysis, IdentifierTransformInput, identifier_for_transform, local_internal_id,
};
use crate::transform_decorators_imports::decorator_runtime_imports;
use crate::transform_expressions::{
	ExpressionTransformResult, default_flamework_macro_runtime, transform_flamework_expression_children_with_runtime,
	transform_flamework_expression_with_runtime,
};
use crate::transform_imports::reflect_identifier_for_node;
use crate::transform_macro_types::{MacroImport, alloc};
use crate::transform_macros::transform_flamework_call;
use crate::transform_reflection_metadata::{merge_metadata, metadata_for_decorator, metadata_for_node};
use crate::transform_state::TransformState;
use crate::transform_update::update_flamework_component_config;
use crate::{Error, ErrorKind};

/// `decoratorTarget`
#[derive(Clone, Debug, Default)]
pub(crate) struct DecoratorTarget {
	pub(crate) class_name: String,
	pub(crate) property_name: String,
	pub(crate) plan: ClassPlan,
	pub(crate) is_static: bool,
	pub(crate) member: bool,
}

/// `analyzeClassDecorators`
pub(crate) fn analyze_class_decorators(
	state: &mut TransformState<'_>,
	plan: &ClassPlan,
	node: P<Node>,
) -> Result<(Vec<ClassDecoratorAnalysis>, Metadata), Error> {
	let decorators = analyze_node_decorators(state, plan, node)?;
	let mut metadata = metadata_for_node(state, node);
	for decorator in &decorators {
		let decorator_metadata = metadata_for_decorator(state, decorator.expression);
		metadata = merge_metadata(&metadata, &decorator_metadata);
	}
	Ok((decorators, metadata))
}

/// `analyzeNodeDecorators`: the node's Flamework decorators, their arguments transformed last
/// to first.
pub(crate) fn analyze_node_decorators(
	state: &mut TransformState<'_>,
	plan: &ClassPlan,
	node: P<Node>,
) -> Result<Vec<ClassDecoratorAnalysis>, Error> {
	let mut decorators = Vec::new();
	let Some(modifiers) = node.modifiers() else {
		return Ok(decorators);
	};
	for &modifier in modifiers.nodes() {
		if !is_decorator(modifier) || !is_flamework_decorator(state, modifier) {
			continue;
		}
		let Some(expression) = modifier.expression() else {
			continue;
		};
		let mut identifier = expression;
		let mut arguments = Vec::new();
		if is_call_expression(expression)
			&& let Some(callee) = expression.expression()
		{
			identifier = callee;
			arguments.extend_from_slice(expression.arguments());
		}
		let Some(name) = decorator_reference_name(identifier) else {
			return Err(Error::kinded(
				ErrorKind::InvalidDecorator,
				&format!(": decorator expression on {}", plan.internal_id),
			));
		};
		let decorator_plan = planned_decorator_by_name(plan, &name).unwrap_or_else(|| DecoratorPlan {
			internal_id: local_internal_id(&plan.internal_id, &name),
			name: name.clone(),
		});
		decorators.push(ClassDecoratorAnalysis {
			name,
			internal_id: decorator_plan.internal_id,
			expression: identifier,
			arguments,
			prerequisites: Vec::new(),
			imports: Vec::new(),
		});
	}
	for index in (0..decorators.len()).rev() {
		let (arguments, prerequisites, imports) =
			transform_decorator_config(state, node, decorators[index].expression, &decorators[index].arguments)?;
		let decorator = &mut decorators[index];
		decorator.arguments = arguments;
		decorator.prerequisites = prerequisites;
		decorator.imports = imports;
	}
	Ok(decorators)
}

/// `decoratorReferenceName`
pub(crate) fn decorator_reference_name(node: P<Node>) -> Option<String> {
	if is_identifier(node) {
		return Some(node.text().to_owned());
	}
	if is_property_access_expression(node) {
		return node.name().map(|name| name.text().to_owned());
	}
	if is_element_access_expression(node) {
		let argument = node.as_element_access_expression().argument_expression;
		if is_string_literal(argument) {
			return Some(argument.text().to_owned());
		}
	}
	None
}

type DecoratorConfig = (Vec<P<Node>>, Vec<P<Node>>, Vec<MacroImport>);

fn transform_decorator_config(
	state: &mut TransformState<'_>,
	declaration: P<Node>,
	identifier: P<Node>,
	arguments: &[P<Node>],
) -> Result<DecoratorConfig, Error> {
	let file = get_source_file_of_node(declaration);
	if metadata_for_decorator(state, decorator_symbol_node(identifier)).requested("intrinsic-component-decorator") {
		if !is_class_declaration(declaration) {
			let placement = declaration.name().map_or_else(
				|| rotor_tsrs::ast::kind_string(declaration.kind()),
				|name| name.text().to_owned(),
			);
			return Err(Error::kinded(
				ErrorKind::InvalidDecorator,
				&format!(": component decorator requires a class declaration, got {placement}"),
			));
		}
		if arguments.len() > 1 || (arguments.len() == 1 && !is_object_literal_expression(arguments[0])) {
			return Err(Error::kinded(
				ErrorKind::InvalidDecorator,
				": component decorator config must be an object literal",
			));
		}
		let base = match arguments.first() {
			Some(first) => *first,
			None => state
				.factory
				.new_object_literal_expression(state.factory.new_node_list(Vec::new()), true),
		};
		let mut transformed_properties = Vec::with_capacity(base.properties().len());
		let mut prerequisites = Vec::new();
		let mut imports = Vec::new();
		let runtime = default_flamework_macro_runtime();
		for (index, property) in base.properties().iter().enumerate() {
			let updated = transform_flamework_expression_children_with_runtime(state, *property, &runtime)
				.map_err(|error| error.wrap(&format!("transform component decorator property {index}")))?;
			transformed_properties.push(updated.expression);
			prerequisites.extend(updated.prerequisites);
			imports.extend(updated.imports);
		}
		let properties = update_flamework_component_config(state, declaration, transformed_properties)
			.map_err(|error| error.wrap("transform component decorator config"))?;
		let transformed =
			vec![
				state
					.factory
					.update_object_literal_expression(base, state.factory.new_node_list(properties), true),
			];
		let imports = decorator_runtime_imports(state, file, &transformed, &prerequisites, imports);
		return Ok((transformed, prerequisites, imports));
	}
	let mut transformed = Vec::with_capacity(arguments.len());
	let mut prerequisites = Vec::new();
	let mut imports = Vec::new();
	for (index, argument) in arguments.iter().enumerate() {
		let updated = transform_decorator_argument(state, *argument)
			.map_err(|error| error.wrap(&format!("transform decorator argument {index}")))?;
		transformed.push(updated.expression);
		prerequisites.extend(updated.prerequisites);
		imports.extend(updated.imports);
	}
	let imports = decorator_runtime_imports(state, file, &transformed, &prerequisites, imports);
	Ok((transformed, prerequisites, imports))
}

fn transform_decorator_argument(
	state: &mut TransformState<'_>,
	argument: P<Node>,
) -> Result<ExpressionTransformResult, Error> {
	if decorator_argument_already_transformed(argument) {
		return Ok(ExpressionTransformResult::empty(argument));
	}
	let runtime = default_flamework_macro_runtime();
	if is_call_expression(argument) || is_new_expression(argument) {
		// tsrs always resolves a signature, so Go's "no signature" fallback never applies.
		let result = transform_flamework_call(state, argument, &runtime)?;
		if result.expression != Some(argument) || !result.prerequisites.is_empty() || !result.imports.is_empty() {
			return Ok(ExpressionTransformResult {
				expression: result.expression.unwrap_or(argument),
				prerequisites: result.prerequisites,
				imports: result.imports,
			});
		}
	}
	transform_flamework_expression_with_runtime(state, argument, &runtime)
}

fn decorator_argument_already_transformed(argument: P<Node>) -> bool {
	argument
		.iter_children()
		.into_iter()
		.any(|child| child.pos().is_synthetic() || decorator_argument_already_transformed(child))
}

fn decorator_symbol_node(identifier: P<Node>) -> P<Node> {
	if is_property_access_expression(identifier) {
		return identifier.name().unwrap_or(identifier);
	}
	if is_element_access_expression(identifier) {
		return identifier.as_element_access_expression().argument_expression;
	}
	identifier
}

/// `isFlameworkDecorator`: whether the decorator's type has `_flamework_Decorator`.
pub(crate) fn is_flamework_decorator(state: &mut TransformState<'_>, decorator: P<Node>) -> bool {
	let Some(expression) = decorator.expression() else {
		return false;
	};
	let decorator_type = state.ck().get_type_at_location(expression);
	state
		.ck()
		.get_property_of_type(decorator_type, "_flamework_Decorator")
		.is_some()
}

fn planned_decorator_by_name(plan: &ClassPlan, name: &str) -> Option<DecoratorPlan> {
	plan.decorators.iter().find(|decorator| decorator.name == name).cloned()
}

/// `stripFlameworkDecorators`
pub(crate) fn strip_flamework_decorators(
	state: &mut TransformState<'_>,
	modifiers: Option<P<ModifierList>>,
) -> Option<P<ModifierList>> {
	let modifiers = modifiers?;
	let mut filtered = Vec::with_capacity(modifiers.nodes().len());
	for &modifier in modifiers.nodes() {
		if is_decorator(modifier) && is_flamework_decorator(state, modifier) {
			continue;
		}
		filtered.push(modifier);
	}
	Some(state.factory.new_modifier_list(filtered))
}

/// `newDecoratorStatement`
pub(crate) fn new_decorator_statement(
	state: &mut TransformState<'_>,
	class_name: &str,
	decorator: &ClassDecoratorAnalysis,
) -> Result<P<Node>, Error> {
	new_decorator_statement_for_target(
		state,
		&DecoratorTarget {
			class_name: class_name.to_owned(),
			..DecoratorTarget::default()
		},
		decorator,
	)
}

/// `newDecoratorStatementForTarget`: `Reflect.decorate(Class, "id", decorator, [args][, "member", static])`.
pub(crate) fn new_decorator_statement_for_target(
	state: &mut TransformState<'_>,
	target: &DecoratorTarget,
	decorator: &ClassDecoratorAnalysis,
) -> Result<P<Node>, Error> {
	let identifier = identifier_for_transform(
		state,
		&IdentifierTransformInput {
			internal_id: decorator.internal_id.clone(),
			name: decorator.name.clone(),
			node: decorator.expression,
		},
	)?;
	let factory = &state.factory;
	let arguments = factory.new_array_literal_expression(factory.new_node_list(decorator.arguments.clone()), false);
	let mut call_arguments = vec![
		factory.new_identifier(alloc(&target.class_name)),
		factory.new_string_literal(alloc(&identifier), TokenFlags::None),
		decorator.expression,
		arguments,
	];
	if target.member {
		call_arguments.push(factory.new_string_literal(alloc(&target.property_name), TokenFlags::None));
		let keyword = if target.is_static {
			Kind::TrueKeyword
		} else {
			Kind::FalseKeyword
		};
		call_arguments.push(factory.new_keyword_expression(keyword));
	}
	Ok(new_reflect_call_with_identifier(
		factory,
		&reflect_identifier_for_node(Some(decorator.expression)),
		"decorate",
		call_arguments,
	))
}

/// `newReflectCallWithIdentifier`: `identifier["method"](arguments...)` as a statement.
pub(crate) fn new_reflect_call_with_identifier(
	factory: &NodeFactory,
	identifier: &str,
	method: &str,
	arguments: Vec<P<Node>>,
) -> P<Node> {
	let access = factory.new_element_access_expression(
		factory.new_identifier(alloc(identifier)),
		None,
		factory.new_string_literal(alloc(method), TokenFlags::None),
		NodeFlags::None,
	);
	let call = factory.new_call_expression(access, None, None, factory.new_node_list(arguments), NodeFlags::None);
	factory.new_expression_statement(call)
}
