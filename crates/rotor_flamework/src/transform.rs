//! `transform.go`: the transform's entry point, per-file driver and Flamework surface filters.

use std::collections::{HashMap, HashSet, VecDeque};

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, SourceFile, get_source_file_of_node, is_class_declaration, is_element_access_expression, is_external_module,
	is_identifier, is_property_access_expression, set_parent_in_children, skip_parentheses,
};

use crate::Error;
use crate::metadata::Metadata;
use crate::plan::{ClassPlan, FileAnalysis, FilePlan, normalize_file_id};
use crate::state::DeclarationIdentity;
use crate::transform_analysis::analyze_flamework_classes_with;
use crate::transform_analysis_discovery::declaration_internal_id;
use crate::transform_class::transform_flamework_class;
use crate::transform_expressions::{map_children, transform_flamework_expressions_in_source_file_with_runtime};
use crate::transform_imports::prepend_flamework_reflect_import;
use crate::transform_parallel::{parallel_transform_eligible, transform_files_parallel};
use crate::transform_source_imports::deduplicate_source_imports;
use crate::transform_state::{
	SourceMetadata, TransformInput, TransformResult, TransformState, new_transform_state, print_source,
};

/// `flameworkModulePrefix`: every published Flamework package.
const FLAMEWORK_MODULE_PREFIX: &str = "@flamework/";
/// `flameworkMarkerPrefix`: the prefix of every marker property the transforms key on.
pub(crate) const FLAMEWORK_MARKER_PREFIX: &str = "_flamework";
/// `flameworkMetadataTag`: marks macro and intrinsic declarations without a marker property.
const FLAMEWORK_METADATA_TAG: &str = "@metadata";

/// `flameworkMacroCalleeNames`: distinctive Flamework API and macro names.
const FLAMEWORK_MACRO_CALLEE_NAMES: [&str; 10] = [
	"Flamework",
	"Modding",
	"Reflect",
	"Networking",
	"createGuard",
	"createEvent",
	"createServer",
	"createClient",
	"inspect",
	"declarationUid",
];

/// `Transform`: serial project analysis, then each source file in caller order.
///
/// # Errors
///
/// Discovery, analysis or a file's transform failed.
pub fn transform(input: TransformInput<'_>) -> Result<TransformResult, Error> {
	let parallel = parallel_transform_eligible(&input);
	let TransformInput {
		program,
		checker,
		files,
		project,
		analyses,
		macro_runtime,
		mut print,
	} = input;
	let mut state = new_transform_state(program, checker, project, macro_runtime.as_ref(), &[]);
	let discovered = analyze_flamework_classes_with(&mut state, &files, false)
		.map_err(|error| error.wrap("discover Flamework sources"))?;
	let analyses = merge_transform_analyses(&analyses, &discovered.files())?;
	let plans = state
		.project
		.analyze(&analyses)
		.map_err(|error| error.wrap("analyze Flamework sources"))?;
	preload_transform_identifiers(&mut state, &plans)?;
	state.plans.clone_from(&plans);
	for diagnostic in discovered.diagnostics() {
		state.add_diagnostic(diagnostic);
	}

	if parallel && transform_files_parallel() {
		return Err(Error::msg("flamework: parallel transform is unavailable"));
	}
	let runtime = state.macro_runtime();
	let mut transformed_files = Vec::with_capacity(files.len());
	let mut sources = Vec::with_capacity(files.len());
	for &source_file in &files {
		let transformed = transform_source_file(&mut state, source_file, &runtime).map_err(|error| {
			error.wrap(&format!(
				"transform Flamework source {}",
				rotor_support::gostr::quote(source_file.file_name())
			))
		})?;
		transformed_files.push(transformed);
		let metadata = print_source(
			&mut print,
			SourceMetadata::new(source_file, transformed, state.emit_context),
		)?;
		sources.push(metadata);
	}
	Ok(TransformResult {
		files: transformed_files,
		plans,
		diagnostics: state.ordered_diagnostics(),
		sources,
	})
}

fn preload_transform_identifiers(state: &mut TransformState<'_>, plans: &[FilePlan]) -> Result<(), Error> {
	for plan in plans {
		for class in &plan.classes {
			preload_class_identifier(state, class, plan.file_id())?;
		}
	}
	Ok(())
}

