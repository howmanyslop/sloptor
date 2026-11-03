//! `importexpr.go`: `util/getSourceFileFromModuleSpecifier.ts`, `util/createImportExpression.ts`
//! and `nodes/expressions/transformImportExpression.ts`.

use rotor_rojo::{
	FileRelation, NetworkType, RbxPath, RbxType, RelativeRbxPathSegment, RojoResolver, canonical_file_name,
};
use rotor_support::gopath;
use rotor_tsrs::ast::{self as tsast, get_source_file_of_node};
use rotor_tsrs::tscore::RESOLUTION_MODE_NONE;
use rotor_tsrs::tspath;
use rotor_tsrs::vfs::osvfs;

use crate::diagnostics;
use crate::prelude::*;
use crate::runtimelib::{create_get_service, property_access_expression_chain};
use crate::state::{ProjectType, RojoContext};

/// `nodeModules` (`Shared/constants.ts` `NODE_MODULES`).
const NODE_MODULES: &str = "node_modules";

/// `parentField` (`Shared/constants.ts`).
const PARENT_FIELD: &str = "Parent";

/// `getSourceFileFromModuleSpecifier` (`util/getSourceFileFromModuleSpecifier.ts` L4-33): the
/// program file a module specifier resolves to.
pub(super) fn get_source_file_from_module_specifier(
	state: &mut State<'_>,
	module_specifier: P<Node>,
) -> Option<P<SourceFile>> {
	let symbol = state
		.ck()
		.get_symbol_at_location(module_specifier)
		.or_else(|| state.ck().resolve_external_module_name(module_specifier));
	if let Some(symbol) = symbol {
		let declaration = symbol.value_declaration();

		if let Some(declaration) = declaration
			&& tsast::is_module_declaration(declaration)
			&& let Some(name) = declaration.name()
			&& tsast::is_string_literal_like(name)
		{
			// Ambient module declaration: chase the real file through the program's resolution
			// cache.
			let source_file = get_source_file_of_node(module_specifier).unwrap_or(state.source_file);
			let mode = state.program.get_mode_for_usage_location(source_file, name);
			if let Some(resolved_module) = state.program.get_resolved_module(source_file, name.text(), mode)
				&& resolved_module.is_resolved()
			{
				return state.program.get_source_file(resolved_module.resolved_file_name);
			}
		}

		if let Some(declaration) = declaration
			&& tsast::is_source_file(declaration)
		{
			return Some(declaration.as_source_file_p());
		}
	}

	// Fallback for $getModuleTree when the module is not referenced by any regular import
	// (upstream L25-32): a raw resolution through the program's resolver. Upstream passes no
	// resolution mode, which is `ResolutionModeNone` (resolve per the compiler options).
	if tsast::is_string_literal_like(module_specifier) {
		let source_file = get_source_file_of_node(module_specifier).unwrap_or(state.source_file);
		if let Some(result) =
			state
				.program
				.resolve_module_name(module_specifier.text(), source_file.file_name(), RESOLUTION_MODE_NONE)
			&& result.is_resolved()
		{
			return state.program.get_source_file(result.resolved_file_name);
		}
	}

	None
}

/// `isInsideNodeModules`: any path component is `node_modules`.
fn is_inside_node_modules(file_path: &str) -> bool {
	gopath::to_slash(file_path).split('/').any(|part| part == NODE_MODULES)
}

/// `relToProject`: a path relative to the project directory, for `noRojoData`.
fn rel_to_project(rojo: &RojoContext, target: &str) -> String {
	gopath::rel(&rojo.project_path, target).unwrap_or_else(|_| target.to_owned())
}

fn none_parts(state: &mut State<'_>) -> Vec<NodeId> {
	vec![state.ast.none()]
}

/// `getAbsoluteImport` (L15-24): `[game:GetService("<p0>"), "p1", "p2", ...]`.
fn get_absolute_import(state: &mut State<'_>, module_rbx_path: &[String]) -> Vec<NodeId> {
	let Some((service_name, rest)) = module_rbx_path.split_first() else {
		state.ice("transformer: getAbsoluteImport: empty Roblox path");
		return none_parts(state);
	};
	let mut path_expressions = vec![create_get_service(&mut state.ast, service_name)];
	for part in rest {
		path_expressions.push(state.ast.str(part.as_str()));
	}
	path_expressions
}

