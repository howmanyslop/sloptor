//! `transform_addpaths.go`: expanding `Flamework.addPaths` calls into static imports.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use rotor_rojo::RbxType;
use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFactory, NodeFlags, SourceFile, TokenFlags, is_block, is_call_expression, is_expression_statement,
	is_property_access_expression, is_source_file, skip_parentheses,
};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::tspath::{
	ComparePathsOptions, ensure_path_is_non_module_name, get_directory_path, get_relative_path_from_directory,
	remove_file_extension,
};

use crate::Error;
use crate::glob::{clean_glob_pattern, match_glob, match_glob_segments};
use crate::path::project_relative_path;
use crate::project::Project;
use crate::transform::call_may_be_flamework_macro;
use crate::transform_expressions::map_children;
use crate::transform_macro_types::{alloc, read_macro_metadata, string_literal_value};
use crate::transform_macros::flamework_rewrite_symbol;
use crate::transform_paths::{path_intrinsic_rbx_path, project_glob, project_path_input};
use crate::transform_state::{TransformState, new_transform_state_base};

/// The Luau transformer's `FlameworkStaticImportMarker` (`rotor_transformer`'s
/// `extras::importexpr`), which marks a static `import()` this transform emits.
pub const FLAMEWORK_STATIC_IMPORT_MARKER: &str = "sloptor:flamework-static-import";

/// `StaticAddPathsMatches`: each plain `addPaths` call's compile-time module set. An incremental
/// input only: not written to `flamework.build` or `globs.json`.
///
/// # Errors
///
/// A pattern is invalid or a walk failed.
pub fn static_add_paths_matches(
	program: &'static Program,
	checker: &mut rotor_tsrs::checker::Checker,
	project: &mut Project,
) -> Result<BTreeMap<String, Vec<String>>, Error> {
	let mut state = new_transform_state_base(program, checker, project, None, &[]);
	let mut matches: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for &file in program.get_source_files() {
		if file.is_declaration_file() || program.is_source_file_from_external_library(file) {
			continue;
		}
		static_add_paths_in(&mut state, file, file.as_node(), &mut matches)?;
	}
	Ok(matches)
}

fn static_add_paths_in(
	state: &mut TransformState<'_>,
	file: P<SourceFile>,
	node: P<Node>,
	matches: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
	if is_call_expression(node)
		&& !node.arguments().is_empty()
		&& node.expression().is_some_and(is_add_paths_callee)
		&& call_may_be_flamework_macro(state, node)
	{
		let signature = state.ck().get_resolved_signature(node);
		let target = flamework_rewrite_symbol(&read_macro_metadata(state, signature.declaration()));
		let argument_type = state.ck().get_type_at_location(node.arguments()[0]);
		if let Some(target) = target
			&& target.parent().is_some_and(|parent| parent.name() == "Flamework")
			&& target.name() == "_addPaths"
			&& argument_type.is_string_literal()
			&& let Some(pattern) = add_paths_pattern(state, &string_literal_value(argument_type))
		{
			let clean = clean_glob_pattern(&pattern)?;
			let (mut modules, sources) = add_paths_modules(state, file, &clean)?;
			if has_unimportable_module(state, &clean, &sources)? {
				modules.push("<runtime-only-module>".to_owned());
			}
			matches.entry(clean).or_default().extend(modules);
		}
	}
	for child in node.iter_children() {
		static_add_paths_in(state, file, child, matches)?;
	}
	Ok(())
}

/// `expandAddPaths`: each standalone `Flamework.addPaths`/`addPathsGlob` call becomes
/// side-effect imports at the call site; calls it cannot expand keep the runtime rewrite.
pub(crate) fn expand_add_paths(
	state: &mut TransformState<'_>,
	source_file: P<SourceFile>,
) -> Result<P<SourceFile>, Error> {
	let source_file = expand_top_level_add_paths(state, source_file)?;
	let mut failure: Option<Error> = None;
	let visited = visit_add_paths(state, source_file, source_file.as_node(), &mut failure);
	if let Some(error) = failure {
		return Err(error);
	}
	Ok(visited.as_source_file_p())
}