/// `preloadClassIdentifier`: the class's identifier, and its project decorators'.
pub(crate) fn preload_class_identifier(
	state: &mut TransformState<'_>,
	class: &ClassPlan,
	file_name: &str,
) -> Result<(), Error> {
	let name = class_name_from_internal_id(&class.internal_id);
	if name.is_empty() {
		return Ok(());
	}
	state
		.identifier(&DeclarationIdentity {
			internal_id: class.internal_id.clone(),
			declaration_name: name,
			lua_file_name: lua_file_name(file_name),
			..DeclarationIdentity::default()
		})
		.map_err(|error| {
			error.wrap(&format!(
				"preload Flamework identifier {}",
				rotor_support::gostr::quote(&class.internal_id)
			))
		})?;
	let package_prefix = format!("{}:", state.project.package_name);
	for decorator in &class.decorators {
		if !decorator.internal_id.starts_with(&package_prefix) {
			continue;
		}
		state
			.identifier(&DeclarationIdentity {
				internal_id: decorator.internal_id.clone(),
				declaration_name: decorator.name.clone(),
				lua_file_name: lua_file_name(file_name),
				..DeclarationIdentity::default()
			})
			.map_err(|error| {
				error.wrap(&format!(
					"preload Flamework decorator identifier {}",
					rotor_support::gostr::quote(&decorator.internal_id)
				))
			})?;
	}
	Ok(())
}

/// `transformSourceFile`
pub(crate) fn transform_source_file(
	state: &mut TransformState<'_>,
	source_file: P<SourceFile>,
	runtime: &crate::transform_macro_types::MacroRuntime,
) -> Result<P<SourceFile>, Error> {
	let plan = state.plan_for_file(source_file)?;
	// Most sources have no Flamework classes, macros or APIs; skip the expression visitor
	// when no surface exists.
	let needs_expressions = plan.is_some() || source_needs_flamework_expression_transform(state, source_file);
	if plan.is_none() && !needs_expressions {
		return Ok(source_file);
	}
	let expression_transformed = if needs_expressions {
		transform_flamework_expressions_in_source_file_with_runtime(state, source_file, runtime)
			.map_err(|error| error.wrap("transform Flamework expressions"))?
	} else {
		source_file
	};
	let Some(plan) = plan else {
		// Nothing below reads parents here, which lets parallel workers visit unplanned files.
		if expression_transformed == source_file {
			return Ok(source_file);
		}
		let transformed = deduplicate_source_imports(&state.factory, source_file, expression_transformed);
		if transformed == source_file {
			return Ok(source_file);
		}
		return Ok(reparse(state, transformed));
	};
	set_parent_in_children(expression_transformed.as_node());
	let mut class_error: Option<Error> = None;
	let mut class_transformed = false;
	let visited = visit_planned_classes(
		state,
		&plan,
		expression_transformed.as_node(),
		&mut class_error,
		&mut class_transformed,
	);
	set_parent_in_children(source_file.as_node());
	if let Some(error) = class_error {
		return Err(error.wrap("transform Flamework class"));
	}
	let mut visited = visited.as_source_file_p();
	if class_transformed {
		visited = prepend_flamework_reflect_import(&state.factory, visited);
	}
	let transformed = deduplicate_source_imports(&state.factory, source_file, visited);
	if transformed == source_file {
		return Ok(source_file);
	}
	Ok(reparse(state, transformed))
}

fn reparse(state: &TransformState<'_>, file: P<SourceFile>) -> P<SourceFile> {
	state
		.factory
		.deep_clone_reparse(Some(file.as_node()))
		.map_or(file, |node| node.as_source_file_p())
}

fn visit_planned_classes(
	state: &mut TransformState<'_>,
	plan: &FilePlan,
	node: P<Node>,
	class_error: &mut Option<Error>,
	class_transformed: &mut bool,
) -> P<Node> {
	let factory = state.factory.clone();
	let visited = map_children(&factory, node, |child| {
		Some(visit_planned_classes(
			state,
			plan,
			child,
			class_error,
			class_transformed,
		))
	});
	if class_error.is_some() || !is_class_declaration(visited) || visited.name().is_none() {
		return visited;
	}
	let class_plan = match planned_class_for_node(state, plan, visited) {
		Err(error) => {
			*class_error = Some(error);
			return visited;
		}
		Ok(None) => return visited,
		Ok(Some(class_plan)) => class_plan,
	};
	let class_only_plan = FilePlan {
		file_id: plan.file_id.clone(),
		metadata: plan.metadata.clone(),
		classes: vec![class_plan],
	};
	match transform_flamework_class(state, &class_only_plan, visited) {
		Err(error) => {
			*class_error = Some(error);
			visited
		}
		Ok(transformed) => {
			*class_transformed = true;
			transformed
		}
	}
}

/// `sourceNeedsFlameworkExpressionTransform`: a cheap, over-inclusive prefilter for the
/// expression visitor.
fn source_needs_flamework_expression_transform(state: &mut TransformState<'_>, source_file: P<SourceFile>) -> bool {
	source_declares_flamework_expression_surface(state, source_file) || reaches_expression_surface(state, source_file)
}

fn reaches_expression_surface(state: &TransformState<'_>, source_file: P<SourceFile>) -> bool {
	state
		.flamework_expression_surfaces
		.as_ref()
		.is_some_and(|surfaces| surfaces.contains(source_file.file_name()))
}