/// `getRelativeImport` (L26-47): the leading parent segments fold into one
/// `script.Parent.Parent...` root; the remaining names follow as strings.
fn get_relative_import(state: &mut State<'_>, source_rbx_path: &[String], module_rbx_path: &[String]) -> Vec<NodeId> {
	let relative_path = rotor_rojo::relative(source_rbx_path, module_rbx_path);

	let parents: Vec<String> = relative_path
		.iter()
		.take_while(|segment| **segment == RelativeRbxPathSegment::Parent)
		.map(|_| PARENT_FIELD.to_owned())
		.collect();

	let script = state.ast.global_id("script");
	let mut path_expressions = vec![property_access_expression_chain(&mut state.ast, script, &parents)];
	for segment in &relative_path[parents.len()..] {
		// upstream assert(typeof pathPart === "string"): relative never yields a parent after a
		// name segment.
		if let RelativeRbxPathSegment::Name(name) = segment {
			path_expressions.push(state.ast.str(name.as_str()));
		}
	}
	path_expressions
}

/// `validateModule` (L49-59): the scope's directory must equal one of the type roots.
fn validate_module(rojo: &RojoContext, scope: &str) -> bool {
	let scoped_modules = gopath::clean(&gopath::join(&[&rojo.node_modules_path, scope]));
	rojo.type_roots
		.iter()
		.any(|type_root| scoped_modules == gopath::clean(&gopath::from_slash(type_root)))
}

/// `findRelativeRbxPath` (L61-68): the first package resolver that covers the path wins.
fn find_relative_rbx_path(module_out_path: &str, pkg_rojo_resolvers: &[RojoResolver]) -> Option<RbxPath> {
	pkg_rojo_resolvers
		.iter()
		.find_map(|resolver| resolver.get_rbx_path_from_file_path(module_out_path))
}

fn get_paths_with_scope(resolver: &RojoResolver, module_scope: &str) -> Vec<String> {
	let mut paths: Vec<String> = Vec::new();
	for partition in resolver.partitions() {
		let normalized = gopath::clean(&partition.fs_path);
		if normalized.ends_with(module_scope) && !normalized.contains(NODE_MODULES) && !paths.contains(&normalized) {
			paths.push(normalized);
		}
	}
	paths
}

fn split_native(path: &str) -> Vec<String> {
	path.split(gopath::NATIVE.separator()).map(str::to_owned).collect()
}

fn realpath(path: &str) -> String {
	gopath::from_slash(&osvfs::fs().realpath(&gopath::to_slash(path))).into_owned()
}

/// A path under `node_modules_path` split into its package directory (`node_modules/<name>` or
/// `node_modules/@scope/<name>`), the package's real directory, and the path inside it.
struct NodeModulesPackage {
	package_path: String,
	real_package_path: String,
	inner: Vec<String>,
}

fn node_modules_package(node_modules_path: &str, file_path: &str) -> Option<NodeModulesPackage> {
	let rel = gopath::rel(node_modules_path, file_path).ok()?;
	let segments = split_native(&rel);
	let package_segments = if segments[0].starts_with('@') { 2 } else { 1 };
	if segments[0] == ".." || segments.len() < package_segments {
		return None;
	}
	let package: Vec<&str> = segments[..package_segments].iter().map(String::as_str).collect();
	let package_path = gopath::join(&[node_modules_path, &gopath::join(&package)]);
	let real_package_path = realpath(&package_path);
	Some(NodeModulesPackage {
		package_path,
		real_package_path,
		inner: segments[package_segments..].to_vec(),
	})
}

fn join_inner(base: &str, inner: &[String]) -> String {
	let inner: Vec<&str> = inner.iter().map(String::as_str).collect();
	gopath::join(&[base, &gopath::join(&inner)])
}

fn resolve_symlinked_module_path(
	node_modules_path: &str,
	module_out_path: &str,
	use_case_sensitive_file_names: bool,
) -> Option<String> {
	let package = node_modules_package(node_modules_path, module_out_path)?;
	if canonical_file_name(&package.real_package_path, use_case_sensitive_file_names)
		== canonical_file_name(&package.package_path, use_case_sensitive_file_names)
	{
		return None;
	}
	Some(join_inner(&package.real_package_path, &package.inner))
}

