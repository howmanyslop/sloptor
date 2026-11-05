//! `reference_validation.go`: the checks a solution runs before it builds.
//!
//! Referenced projects must be composite and emit, output directories must not overlap sources or
//! each other, and a referenced project with its own Rojo config must mount its outputs where its
//! consumers do.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use rotor_rojo::{RojoResolver, RojoResolverCache, canonical_file_name};
use rotor_support::gopath;
use rotor_tsrs::tsoptions::outputpaths;
use rotor_tsrs::tspath;

use crate::compile::{DiagnosticInfo, Error, info_from_ts_diag};
use crate::project::{
	ProjectOutputHost, create_path_translator, is_path_descendant_of, lock, resolve_rojo_config_path,
};
use crate::rbxts_options::{RbxtsOptions, read_rbxts_options};
use crate::solution::{SolutionGraph, SolutionProject};
use crate::source_trace::DiagnosticTraces;

/// `referenceDiagnosticError`: a validation failure that carries a TypeScript diagnostic.
pub(crate) fn reference_diagnostic_error(diagnostic: DiagnosticInfo) -> Error {
	Error::message(diagnostic.message)
}

/// A solution validation failure: the error, and the diagnostic when it is a TypeScript one
/// (Go's `*referenceDiagnosticError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
	/// The error.
	pub error: Error,
	/// The TypeScript diagnostic behind it.
	pub diagnostic: Option<Box<DiagnosticInfo>>,
}

impl From<Error> for ValidationError {
	fn from(error: Error) -> Self {
		Self {
			error,
			diagnostic: None,
		}
	}
}

impl std::fmt::Display for ValidationError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.error.fmt(formatter)
	}
}

/// `validateSolutionReferences`: every referenced project that emits must be composite and may
/// not disable emit.
pub(crate) fn validate_solution_references(
	graph: &SolutionGraph,
	programs: &HashMap<String, &dyn ProjectOutputHost>,
) -> Result<(), ValidationError> {
	let references = graph
		.projects
		.iter()
		.map(|project| &project.references)
		.chain(graph.coordinator_config_chains.iter().map(|chain| &chain.references));
	for dependencies in references {
		for config_path in dependencies {
			let Some(program) = programs.get(config_path) else {
				continue;
			};
			let options = program.options();
			if options.out_dir.is_empty() {
				continue;
			}
			let message = if !options.composite.is_true() {
				Some(&rotor_tsrs::diagnostics::REFERENCED_PROJECT_0_MUST_HAVE_SETTING_COMPOSITE_COLON_TRUE)
			} else if options.no_emit.is_true() || options.declaration.is_false() {
				Some(&rotor_tsrs::diagnostics::REFERENCED_PROJECT_0_MAY_NOT_DISABLE_EMIT)
			} else {
				None
			};
			if let Some(message) = message {
				let diagnostic = rotor_tsrs::ast::new_compiler_diagnostic(message, &[config_path]);
				let info = info_from_ts_diag(diagnostic, &DiagnosticTraces::default());
				return Err(ValidationError {
					error: reference_diagnostic_error(info.clone()),
					diagnostic: Some(Box::new(info)),
				});
			}
		}
	}
	Ok(())
}

/// `projectRootDirs`: `rootDir`, else `rootDirs`.
pub(crate) fn project_root_dirs(program: &dyn ProjectOutputHost) -> Vec<String> {
	let options = program.options();
	if !options.root_dir.is_empty() {
		return vec![options.root_dir.clone()];
	}
	options.root_dirs.clone().unwrap_or_default()
}

