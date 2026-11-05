//! `transform_analysis.go`: class discovery over the transform's files, and a class's analysis.

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, get_source_file_of_node, is_class_declaration};

use crate::metadata::Metadata;
use crate::plan::{ClassPlan, FileAnalysis, FilePlan};
use crate::state::DeclarationIdentity;
use crate::transform::lua_file_name;
use crate::transform_analysis_discovery::discover_source_classes;
use crate::transform_class_diagnostics::{DependencyClass, validate_dependency_cycles};
use crate::transform_class_diagnostics_constraints::{constraint_diagnostics, order_class_diagnostics};
use crate::transform_decorators::analyze_class_decorators;
use crate::transform_diagnostics::Diagnostic;
use crate::transform_macro_types::MacroImport;
use crate::transform_reflection::{analyze_constructor_parameters, analyze_implemented_types};
use crate::transform_state::{TransformInput, TransformState, new_transform_state_base};
use crate::transform_symbols::{declaration_from_node, declaration_full_name, declaration_uid};
use crate::{Error, ErrorKind};

/// `classTransformAnalysis`
pub(crate) struct ClassTransformAnalysis {
	pub(crate) name: String,
	#[expect(dead_code, reason = "Go's classTransformAnalysis keeps it; nothing reads it")]
	pub(crate) internal_id: String,
	pub(crate) identifier: String,
	pub(crate) metadata: Metadata,
	pub(crate) decorators: Vec<ClassDecoratorAnalysis>,
	#[expect(dead_code, reason = "Go's classTransformAnalysis keeps it; nothing reads it")]
	pub(crate) parameter_ids: Vec<String>,
	pub(crate) implemented_ids: Vec<String>,
	pub(crate) contains_legacy_decorator: bool,
}

/// `classDecoratorAnalysis`
#[derive(Clone, Debug)]
pub(crate) struct ClassDecoratorAnalysis {
	pub(crate) name: String,
	pub(crate) internal_id: String,
	pub(crate) expression: P<Node>,
	pub(crate) arguments: Vec<P<Node>>,
	pub(crate) prerequisites: Vec<P<Node>>,
	pub(crate) imports: Vec<MacroImport>,
}

/// `identifierTransformInput`
pub(crate) struct IdentifierTransformInput {
	pub(crate) internal_id: String,
	pub(crate) name: String,
	pub(crate) node: P<Node>,
}

/// `FlameworkClassAnalysis`
#[derive(Clone, Debug, Default)]
pub struct FlameworkClassAnalysis {
	files: Vec<FileAnalysis>,
	classes: Vec<DiscoveredFlameworkClass>,
	diagnostics: Vec<Diagnostic>,
}

/// `DiscoveredFlameworkClass`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveredFlameworkClass {
	/// The class's internal ID.
	pub internal_id: String,
	/// Its decorators' internal IDs.
	pub decorator_ids: Vec<String>,
	/// Its constructor parameters' type IDs.
	pub constructor_type_ids: Vec<String>,
}

impl FlameworkClassAnalysis {
	/// `Files`
	#[must_use]
	pub fn files(&self) -> Vec<FileAnalysis> {
		self.files.clone()
	}

	/// `Classes`
	#[must_use]
	pub fn classes(&self) -> Vec<DiscoveredFlameworkClass> {
		self.classes.clone()
	}

	/// `Diagnostics`
	#[must_use]
	pub fn diagnostics(&self) -> Vec<Diagnostic> {
		self.diagnostics.clone()
	}
}

/// `AnalyzeFlameworkClasses`
///
/// # Errors
///
/// Discovery failed, or the classes' constructor dependencies form a cycle.
pub fn analyze_flamework_classes(input: &mut TransformInput<'_>) -> Result<FlameworkClassAnalysis, Error> {
	let mut state = new_transform_state_base(
		input.program,
		input.checker,
		input.project,
		input.macro_runtime.as_ref(),
		&[],
	);
	analyze_flamework_classes_with(&mut state, &input.files, true)
}