fn visit_add_paths(
	state: &mut TransformState<'_>,
	file: P<SourceFile>,
	node: P<Node>,
	failure: &mut Option<Error>,
) -> P<Node> {
	if failure.is_some() {
		return node;
	}
	if is_expression_statement(node) {
		match expand_add_paths_statement(state, file, node) {
			Err(error) => {
				*failure = Some(error);
				return node;
			}
			Ok(Some(imports)) => {
				let statements: Vec<P<Node>> = imports
					.iter()
					.filter_map(|declaration| declaration.module_specifier())
					.map(|specifier| static_import_call(&state.factory, specifier))
					.collect();
				if let Some(parent) = node.parent()
					&& !is_block(parent)
					&& !is_source_file(parent)
				{
					return state.factory.new_block(state.factory.new_node_list(statements), true);
				}
				return state.factory.new_syntax_list(rotor_tsrs::tscore::alloc_vec(statements));
			}
			Ok(None) => {}
		}
	}
	let factory = state.factory.clone();
	map_children(&factory, node, |child| {
		Some(visit_add_paths(state, file, child, failure))
	})
}

/// `staticImportCall`: `import(specifier + marker)`, which the Luau transformer turns into an
/// immediate `TS.import`.
fn static_import_call(factory: &NodeFactory, module_specifier: P<Node>) -> P<Node> {
	let marked = factory.new_binary_expression(
		None,
		module_specifier,
		None,
		factory.new_token(Kind::PlusToken),
		factory.new_string_literal(FLAMEWORK_STATIC_IMPORT_MARKER, TokenFlags::None),
	);
	let call = factory.new_call_expression(
		factory.new_keyword_expression(Kind::ImportKeyword),
		None,
		None,
		factory.new_node_list(vec![marked]),
		NodeFlags::None,
	);
	factory.new_expression_statement(call)
}

fn expand_top_level_add_paths(
	state: &mut TransformState<'_>,
	source_file: P<SourceFile>,
) -> Result<P<SourceFile>, Error> {
	let all = source_file.statements().nodes();
	let mut statements = Vec::with_capacity(all.len());
	let mut changed = false;
	for &statement in all {
		match expand_add_paths_statement(state, source_file, statement)? {
			Some(imports) => {
				changed = true;
				statements.extend(imports);
			}
			None => statements.push(statement),
		}
	}
	if !changed {
		return Ok(source_file);
	}
	Ok(state
		.factory
		.update_source_file(
			source_file.as_node(),
			state.factory.new_node_list(statements),
			source_file.end_of_file_token,
		)
		.as_source_file_p())
}

fn expand_add_paths_statement(
	state: &mut TransformState<'_>,
	file: P<SourceFile>,
	statement: P<Node>,
) -> Result<Option<Vec<P<Node>>>, Error> {
	if !is_expression_statement(statement) {
		return Ok(None);
	}
	let Some(call) = statement.expression() else {
		return Ok(None);
	};
	if !is_call_expression(call)
		|| call.arguments().is_empty()
		|| !call.expression().is_some_and(is_add_paths_callee)
		|| !call_may_be_flamework_macro(state, call)
	{
		return Ok(None);
	}
	let signature = state.ck().get_resolved_signature(call);
	let Some(target) = flamework_rewrite_symbol(&read_macro_metadata(state, signature.declaration())) else {
		return Ok(None);
	};
	if target.parent().is_none_or(|parent| parent.name() != "Flamework") {
		return Ok(None);
	}
	let argument_type = state.ck().get_type_at_location(call.arguments()[0]);
	if !argument_type.is_string_literal() {
		return Ok(None);
	}
	let value = string_literal_value(argument_type);
	let pattern = match target.name() {
		"_addPaths" => add_paths_pattern(state, &value),
		"_addPathsGlob" => {
			let pattern = project_glob(state, file, &value);
			if let Some(pattern) = &pattern {
				// Registered as upstream does, so a new match recompiles this file.
				let file_id = project_relative_path(&state.project.root_directory, file.file_name())?;
				state.add_glob(pattern, &file_id);
			}
			pattern
		}
		_ => None,
	};
	let Some(pattern) = pattern else {
		return Ok(None);
	};
	let Ok(clean) = clean_glob_pattern(&pattern) else {
		return Ok(None);
	};
	let (modules, sources) = add_paths_modules(state, file, &clean)?;
	if has_unimportable_module(state, &clean, &sources)? {
		return Ok(None);
	}
	let imports = modules
		.iter()
		.map(|module| {
			state.factory.new_import_declaration(
				None,
				None,
				state.factory.new_string_literal(alloc(module), TokenFlags::None),
				None,
			)
		})
		.collect();
	Ok(Some(imports))
}

