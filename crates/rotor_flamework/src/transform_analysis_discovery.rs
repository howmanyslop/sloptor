//! `transform_analysis_discovery.go`: finding a file's Flamework classes.

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, SourceFile, SymbolFlags, get_source_file_of_node, is_call_expression, is_class_declaration,
	is_constructor_declaration,
};
use rotor_tsrs::checker::Type;

use crate::Error;
use crate::ErrorKind;
use crate::plan::{ClassPlan, DecoratorPlan};
use crate::project_discovery::{find_package_json, package_name};
use crate::transform::preload_class_identifier;
use crate::transform_analysis::DiscoveredFlameworkClass;
use crate::transform_analysis_build::{add_package_build_class, root_build_class_by_internal_id};
use crate::transform_class_diagnostics_constraints::collect_node_metadata;
use crate::transform_decorators::decorator_reference_name;
use crate::transform_state::TransformState;
use crate::transform_symbols::{
	declaration_from_node, declaration_full_name, node_uid, package_relative_module, path_within, trim_extension,
	type_uid,
};

/// `discoveredClassState`
pub(crate) struct DiscoveredClassState {
	pub(crate) class: DiscoveredFlameworkClass,
	pub(crate) plan: ClassPlan,
	pub(crate) dependencies: Vec<String>,
	pub(crate) node: P<Node>,
}

/// `discoverSourceClasses`
pub(crate) fn discover_source_classes(
	state: &mut TransformState<'_>,
	file: P<SourceFile>,
) -> Result<Vec<DiscoveredClassState>, Error> {
	// Cheap reject: Flamework classes require a class declaration in the source text.
	if !file.text().contains("class") {
		return Ok(Vec::new());
	}
	let mut nodes = Vec::new();
	collect_class_declarations(file.as_node(), &mut nodes);
	let mut classes = Vec::with_capacity(nodes.len());
	for node in nodes {
		if let Some(class) = discover_class(state, node)? {
			classes.push(class);
		}
	}
	Ok(classes)
}

fn collect_class_declarations(node: P<Node>, classes: &mut Vec<P<Node>>) {
	if is_class_declaration(node) {
		classes.push(node);
	}
	for child in node.iter_children() {
		collect_class_declarations(child, classes);
	}
}

fn discover_class(state: &mut TransformState<'_>, node: P<Node>) -> Result<Option<DiscoveredClassState>, Error> {
	if node.name().is_none() {
		return Ok(None);
	}
	let internal_id = declaration_internal_id(state, node)?;
	let metadata = collect_node_metadata(state, node);
	let mut decorators = discover_decorators(state, node)?;
	let contains_legacy_decorator = !decorators.is_empty() || has_member_flamework_decorator(state, node);
	let build_class = root_build_class_by_internal_id(state.project, &internal_id);
	if !contains_legacy_decorator && !metadata.requested("reflect") && build_class.is_none() {
		return Ok(None);
	}
	if decorators.is_empty()
		&& let Some(build_class) = &build_class
	{
		decorators = build_class
			.decorators
			.iter()
			.map(|decorator| DecoratorPlan {
				name: decorator.name.clone(),
				internal_id: decorator.internal_id.clone(),
			})
			.collect();
	}
	let (type_ids, dependencies) = constructor_type_ids(state, node)?;
	let plan = ClassPlan {
		internal_id: internal_id.clone(),
		decorators: decorators.clone(),
		contains_legacy_decorator,
		..ClassPlan::default()
	};
	let file_name = get_source_file_of_node(node).map_or_else(String::new, |file| file.file_name().to_owned());
	preload_class_identifier(state, &plan, &file_name)?;
	if !state.project.is_game && build_class.is_none() {
		add_package_build_class(state, &plan, &file_name)?;
	}
	Ok(Some(DiscoveredClassState {
		class: DiscoveredFlameworkClass {
			internal_id,
			decorator_ids: decorators
				.iter()
				.map(|decorator| decorator.internal_id.clone())
				.collect(),
			constructor_type_ids: type_ids,
		},
		plan,
		dependencies,
		node,
	}))
}