/// `analyzeFlameworkClasses`, over a state the caller built.
pub(crate) fn analyze_flamework_classes_with(
	state: &mut TransformState<'_>,
	files: &[P<rotor_tsrs::ast::SourceFile>],
	include_native_safety_diagnostics: bool,
) -> Result<FlameworkClassAnalysis, Error> {
	let mut result = FlameworkClassAnalysis::default();
	let mut dependencies = Vec::new();
	for &file in files {
		let classes = discover_source_classes(state, file).map_err(|error| {
			error.wrap(&format!(
				"discover Flamework classes in {}",
				rotor_support::gostr::quote(file.file_name())
			))
		})?;
		if classes.is_empty() {
			continue;
		}
		let file_id = gopath::rel(&state.project.root_directory, file.file_name())
			.map_err(|error| Error::msg(format!("resolve Flamework source path: {error}")))?;
		let mut analysis = FileAnalysis {
			file_id: gopath::to_slash(&file_id).into_owned(),
			classes: Vec::with_capacity(classes.len()),
			..FileAnalysis::default()
		};
		for class in &classes {
			analysis.classes.push(class.plan.clone());
			result.classes.push(class.class.clone());
			if include_native_safety_diagnostics {
				let diagnostics = constraint_diagnostics(state, class);
				result.diagnostics.extend(diagnostics);
			}
			dependencies.push(DependencyClass {
				internal_id: class.class.internal_id.clone(),
				dependencies: class.dependencies.clone(),
			});
		}
		result.files.push(analysis);
	}
	if include_native_safety_diagnostics {
		validate_dependency_cycles(&dependencies)?;
	}
	result.diagnostics = order_class_diagnostics(&result.diagnostics);
	Ok(result)
}

/// `analyzeFlameworkClass`
pub(crate) fn analyze_flamework_class(
	state: &mut TransformState<'_>,
	plan: &FilePlan,
	node: P<Node>,
) -> Result<ClassTransformAnalysis, Error> {
	let Some(name_node) = node.name().filter(|_| is_class_declaration(node)) else {
		return Err(Error::kinded(
			ErrorKind::InvalidClassDeclaration,
			": expected named class declaration",
		));
	};
	let name = name_node.text().to_owned();
	let Some(class_plan) = planned_class_by_name(plan, &name) else {
		return Err(Error::kinded(ErrorKind::UnplannedClass, &format!(": {name}")));
	};
	let identifier = identifier_for_transform(
		state,
		&IdentifierTransformInput {
			internal_id: class_plan.internal_id.clone(),
			name: name.clone(),
			node,
		},
	)?;
	let (decorators, metadata) = analyze_class_decorators(state, &class_plan, node)?;
	let parameter_ids = analyze_constructor_parameters(state, plan, node)?;
	let implemented_ids = analyze_implemented_types(state, plan, node)?;
	Ok(ClassTransformAnalysis {
		name,
		internal_id: class_plan.internal_id.clone(),
		identifier,
		metadata,
		decorators,
		parameter_ids,
		implemented_ids,
		contains_legacy_decorator: class_plan.contains_legacy_decorator,
	})
}

/// `plannedClassByName`
pub(crate) fn planned_class_by_name(plan: &FilePlan, name: &str) -> Option<ClassPlan> {
	plan.classes
		.iter()
		.find(|class| crate::transform::class_name_from_internal_id(&class.internal_id) == name)
		.cloned()
}

/// `identifierForTransform`: the UID of the file that declares the symbol (the reference
/// `getNodeUid`); the input's internal ID only for nodes with no resolvable declaration.
pub(crate) fn identifier_for_transform(
	state: &mut TransformState<'_>,
	input: &IdentifierTransformInput,
) -> Result<String, Error> {
	if let Some(declaration) = declaration_from_node(state, input.node)
		&& !declaration_full_name(declaration).is_empty()
	{
		return declaration_uid(state, declaration);
	}
	let file_name = get_source_file_of_node(input.node).map_or_else(String::new, |file| file.file_name().to_owned());
	state
		.identifier(&DeclarationIdentity {
			internal_id: input.internal_id.clone(),
			declaration_name: input.name.clone(),
			lua_file_name: lua_file_name(&file_name),
			..DeclarationIdentity::default()
		})
		.map_err(|error| {
			error.wrap(&format!(
				"generate Flamework identifier {}",
				rotor_support::gostr::quote(&input.internal_id)
			))
		})
}

/// `localInternalID`: `classInternalID` with its declaration name replaced.
pub(crate) fn local_internal_id(class_internal_id: &str, declaration_name: &str) -> String {
	class_internal_id.rfind('@').map_or_else(
		|| class_internal_id.to_owned(),
		|at| format!("{}{declaration_name}", &class_internal_id[..=at]),
	)
}