/// `validateProjectOutputs`: no output directory holds a source directory, no two projects
/// share an output directory, and every build info file has one owner outside the others'
/// outputs.
pub(crate) fn validate_project_outputs(
	graph: &SolutionGraph,
	programs: &HashMap<String, &dyn ProjectOutputHost>,
) -> Result<(), Error> {
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let mut owners: Vec<(String, String)> = Vec::new();
	let mut build_info_owners: BTreeMap<String, (usize, String, String)> = BTreeMap::new();
	let mut sources = Vec::new();
	for project in &graph.projects {
		if let Some(program) = programs.get(&project.config_path) {
			sources.extend(project_root_dirs(*program));
		}
	}
	for project in &graph.projects {
		let Some(program) = programs.get(&project.config_path) else {
			continue;
		};
		let options = program.options();
		if options.out_dir.is_empty() {
			continue;
		}
		let translator = create_path_translator(*program, !project.options.lua_extension);
		let mut roots = vec![translator.out_dir.clone()];
		if options.get_emit_declarations() && !options.declaration_dir.is_empty() {
			roots.push(gopath::from_slash(&options.declaration_dir).into_owned());
		}
		for root in roots {
			for source in &sources {
				let source = gopath::from_slash(source);
				if is_path_descendant_of(&source, &root) {
					return Err(Error::message(format!(
						r#"Output directory "{root}" contains source directory "{source}"."#
					)));
				}
			}
			for (owner_path, owner_config) in &owners {
				if *owner_config != project.config_path
					&& (is_path_descendant_of(&root, owner_path) || is_path_descendant_of(owner_path, &root))
				{
					return Err(Error::message(format!(
						r#"Projects "{owner_config}" and "{}" have overlapping output directories. Use separate outDir and declarationDir paths."#,
						project.config_path
					)));
				}
			}
			owners.push((root, project.config_path.clone()));
		}
		let ts_build_info_path = outputpaths::get_build_info_file_name(
			&options,
			&tspath::ComparePathsOptions {
				current_directory: gopath::dir(&project.config_path),
				use_case_sensitive_file_names: case_sensitive,
			},
		);
		for path in [
			translator.build_info_output_path.clone(),
			gopath::from_slash(&ts_build_info_path).into_owned(),
		] {
			if path.is_empty() {
				continue;
			}
			let key = canonical_file_name(&path, case_sensitive);
			if build_info_owners.contains_key(&key) {
				return Err(Error::message(format!(
					r#"Multiple projects write "{path}". Set a unique tsBuildInfoFile for each project."#
				)));
			}
			let order = build_info_owners.len();
			build_info_owners.insert(key.clone(), (order, key, project.config_path.clone()));
		}
	}
	let mut ordered: Vec<&(usize, String, String)> = build_info_owners.values().collect();
	ordered.sort_by_key(|(order, _, _)| *order);
	for (_, path, config_path) in ordered {
		for (owner_path, owner_config) in &owners {
			let root_key = canonical_file_name(owner_path, case_sensitive);
			if owner_config != config_path
				&& is_path_descendant_of(&gopath::from_slash(path), &gopath::from_slash(&root_key))
			{
				return Err(Error::message(format!(
					r#"Build info "{path}" is inside another project's output directory "{owner_path}"."#
				)));
			}
		}
	}
	Ok(())
}

/// `referenceRojoCache`: the Rojo resolvers and rbxts options the mount checks of a solution
/// share, so a watch cycle does not walk the Rojo tree or read every reference's config again.
pub struct ReferenceRojoCache {
	pub(crate) resolvers: Arc<RojoResolverCache>,
	/// rotor-rs: without a shared cache, each Rojo configuration's resolver comes from the disk
	/// cache its project's builds keep beside it (`.rotor/cache/rojo`), read but never written,
	/// where Go parses every configuration again.
	disk: Option<Mutex<HashMap<String, Arc<RojoResolverCache>>>>,
	options: Mutex<HashMap<String, Result<Option<RbxtsOptions>, Error>>>,
}

impl ReferenceRojoCache {
	/// `newReferenceRojoCache`
	pub(crate) fn new(resolvers: Option<Arc<RojoResolverCache>>) -> Self {
		Self {
			disk: resolvers.is_none().then(Mutex::default),
			resolvers: resolvers.unwrap_or_else(|| Arc::new(RojoResolverCache::memory())),
			options: Mutex::new(HashMap::new()),
		}
	}

	/// `resolver`
	pub(crate) fn resolver(&self, path: &str) -> Arc<RojoResolver> {
		let Some(disk) = &self.disk else {
			return self.resolvers.load(path);
		};
		let directory = gopath::dir(path);
		let cache = Arc::clone(lock(disk).entry(directory.clone()).or_insert_with(|| {
			Arc::new(RojoResolverCache::with_deferred_persist(
				&gopath::join(&[&directory, ".rotor", "cache", "rojo"]),
				&crate::copyfilesgate::compiler_version(),
				true,
			))
		}));
		cache.load(path)
	}