fn discover_decorators(state: &mut TransformState<'_>, node: P<Node>) -> Result<Vec<DecoratorPlan>, Error> {
	let mut decorators = Vec::new();
	for decorator in node.decorators() {
		let Some(expression) = decorator.expression() else {
			continue;
		};
		let identifier = if is_call_expression(expression) {
			expression.expression().unwrap_or(expression)
		} else {
			expression
		};
		let Some(name) = decorator_reference_name(identifier) else {
			continue;
		};
		if !is_flamework_decorator_type(state, expression) {
			continue;
		}
		let Some(declaration) = declaration_from_node(state, identifier) else {
			continue;
		};
		let internal_id = declaration_internal_id(state, declaration)?;
		decorators.push(DecoratorPlan { name, internal_id });
	}
	Ok(decorators)
}

/// `hasMemberFlameworkDecorator`
pub(crate) fn has_member_flamework_decorator(state: &mut TransformState<'_>, node: P<Node>) -> bool {
	for &member in node.members() {
		for decorator in member.decorators() {
			if decorator
				.expression()
				.is_some_and(|expression| is_flamework_decorator_type(state, expression))
			{
				return true;
			}
		}
	}
	false
}

fn is_flamework_decorator_type(state: &mut TransformState<'_>, node: P<Node>) -> bool {
	let node_type = state.ck().get_type_at_location(node);
	state
		.ck()
		.get_properties_of_type(node_type)
		.iter()
		.any(|property| property.name() == "_flamework_Decorator")
}

type ConstructorTypeIds = (Vec<String>, Vec<String>);

fn constructor_type_ids(state: &mut TransformState<'_>, node: P<Node>) -> Result<ConstructorTypeIds, Error> {
	for &member in node.members() {
		if !is_constructor_declaration(member) {
			continue;
		}
		let parameters = member.parameters();
		let mut ids = Vec::with_capacity(parameters.len());
		let mut dependencies = Vec::with_capacity(parameters.len());
		for &parameter in parameters {
			let mut target_type = state.ck().get_type_at_location(parameter);
			let id = match parameter.type_node() {
				None => type_uid(state, target_type, parameter)?,
				Some(target) => {
					let id = node_uid(state, target)?;
					target_type = state.ck().get_type_at_location(target);
					id
				}
			};
			ids.push(id);
			if let Some(declaration) = declaration_from_type(state, target_type) {
				dependencies.push(declaration_internal_id(state, declaration)?);
			}
		}
		return Ok((ids, dependencies));
	}
	Ok((Vec::new(), Vec::new()))
}

fn declaration_from_type(state: &mut TransformState<'_>, target_type: P<Type>) -> Option<P<Node>> {
	let mut symbol = target_type.symbol()?;
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	symbol
		.value_declaration()
		.or_else(|| symbol.declarations().first().copied())
}

/// `declarationInternalID`: `package:out/path@Name` for a project or package declaration.
pub(crate) fn declaration_internal_id(state: &TransformState<'_>, declaration: P<Node>) -> Result<String, Error> {
	let name = declaration_full_name(declaration);
	let file_name = get_source_file_of_node(declaration).map_or_else(String::new, |file| file.file_name().to_owned());
	if path_within(&state.project.translator.root_dir, &file_name) {
		let output = state.project.translator.get_output_path(&file_name);
		let relative = gopath::rel(&state.project.root_directory, &output)
			.map_err(|error| Error::msg(format!("resolve Flamework declaration path: {error}")))?;
		let relative = trim_extension(&gopath::to_slash(&relative), &relative);
		return Ok(format!("{}:{relative}@{name}", state.project.package_name));
	}
	let package_json = find_package_json(&gopath::dir(&file_name));
	let data = std::fs::read(&package_json)
		.map_err(|error| Error::io("open", &package_json, &error).wrap("read Flamework declaration package"))?;
	let Some(package) = package_name(&data).filter(|package| !package.is_empty()) else {
		return Err(Error::kinded(ErrorKind::InvalidPackage, ": declaration package"));
	};
	let relative = package_relative_module(&package_json, &file_name)?;
	Ok(format!("{package}:{relative}@{name}"))
}