/// `callMayBeFlameworkMacro`: a cheap filter before resolving a call's signature.
pub(crate) fn call_may_be_flamework_macro(state: &mut TransformState<'_>, node: P<Node>) -> bool {
	if !node.type_arguments().is_empty() || node.expression().is_some_and(callee_may_be_flamework_macro) {
		return true;
	}
	let Some(source_file) = get_source_file_of_node(node) else {
		return false;
	};
	source_declares_flamework_expression_surface(state, source_file) || reaches_expression_surface(state, source_file)
}

/// `sourceMayNeedFlameworkAccessRewrite`
pub(crate) fn source_may_need_flamework_access_rewrite(
	state: &mut TransformState<'_>,
	source_file: P<SourceFile>,
) -> bool {
	source_declares_flamework_expression_surface(state, source_file) || reaches_expression_surface(state, source_file)
}

fn callee_may_be_flamework_macro(expression: P<Node>) -> bool {
	let mut current = skip_parentheses(expression);
	loop {
		if is_identifier(current) {
			return FLAMEWORK_MACRO_CALLEE_NAMES.contains(&current.text());
		}
		if is_property_access_expression(current) {
			if current
				.name()
				.is_some_and(|name| FLAMEWORK_MACRO_CALLEE_NAMES.contains(&name.text()))
			{
				return true;
			}
			let Some(next) = current.expression() else {
				return false;
			};
			current = skip_parentheses(next);
			continue;
		}
		// Dynamic callees can still resolve to an intrinsic; only signature resolution can tell.
		return is_element_access_expression(current);
	}
}

/// `sourceDeclaresFlameworkSurface`: an `@flamework/*` import, a `_flamework` marker or a
/// `@metadata` tag in the file itself.
fn source_declares_flamework_surface(state: &mut TransformState<'_>, source_file: P<SourceFile>) -> bool {
	if let Some(cached) = state.flamework_surface(source_file) {
		return cached;
	}
	let declares = source_file
		.imports()
		.iter()
		.any(|import| import.text().starts_with(FLAMEWORK_MODULE_PREFIX))
		|| {
			let text = source_file.text();
			text.contains(FLAMEWORK_MARKER_PREFIX) || text.contains(FLAMEWORK_METADATA_TAG)
		};
	state.set_flamework_surface(source_file, declares);
	declares
}

/// `mergeTransformAnalyses`: the caller's analyses (whose classes count as decorated) merged
/// with discovery's, by file ID in first-seen order.
fn merge_transform_analyses(
	explicit: &[FileAnalysis],
	discovered: &[FileAnalysis],
) -> Result<Vec<FileAnalysis>, Error> {
	let mut merged: Vec<FileAnalysis> = Vec::with_capacity(explicit.len() + discovered.len());
	let mut indices: HashMap<String, usize> = HashMap::with_capacity(explicit.len() + discovered.len());
	let characterized = explicit.iter().cloned().map(|mut analysis| {
		for class in &mut analysis.classes {
			class.contains_legacy_decorator = true;
		}
		analysis
	});
	for analysis in characterized.chain(discovered.iter().cloned()) {
		let file_id = normalize_file_id(&analysis.file_id).map_err(|error| error.wrap("merge Flamework analysis"))?;
		let Some(&index) = indices.get(&file_id) else {
			indices.insert(file_id.clone(), merged.len());
			merged.push(FileAnalysis {
				file_id,
				metadata: analysis.metadata.clone(),
				classes: analysis.classes.clone(),
			});
			continue;
		};
		let current = &mut merged[index];
		let mut tokens = current.metadata.tokens();
		tokens.extend(analysis.metadata.tokens());
		current.metadata = Metadata::new(tokens);
		for class in analysis.classes {
			match current
				.classes
				.iter()
				.position(|candidate| candidate.internal_id == class.internal_id)
			{
				None => current.classes.push(class),
				Some(class_index) => {
					current.classes[class_index] = merge_transform_class_plan(&current.classes[class_index], &class);
				}
			}
		}
	}
	Ok(merged)
}

fn merge_transform_class_plan(characterized: &ClassPlan, discovered: &ClassPlan) -> ClassPlan {
	let mut result = discovered.clone();
	result.contains_legacy_decorator = characterized.contains_legacy_decorator || discovered.contains_legacy_decorator;
	if result.file_path.is_empty() {
		result.file_path.clone_from(&characterized.file_path);
	}
	let seen: HashSet<String> = result
		.decorators
		.iter()
		.map(|decorator| decorator.internal_id.clone())
		.collect();
	for decorator in &characterized.decorators {
		if !seen.contains(&decorator.internal_id) {
			result.decorators.push(decorator.clone());
		}
	}
	result
}