/// `referencePackagePath`: rebases a referenced project's Luau path onto the `node_modules`
/// package directory `node_module_path` sits in.
fn reference_package_path(rojo: &RojoContext, node_module_path: &str, reference_path: &str) -> Option<String> {
	let package = node_modules_package(&rojo.node_modules_path, &gopath::from_slash(node_module_path))?;
	let reference_path = gopath::from_slash(reference_path);
	let inner = relative_inside(&package.real_package_path, &reference_path).or_else(|| {
		// The reference path may be spelled through an alias of the real package directory (a
		// junction or 8.3 short path).
		relative_inside(&package.real_package_path, &real_path_of_ancestor(&reference_path))
	})?;
	Some(gopath::join(&[&package.package_path, &inner]))
}

fn relative_inside(base: &str, target: &str) -> Option<String> {
	let inner = gopath::rel(base, target).ok()?;
	let parent_prefix = format!("..{}", gopath::NATIVE.separator());
	if inner == ".." || inner.starts_with(&parent_prefix) {
		return None;
	}
	Some(inner)
}

/// `realPathOfAncestor`: resolves the nearest existing ancestor and rejoins the missing tail,
/// since output files may not exist yet.
fn real_path_of_ancestor(path: &str) -> String {
	let fs = osvfs::fs();
	let mut missing: Vec<String> = Vec::new();
	let mut dir = path.to_owned();
	loop {
		let slashed = gopath::to_slash(&dir).into_owned();
		if fs.directory_exists(&slashed) || fs.file_exists(&slashed) {
			let mut resolved = gopath::from_slash(&fs.realpath(&slashed)).into_owned();
			for part in missing.iter().rev() {
				resolved = gopath::join(&[&resolved, part]);
			}
			return resolved;
		}
		let parent = gopath::dir(&dir);
		if parent == dir {
			return path.to_owned();
		}
		missing.push(gopath::base(&dir));
		dir = parent;
	}
}

/// `getNodeModulesImportParts` (L70-134).
fn get_node_modules_import_parts(
	state: &mut State<'_>,
	rojo: &RojoContext,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
	module_out_path: &str,
) -> Vec<NodeId> {
	let rel = match gopath::rel(&rojo.node_modules_path, module_out_path) {
		Ok(rel) if !rel.is_empty() => rel,
		_ => {
			state.ice("transformer: getNodeModulesImportParts: no module scope");
			return none_parts(state);
		}
	};
	let rel_segments = split_native(&rel);
	let module_scope = rel_segments[0].clone();

	if !module_scope.starts_with('@') {
		state.diags.add(diagnostics::no_unscoped_module(module_specifier));
		return none_parts(state);
	}

	if !validate_module(rojo, &module_scope) {
		state.diags.add(diagnostics::no_invalid_module(module_specifier));
		return none_parts(state);
	}

	if state.project_type == Some(ProjectType::Package) {
		let Some(relative_rbx_path) = find_relative_rbx_path(module_out_path, &rojo.pkg_rojo_resolvers) else {
			let path = rel_to_project(rojo, module_out_path);
			state
				.diags
				.add(diagnostics::no_rojo_data(module_specifier, &path, true));
			return none_parts(state);
		};

		let Some((module_name, rest)) = relative_rbx_path.split_first() else {
			state.ice("transformer: getNodeModulesImportParts: empty package Roblox path");
			return none_parts(state);
		};

		let get_module = state.runtime_lib(module_specifier.parent(), "getModule");
		let script = state.ast.global_id("script");
		let scope = state.ast.str(module_scope.as_str());
		let name = state.ast.str(module_name.as_str());
		let args = state.ast.list_from([script, scope, name]);
		let call = state.ast.call(get_module, args);
		return vec![property_access_expression_chain(&mut state.ast, call, rest)];
	}

	let mut module_rbx_path = rojo.resolver.get_rbx_path_from_file_path(module_out_path);
	if module_rbx_path.is_none()
		&& let Some(resolved) = resolve_symlinked_module_path(
			&rojo.node_modules_path,
			module_out_path,
			rojo.use_case_sensitive_file_names,
		) {
		module_rbx_path = rojo.resolver.get_rbx_path_from_file_path(&resolved);
	}
	if module_rbx_path.is_none() {
		let package_relative_path = &rel_segments[1..];
		for scope_path in get_paths_with_scope(&rojo.resolver, &module_scope) {
			let candidate = join_inner(&scope_path, package_relative_path);
			if let Some(resolved_path) = rojo.resolver.get_rbx_path_from_file_path(&candidate) {
				module_rbx_path = Some(resolved_path);
				break;
			}
		}
	}
	let Some(module_rbx_path) = module_rbx_path else {
		let path = rel_to_project(rojo, module_out_path);
		state
			.diags
			.add(diagnostics::no_rojo_data(module_specifier, &path, true));
		return none_parts(state);
	};

	let index_of_scope = module_rbx_path.iter().position(|part| *part == module_scope);
	if !index_of_scope.is_some_and(|index| index > 0 && module_rbx_path[index - 1] == NODE_MODULES) {
		let path = rel_to_project(rojo, module_out_path);
		state.diags.add(diagnostics::no_package_import_without_scope(
			module_specifier,
			&path,
			&module_rbx_path,
		));
		return none_parts(state);
	}

	get_project_import_parts(
		state,
		rojo,
		source_file,
		module_specifier,
		module_out_path,
		&module_rbx_path,
	)
}