/// `isAddPathsCallee`: a cheap syntactic prefilter.
fn is_add_paths_callee(callee: P<Node>) -> bool {
	let callee = skip_parentheses(callee);
	is_property_access_expression(callee)
		&& callee
			.name()
			.is_some_and(|name| matches!(name.text(), "addPaths" | "addPathsGlob"))
}

/// `addPathsPattern`: the target and every descendant; `None` when the target has no Rojo data.
fn add_paths_pattern(state: &TransformState<'_>, input: &str) -> Option<String> {
	let absolute = project_path_input(state, input);
	path_intrinsic_rbx_path(state, &absolute)?;
	let relative = project_relative_path(&state.project.root_directory, &absolute).ok()?;
	Some(rotor_support::gopath::Flavor::Unix.join(&[&relative, "**"]))
}

/// `addPathsSourceIndex`: every program source with a project-relative path, in program order.
pub(crate) struct AddPathsSourceIndex {
	entries: Vec<AddPathsSource>,
	sources: HashSet<String>,
}

struct AddPathsSource {
	file: P<SourceFile>,
	relative: String,
	segments: Vec<String>,
}

fn program_add_paths_sources(state: &mut TransformState<'_>) -> Rc<AddPathsSourceIndex> {
	if let Some(index) = &state.add_paths_sources {
		return Rc::clone(index);
	}
	let root = state.project.root_directory.clone();
	let mut index = AddPathsSourceIndex {
		entries: Vec::new(),
		sources: HashSet::new(),
	};
	for &source in state.program.get_source_files() {
		if source.is_declaration_file() || state.program.is_source_file_from_external_library(source) {
			continue;
		}
		let Ok(relative) = project_relative_path(&root, source.file_name()) else {
			continue;
		};
		index.sources.insert(relative.clone());
		let segments = relative.to_lowercase().split('/').map(str::to_owned).collect();
		index.entries.push(AddPathsSource {
			file: source,
			relative,
			segments,
		});
	}
	let index = Rc::new(index);
	state.add_paths_sources = Some(Rc::clone(&index));
	index
}

type AddPathsModules = (Vec<String>, Rc<AddPathsSourceIndex>);

/// `addPathsModules`: import specifiers, relative to `file`, for every program source the
/// pattern matches (itself or an ancestor directory) that Rojo places as a module.
fn add_paths_modules(
	state: &mut TransformState<'_>,
	file: P<SourceFile>,
	pattern: &str,
) -> Result<AddPathsModules, Error> {
	let compare = ComparePathsOptions {
		use_case_sensitive_file_names: state.program.use_case_sensitive_file_names(),
		..ComparePathsOptions::default()
	};
	let from_directory = get_directory_path(file.file_name());
	let sources = program_add_paths_sources(state);
	let lowered = pattern.to_lowercase();
	let pattern_segments: Vec<&str> = lowered.split('/').collect();
	let mut matches: Vec<(String, String)> = Vec::new();
	for source in &sources.entries {
		if source.file == file {
			continue;
		}
		let segments: Vec<&str> = source.segments.iter().map(String::as_str).collect();
		if !match_add_paths_glob_segments_or_ancestor(&pattern_segments, &segments).map_err(glob_error)? {
			continue;
		}
		let output = state.project.translator.get_output_path(source.file.file_name());
		if state.project.resolver.get_rbx_path_from_file_path(&output).is_none() {
			continue;
		}
		if is_script_rbx_type(state.project.resolver.get_rbx_type_from_file_path(&output)) {
			continue;
		}
		matches.push((
			source.relative.clone(),
			module_specifier(&from_directory, source.file.file_name(), &compare),
		));
	}
	// sort.Slice is not stable; relative paths are distinct, so the order is total.
	matches.sort_by(|left, right| left.0.cmp(&right.0));
	Ok((matches.into_iter().map(|(_, module)| module).collect(), sources))
}

fn glob_error(message: &'static str) -> Error {
	Error::msg(message)
}