fn planned_class_for_node(
	state: &TransformState<'_>,
	plan: &FilePlan,
	node: P<Node>,
) -> Result<Option<ClassPlan>, Error> {
	let internal_id = declaration_internal_id(state, node).map_err(|error| {
		let name = node.name().map_or("", |name| name.text());
		error.wrap(&format!("resolve planned class {}", rotor_support::gostr::quote(name)))
	})?;
	Ok(plan
		.classes
		.iter()
		.find(|class| class.internal_id == internal_id)
		.cloned())
}

/// `classNameFromInternalID`: the last dotted segment after `@`.
pub(crate) fn class_name_from_internal_id(internal_id: &str) -> String {
	let Some(separator) = internal_id
		.rfind('@')
		.filter(|separator| *separator != internal_id.len() - 1)
	else {
		return String::new();
	};
	let name = &internal_id[separator + 1..];
	name.rfind('.').map_or(name, |nested| &name[nested + 1..]).to_owned()
}

/// `luaFileName`: the module name a TypeScript file compiles to.
pub(crate) fn lua_file_name(file_name: &str) -> String {
	let file_name = file_name.strip_suffix(".ts").unwrap_or(file_name);
	let file_name = file_name.strip_suffix(".d").unwrap_or(file_name);
	// Go's LastIndexAny(`/\\`) names '/' and '\\' (the raw string repeats the backslash).
	let file_name = file_name
		.rfind(['/', '\\'])
		.map_or(file_name, |separator| &file_name[separator + 1..]);
	if file_name == "index" {
		return "init".to_owned();
	}
	file_name.to_owned()
}

/// `sourceDeclaresFlameworkExpressionSurface`: the file itself triggers expression transforms.
fn source_declares_flamework_expression_surface(state: &mut TransformState<'_>, source_file: P<SourceFile>) -> bool {
	if let Some(cached) = state.flamework_expression_surface(source_file) {
		return cached;
	}
	let declares = scan_flamework_expression_surface(state, source_file);
	state.set_flamework_expression_surface(source_file, declares);
	declares
}

fn scan_flamework_expression_surface(state: &mut TransformState<'_>, source_file: P<SourceFile>) -> bool {
	if source_declares_flamework_surface(state, source_file) {
		return true;
	}
	let text = source_file.text();
	if text.is_empty() {
		return false;
	}
	["Flamework", "Modding", "Reflect.", ".attributes"]
		.iter()
		.any(|needle| text.contains(needle))
		|| FLAMEWORK_MACRO_CALLEE_NAMES.iter().any(|name| text.contains(name))
}

/// `initializeFlameworkExpressionSurfaces`: every file that is, or transitively imports, a
/// direct expression surface. A non-module surface outside the libraries marks every file.
pub(crate) fn initialize_flamework_expression_surfaces(state: &mut TransformState<'_>) {
	if state.flamework_expression_surfaces.is_some() {
		return;
	}
	let mut surfaces: HashSet<String> = HashSet::new();
	let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
	let all = state.program.get_source_files();
	for &file in all {
		let file_name = file.file_name();
		for &specifier in file.imports() {
			let Some(resolved) = state.program.get_resolved_module_from_module_specifier(file, specifier) else {
				continue;
			};
			if !resolved.is_resolved() {
				continue;
			}
			let imported = resolved.resolved_file_name;
			if imported.is_empty() || imported == file_name {
				continue;
			}
			reverse
				.entry(imported.to_owned())
				.or_default()
				.push(file_name.to_owned());
		}
	}

	let mut queue: VecDeque<String> = VecDeque::with_capacity(all.len());
	let mut has_global = false;
	for &seed in all {
		let seed_name = seed.file_name().to_owned();
		if !source_declares_flamework_expression_surface(state, seed) {
			continue;
		}
		if surfaces.insert(seed_name.clone()) {
			queue.push_back(seed_name.clone());
		}
		if !is_external_module(seed) && !seed_name.starts_with("bundled:") && !seed_name.contains("/libs/") {
			has_global = true;
		}
	}
	if has_global {
		surfaces.extend(all.iter().map(|file| file.file_name().to_owned()));
		state.flamework_expression_surfaces = Some(surfaces);
		return;
	}
	while let Some(current) = queue.pop_front() {
		if let Some(importers) = reverse.get(&current) {
			for importer in importers {
				if surfaces.insert(importer.clone()) {
					queue.push_back(importer.clone());
				}
			}
		}
	}
	state.flamework_expression_surfaces = Some(surfaces);
}

/// Go's `filepath.ToSlash(filepath.Clean(...))` of a file name, for plan matching.
#[must_use]
pub fn clean_slashed(file_name: &str) -> String {
	gopath::to_slash(&gopath::clean(file_name)).into_owned()
}