	/// `rbxtsOptions`
	pub(crate) fn rbxts_options(&self, config_path: &str) -> Result<Option<RbxtsOptions>, Error> {
		lock(&self.options)
			.entry(config_path.to_owned())
			.or_insert_with(|| read_rbxts_options(config_path))
			.clone()
	}

	/// `forget`
	pub(crate) fn forget(&self, projects: &[SolutionProject]) {
		let mut options = lock(&self.options);
		for project in projects {
			options.remove(&project.config_path);
		}
	}
}

fn cached_resolver(cache: Option<&ReferenceRojoCache>, path: &str) -> Arc<RojoResolver> {
	cache.map_or_else(|| Arc::new(RojoResolver::from_path(path)), |cache| cache.resolver(path))
}

fn cached_rbxts_options(cache: Option<&ReferenceRojoCache>, config_path: &str) -> Result<Option<RbxtsOptions>, Error> {
	cache.map_or_else(
		|| read_rbxts_options(config_path),
		|cache| cache.rbxts_options(config_path),
	)
}

/// `validateReferenceRojoPaths`
pub(crate) fn validate_reference_rojo_paths(
	graph: &SolutionGraph,
	outputs: &HashMap<String, Vec<String>>,
	cache: Option<&ReferenceRojoCache>,
) -> Result<(), Error> {
	validate_projects_reference_rojo_paths(graph, &graph.projects, outputs, cache)
}

/// `validateProjectsReferenceRojoPaths`: the direct references of `consumers` only, so one
/// project's mounts never fail an unrelated project.
pub(crate) fn validate_projects_reference_rojo_paths(
	graph: &SolutionGraph,
	consumers: &[SolutionProject],
	outputs: &HashMap<String, Vec<String>>,
	cache: Option<&ReferenceRojoCache>,
) -> Result<(), Error> {
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let mut needed = std::collections::HashSet::new();
	for project in consumers {
		needed.insert(project.config_path.as_str());
		for config_path in &project.direct_references {
			needed.insert(config_path.as_str());
		}
	}
	let mut resolvers: HashMap<&str, Arc<RojoResolver>> = HashMap::new();
	let mut resolver_paths: HashMap<&str, String> = HashMap::new();
	let mut by_rojo_path: HashMap<String, Arc<RojoResolver>> = HashMap::new();
	for project in &graph.projects {
		if !needed.contains(project.config_path.as_str()) {
			continue;
		}
		let (path, _) =
			resolve_rojo_config_path(&gopath::dir(&project.config_path), &project.options.rojo_config_path)?;
		let path = path.unwrap_or_default();
		resolver_paths.insert(&project.config_path, path.clone());
		if path.is_empty() {
			continue;
		}
		let key = canonical_file_name(&path, case_sensitive);
		let resolver = by_rojo_path
			.entry(key)
			.or_insert_with(|| cached_resolver(cache, &path))
			.clone();
		resolvers.insert(&project.config_path, resolver);
	}
	// Upstream checks only direct references; a solution config has no path translator, so
	// projects reached through one are skipped.
	for project in consumers {
		for config_path in &project.direct_references {
			let emitted = outputs.get(config_path);
			let owner = resolvers.get(config_path.as_str());
			let consumer = resolvers.get(project.config_path.as_str());
			let declared = cached_rbxts_options(cache, config_path)?;
			let (Some(paths), Some(owner)) = (emitted, owner) else {
				continue;
			};
			if consumer.is_some_and(|consumer| Arc::ptr_eq(owner, consumer)) {
				continue;
			}
			if declared
				.as_ref()
				.and_then(|declared| declared.rojo.as_ref())
				.is_none_or(String::is_empty)
			{
				continue;
			}
			for path in paths {
				let owner_path = owner.get_rbx_path_from_file_path(path);
				let consumer_path = consumer.and_then(|consumer| consumer.get_rbx_path_from_file_path(path));
				if owner_path == consumer_path {
					continue;
				}
				let mount = owner_path.map_or_else(|| "unmapped".to_owned(), |path| path.join("."));
				return Err(Error::message(format!(
					r#"Project "{}" must mount "{path}" at the same Roblox path as "{}" ({mount}). Referenced projects with their own rbxts.rojo require consistent module and runtime mounts."#,
					project.config_path,
					resolver_paths.get(config_path.as_str()).map_or("", String::as_str),
				)));
			}
		}
	}
	Ok(())
}