fn match_add_paths_glob_segments_or_ancestor(pattern: &[&str], relative: &[&str]) -> Result<bool, &'static str> {
	for length in (1..=relative.len()).rev() {
		if length == 1 && (relative[0] == "." || relative[0].is_empty()) {
			break;
		}
		if match_glob_segments(pattern, &relative[..length])? {
			return Ok(true);
		}
	}
	Ok(false)
}

/// `hasUnimportableModule`: a file on disk the pattern matches and Rojo places as a module but
/// that is not a program source, such as a hand-written Lua module.
fn has_unimportable_module(
	state: &TransformState<'_>,
	pattern: &str,
	sources: &AddPathsSourceIndex,
) -> Result<bool, Error> {
	let prefix = glob_literal_prefix(pattern);
	let directory = gopath::join(&[&state.project.root_directory, &gopath::from_slash(&prefix)]);
	let mut found = false;
	match walk_unimportable(state, &directory, &prefix, "", pattern, sources, &mut found) {
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
		Err(error) => Err(Error::msg(rotor_support::gostr::io_error_text(&error))),
		Ok(()) => Ok(found),
	}
}

fn walk_unimportable(
	state: &TransformState<'_>,
	directory: &str,
	prefix: &str,
	name: &str,
	pattern: &str,
	sources: &AddPathsSourceIndex,
	found: &mut bool,
) -> std::io::Result<()> {
	let current = if name.is_empty() {
		directory.to_owned()
	} else {
		gopath::join(&[directory, &gopath::from_slash(name)])
	};
	let mut entries: Vec<(String, std::fs::FileType)> = Vec::new();
	for entry in std::fs::read_dir(&current)? {
		let entry = entry?;
		entries.push((entry.file_name().to_string_lossy().into_owned(), entry.file_type()?));
	}
	entries.sort_by(|left, right| left.0.cmp(&right.0));
	for (entry_name, file_type) in entries {
		if *found {
			return Ok(());
		}
		let child = if name.is_empty() {
			entry_name
		} else {
			format!("{name}/{entry_name}")
		};
		if file_type.is_dir() {
			walk_unimportable(state, directory, prefix, &child, pattern, sources, found)?;
			continue;
		}
		if !file_type.is_file() {
			continue;
		}
		let relative = rotor_support::gopath::Flavor::Unix.join(&[prefix, &child]);
		let base = gopath::base(&relative);
		let extension = base.rfind('.').map_or("", |dot| &base[dot..]);
		if sources.sources.contains(&relative) || !matches!(extension, ".lua" | ".luau") || relative.ends_with(".d.ts")
		{
			continue;
		}
		if !match_glob_or_ancestor(pattern, &relative).map_err(std::io::Error::other)? {
			continue;
		}
		let output = state
			.project
			.translator
			.get_output_path(&gopath::join(&[directory, &gopath::from_slash(&child)]));
		if state.project.resolver.get_rbx_path_from_file_path(&output).is_some()
			&& !is_script_rbx_type(state.project.resolver.get_rbx_type_from_file_path(&output))
		{
			*found = true;
		}
	}
	Ok(())
}

/// `globLiteralPrefix`: the pattern's leading directory segments without glob syntax.
fn glob_literal_prefix(pattern: &str) -> String {
	let segments: Vec<&str> = pattern.split('/').collect();
	for (index, segment) in segments.iter().enumerate() {
		if *segment == "**" || segment.contains(['*', '?', '[', '\\']) || index == segments.len() - 1 {
			return rotor_support::gopath::Flavor::Unix.join(&segments[..index]);
		}
	}
	String::new()
}

const fn is_script_rbx_type(rbx_type: RbxType) -> bool {
	matches!(rbx_type, RbxType::Script | RbxType::LocalScript)
}

fn match_glob_or_ancestor(pattern: &str, relative: &str) -> Result<bool, &'static str> {
	let mut candidate = relative.to_owned();
	while candidate != "." {
		if match_glob(pattern, &candidate)? {
			return Ok(true);
		}
		candidate = rotor_support::gopath::Flavor::Unix.dir(&candidate);
	}
	Ok(false)
}

fn module_specifier(from_directory: &str, to: &str, compare: &ComparePathsOptions) -> String {
	ensure_path_is_non_module_name(remove_file_extension(&get_relative_path_from_directory(
		from_directory,
		to,
		compare,
	)))
}