/// `getProjectImportParts` (L136-182).
fn get_project_import_parts(
	state: &mut State<'_>,
	rojo: &RojoContext,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
	module_out_path: &str,
	module_rbx_path: &[String],
) -> Vec<NodeId> {
	let module_rbx_type = rojo.resolver.get_rbx_type_from_file_path(module_out_path);
	if module_rbx_type == RbxType::Script || module_rbx_type == RbxType::LocalScript {
		state.diags.add(diagnostics::no_non_module_import(module_specifier));
		return none_parts(state);
	}

	let source_out_path = rojo.path_translator.get_output_path(source_file.file_name());
	let Some(source_rbx_path) = rojo.resolver.get_rbx_path_from_file_path(&source_out_path) else {
		let path = rel_to_project(rojo, &source_out_path);
		state
			.diags
			.add(diagnostics::no_rojo_data(source_file.as_node(), &path, false));
		return none_parts(state);
	};

	if state.project_type == Some(ProjectType::Game) {
		// In the case of `import("")`, don't do the network type check as the call may be guarded
		// by runtime RunService checks (upstream comment).
		if !module_specifier.parent().is_some_and(tsast::is_import_call)
			&& rojo.resolver.get_network_type(module_rbx_path) == NetworkType::Server
			&& rojo.resolver.get_network_type(&source_rbx_path) != NetworkType::Server
		{
			state.diags.add(diagnostics::no_server_import(module_specifier));
			return none_parts(state);
		}

		return match rojo.resolver.get_file_relation(&source_rbx_path, module_rbx_path) {
			FileRelation::OutToOut | FileRelation::InToOut => get_absolute_import(state, module_rbx_path),
			FileRelation::InToIn => get_relative_import(state, &source_rbx_path, module_rbx_path),
			FileRelation::OutToIn => {
				state.diags.add(diagnostics::no_isolated_import(module_specifier));
				none_parts(state)
			}
		};
	}

	get_relative_import(state, &source_rbx_path, module_rbx_path)
}

/// `getImportParts` (L184-210): resolve the specifier to a file, translate to its output
/// location, and pick the `node_modules` or project pipeline. Every error path adds its
/// diagnostic and returns `[none]`.
fn get_import_parts(state: &mut State<'_>, source_file: P<SourceFile>, module_specifier: P<Node>) -> Vec<NodeId> {
	get_import_parts_impl(state, source_file, module_specifier, false)
}

/// `getModuleTreeImportParts`: [`get_import_parts`] with rotor's folder extension, which only
/// `$getModuleTree` opts into.
pub(super) fn get_module_tree_import_parts(
	state: &mut State<'_>,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
) -> Vec<NodeId> {
	get_import_parts_impl(state, source_file, module_specifier, true)
}

fn get_import_parts_impl(
	state: &mut State<'_>,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
	allow_folder: bool,
) -> Vec<NodeId> {
	// rotor guard: a state without a Rojo context gets a diagnostic.
	let Some(rojo) = state.rojo else {
		state.diags.add(diagnostics::rotor_no_project_context(module_specifier));
		return none_parts(state);
	};

	let Some(module_file) = get_source_file_from_module_specifier(state, module_specifier) else {
		if allow_folder && let Some(parts) = get_folder_import_parts(state, rojo, source_file, module_specifier) {
			return parts;
		}
		state.diags.add(diagnostics::no_module_specifier_file(module_specifier));
		return none_parts(state);
	};

	// createImportExpression.ts:191: everything below operates on the virtual path.
	let virtual_path = state
		.guess_virtual_path(module_file.file_name())
		.unwrap_or_else(|| module_file.file_name().to_owned());

	let mut mapped_path = virtual_path.clone();
	let mut has_mapped_path = false;
	let key = canonical_file_name(&virtual_path, rojo.use_case_sensitive_file_names);
	let mut reference_path = rojo.project_reference_paths.get(&key).cloned();
	if reference_path.is_none() && !rojo.project_reference_paths.is_empty() {
		// Project references use real config paths, while package resolution may expose a
		// node_modules symlink (including with preserveSymlinks).
		let real_path = state.program.host().fs().realpath(module_file.file_name());
		let real_key = canonical_file_name(&real_path, rojo.use_case_sensitive_file_names);
		reference_path = rojo.project_reference_paths.get(&real_key).cloned();
	}
	if let Some(mapped) = rojo
		.node_modules_path_mapping
		.lookup(&virtual_path, rojo.use_case_sensitive_file_names)
	{
		mapped_path = mapped;
		has_mapped_path = true;
	}
	if !has_mapped_path
		&& let Some(mapped) = rojo
			.node_modules_path_mapping
			.lookup(module_file.file_name(), rojo.use_case_sensitive_file_names)
	{
		mapped_path = mapped;
		has_mapped_path = true;
	}

	let is_node_module = is_inside_node_modules(&virtual_path) || has_mapped_path;
	// A package reaches a referenced package through getModule, never through a Rojo path, so its
	// output stays relocatable. The reference map only supplies the Luau path inside the package.
	if is_node_module && (reference_path.is_none() || state.project_type == Some(ProjectType::Package)) {
		let module_out_path = reference_path
			.as_ref()
			.and_then(|reference_path| reference_package_path(rojo, &mapped_path, reference_path))
			.unwrap_or_else(|| rojo.path_translator.get_import_path(&mapped_path, true));
		return get_node_modules_import_parts(state, rojo, source_file, module_specifier, &module_out_path);
	}

	let module_out_path = reference_path.unwrap_or_else(|| {
		let mut module_out_path = rojo.path_translator.get_import_path(&virtual_path, false);
		let outside_root = gopath::rel(&rojo.path_translator.root_dir, &gopath::from_slash(&virtual_path))
			.map_or(true, |relative| {
				relative == ".." || relative.starts_with(&format!("..{}", gopath::NATIVE.separator()))
			});
		if outside_root {
			if tspath::is_declaration_file_name(&virtual_path) {
				// Keep the path in place (no outDir rebase), but map index to init.
				module_out_path = rojo.path_translator.get_import_path(&virtual_path, true);
			} else if tsast::is_json_source_file(module_file) {
				module_out_path.clone_from(&virtual_path);
			}
			// Other sources outside rootDir already carry a TypeScript error (upstream asserts);
			// a census still transforms them, so keep the translated path.
		}
		module_out_path
	});
	let Some(module_rbx_path) = rojo.resolver.get_rbx_path_from_file_path(&module_out_path) else {
		let path = rel_to_project(rojo, &module_out_path);
		state
			.diags
			.add(diagnostics::no_rojo_data(module_specifier, &path, false));
		return none_parts(state);
	};
	get_project_import_parts(
		state,
		rojo,
		source_file,
		module_specifier,
		&module_out_path,
		&module_rbx_path,
	)
}

/// `getFolderImportParts`, a rotor extension: `$getModuleTree` on a folder. Candidate bases, in
/// order: the importing file's directory (relative specifiers); the tsconfig `paths`
/// substitutions, then the project directory (non-relative). The first existing directory with
/// Rojo data wins.
fn get_folder_import_parts(
	state: &mut State<'_>,
	rojo: &RojoContext,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
) -> Option<Vec<NodeId>> {
	if !tsast::is_string_literal_like(module_specifier) {
		return None;
	}
	let spec = module_specifier.text();
	let program = state.program;

	let mut candidates = Vec::new();
	if spec == "." || spec == ".." || spec.starts_with("./") || spec.starts_with("../") {
		candidates.push(tspath::resolve_path(
			&tspath::get_directory_path(source_file.file_name()),
			&[spec],
		));
	} else {
		let options = program.options();
		let paths_base = options.get_paths_base_path(program.get_current_directory());
		if let Some(paths) = options.paths.as_ref() {
			for (pattern, substitutions) in paths.iter() {
				let Some(star_text) = match_paths_pattern(pattern, spec) else {
					continue;
				};
				for substitution in substitutions.iter().flatten() {
					candidates.push(tspath::resolve_path(
						&paths_base,
						&[&substitution.replacen('*', star_text, 1)],
					));
				}
			}
		}
		candidates.push(tspath::resolve_path(program.get_current_directory(), &[spec]));
	}

	let fs = program.host().fs();
	for dir in candidates {
		if !fs.directory_exists(&dir) {
			continue;
		}
		let module_out_path = rojo.path_translator.get_output_path(&dir);
		let Some(module_rbx_path) = rojo.resolver.get_rbx_path_from_file_path(&module_out_path) else {
			continue;
		};
		return Some(get_project_import_parts(
			state,
			rojo,
			source_file,
			module_specifier,
			&module_out_path,
			&module_rbx_path,
		));
	}
	None
}

/// `matchPathsPattern`: matches a tsconfig `paths` pattern (at most one `*`), returning the text
/// the star matched.
fn match_paths_pattern<'a>(pattern: &str, candidate: &'a str) -> Option<&'a str> {
	let Some(star) = pattern.find('*') else {
		return (pattern == candidate).then_some("");
	};
	let (prefix, suffix) = (&pattern[..star], &pattern[star + 1..]);
	if candidate.len() >= prefix.len() + suffix.len() && candidate.starts_with(prefix) && candidate.ends_with(suffix) {
		return candidate.get(prefix.len()..candidate.len() - suffix.len());
	}
	None
}

/// `createImportExpression` (L212-220): `TS.import(script, <root expr>, "<name>"...)`.
pub(super) fn create_import_expression(
	state: &mut State<'_>,
	source_file: P<SourceFile>,
	module_specifier: P<Node>,
) -> NodeId {
	let parts = get_import_parts(state, source_file, module_specifier);
	let script = state.ast.global_id("script");
	let args = state.ast.list_from(std::iter::once(script).chain(parts));
	let import = state.runtime_lib(module_specifier.parent(), "import");
	state.ast.call(import, args)
}

/// `transformImportExpression` (`nodes/expressions/transformImportExpression.ts` L8-30):
/// `TS.Promise.new(function(resolve) resolve(TS.import(script, ...)) end)`.
pub fn transform_import_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let mut module_specifier = node.arguments().first().copied();
	let static_import = flamework_static_import_specifier(node);
	if static_import.is_some() {
		module_specifier = static_import;
	}

	let Some(module_specifier) = module_specifier.filter(|&specifier| tsast::is_string_literal(specifier)) else {
		state.diags.add(diagnostics::no_non_string_module_specifier(node));
		return state.ast.none();
	};

	let source_file = get_source_file_of_node(node).unwrap_or(state.source_file);
	let import_expression = create_import_expression(state, source_file, module_specifier);
	if static_import.is_some() {
		return import_expression;
	}
	let resolve_id = state.ast.id("resolve");

	let promise = state.runtime_lib(Some(node), "Promise");
	let promise_new = state.ast.property_access(promise, "new");
	let resolve_args = state.ast.list_from([import_expression]);
	let resolve_call = state.ast.call(resolve_id, resolve_args);
	let resolve_statement = state.ast.call_statement(resolve_call);
	let body = state.ast.list_from([resolve_statement]);
	let params = state.ast.list_from([resolve_id]);
	let callback = state.ast.function_expression(params, false, body);
	let args = state.ast.list_from([callback]);
	state.ast.call(promise_new, args)
}

/// `FlameworkStaticImportMarker`
pub const FLAMEWORK_STATIC_IMPORT_MARKER: &str = "sloptor:flamework-static-import";

fn flamework_static_import_specifier(node: P<Node>) -> Option<P<Node>> {
	let [argument] = node.arguments() else {
		return None;
	};
	if !tsast::is_binary_expression(*argument) {
		return None;
	}
	let binary = argument.as_binary_expression();
	if binary.operator_token.kind() != Kind::PlusToken
		|| !tsast::is_string_literal(binary.left)
		|| !tsast::is_string_literal(binary.right.get())
	{
		return None;
	}
	if binary.right.get().text() != FLAMEWORK_STATIC_IMPORT_MARKER {
		return None;
	}
	Some(binary.left)
}
