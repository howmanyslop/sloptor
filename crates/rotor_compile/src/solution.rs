//! `solution.go`: the solution graph and its coordinator.
//!
//! [`build_solution_graph_for_roots`] discovers the dependency-first union of the roots' project
//! references, one canonical identity per physical config. [`SolutionCoordinator`] owns the
//! graph, each project's last outcome and the cross-project metadata; `solution_drain` builds it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rotor_rojo::RojoResolver;
use rotor_support::gopath;
use rotor_transformer::ProjectType;

use crate::compile::Error;
use crate::crossproject::{CrossProjectMetadata, SolutionWriteMetadata, new_cross_project_metadata};
use crate::output::BuildResult;
use crate::project::{
	ProjectOptions, project_is_package, referenced_project_options, resolve_include_path, resolve_rojo_config_path,
};
use crate::project_references::read_project_reference_paths;
use crate::rbxts_options::read_rbxts_options;
use crate::reference_validation::{ReferenceRojoCache, ValidationError};
use crate::solution_drain::{Drainer, SolutionBuildDrainer, effective_solution_builders};
use crate::solution_watch::SolutionWatchSet;
use crate::timings::BuildTimings;

/// `SolutionProject`: one emitting project of a solution.
#[derive(Clone, Default)]
pub struct SolutionProject {
	/// The canonical config path.
	pub config_path: String,
	/// The emitting projects this one references, coordinators expanded.
	pub references: Vec<String>,
	/// The project's effective options.
	pub options: ProjectOptions,
	/// Whether this config only coordinates references.
	pub coordinator: bool,
	/// The references as written, before coordinators are replaced by their emitting references.
	pub(crate) direct_references: Vec<String>,
}

/// `SolutionGraph`: the emitting projects, dependencies first, and the coordinator configs.
#[derive(Clone, Default)]
pub struct SolutionGraph {
	/// The emitting projects, dependencies first.
	pub projects: Vec<SolutionProject>,
	pub(crate) coordinator_config_chains: Vec<SolutionConfigChain>,
}

/// `solutionConfigChain`: a coordinator config, the configs it extends, and its references.
#[derive(Clone, Debug, Default)]
pub(crate) struct SolutionConfigChain {
	pub(crate) project_path: String,
	pub(crate) config_paths: Vec<String>,
	pub(crate) references: Vec<String>,
}

/// What a drainer returns for one project: Go's `(*BuildResult, []string, error)`.
#[derive(Clone, Debug, Default)]
pub struct DrainResult {
	/// The build result; `None` is Go's nil.
	pub result: Option<BuildResult>,
	/// The failure's messages.
	pub messages: Vec<String>,
	/// The error; `None` on success.
	pub error: Option<Error>,
}

/// `SolutionProjectDrainer`: builds one project of a solution.
pub trait SolutionProjectDrainer: Send + Sync {
	/// `Drain`
	fn drain(&self, project: SolutionProject) -> DrainResult;

	/// Whether the coordinator publishes the persistence this drainer's builds defer once the
	/// whole solution succeeded, as Go's build drainer (and drainers embedding it) does.
	fn queues_persistence(&self) -> bool {
		false
	}
}

/// `SolutionProjectState`: a project's last outcome.
#[derive(Clone)]
pub struct SolutionProjectState {
	/// The project.
	pub project: SolutionProject,
	/// The last build's result.
	pub result: Option<BuildResult>,
	/// Whether the last build succeeded and nothing invalidated it since.
	pub up_to_date: bool,
	/// The failed dependency that blocked the last build.
	pub blocked_by: String,
	/// The last build's error.
	pub err: Option<Error>,
}

/// `SolutionProjectStatus`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolutionProjectStatus {
	/// `success`
	Success,
	/// `no-change`
	NoChange,
	/// `satisfied`
	Satisfied,
	/// `blocked`
	Blocked,
	/// `failed`
	Failed,
}

impl SolutionProjectStatus {
	/// The status's spelling.
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Success => "success",
			Self::NoChange => "no-change",
			Self::Satisfied => "satisfied",
			Self::Blocked => "blocked",
			Self::Failed => "failed",
		}
	}
}

/// `SolutionProjectSelection`: an external scheduler's ownership decision for one drain. `None`
/// builds the whole discovered graph.
#[derive(Clone, Debug, Default)]
pub struct SolutionProjectSelection {
	/// The configs this drain builds.
	pub selected: Vec<String>,
	/// The configs whose restored outputs this drain reads.
	pub satisfied: Vec<String>,
}

/// `SolutionProjectResult`: the terminal outcome of one emitted config.
#[derive(Clone, Debug)]
pub struct SolutionProjectResult {
	/// The config path.
	pub config_path: String,
	/// The outcome.
	pub status: SolutionProjectStatus,
	/// The dependency outcomes that kept this project from running.
	pub blockers: Vec<String>,
	/// The project's diagnostics; blocked projects carry none.
	pub diagnostics: Vec<crate::compile::DiagnosticInfo>,
	/// The outputs the project owns, slash-separated and sorted.
	pub outputs: Vec<String>,
	/// `len(outputs)`
	pub output_count: usize,
	/// The project's timings.
	pub timings: crate::timings::ProjectBuildTimings,
}

/// `SolutionRoot`: one requested root and its effective entry options.
#[derive(Clone)]
pub struct SolutionRoot {
	/// The root config.
	pub config_path: String,
	/// The root's options.
	pub options: ProjectOptions,
}

/// `SolutionCoordinator`
pub struct SolutionCoordinator {
	pub(crate) graph: SolutionGraph,
	pub(crate) drainer: Drainer,
	pub(crate) states: HashMap<String, SolutionProjectState>,
	pub(crate) project_paths: HashMap<String, String>,
	pub(crate) coordinator_paths: HashMap<String, String>,
	pub(crate) write_roots: HashMap<String, Vec<String>>,
	pub(crate) wait_only_dependencies: HashMap<String, Vec<String>>,
	pub(crate) reference_outputs: HashMap<String, Vec<String>>,
	pub(crate) metadata: CrossProjectMetadata,
	/// Projects invalidated since the last metadata refresh.
	pub(crate) pending_projects: HashSet<String>,
	/// The cached project infos changed, but the solution metadata was not derived from them yet.
	pub(crate) metadata_stale: bool,
	/// Each project's watch set until it is invalidated.
	pub(crate) watch_sets: HashMap<String, SolutionWatchSet>,
	pub(crate) builders: i64,
	pub(crate) timings: Option<Arc<BuildTimings>>,
	/// The persistence of successful projects, published when a drain succeeds.
	pub(crate) persist_queue: std::sync::Mutex<crate::solution_drain::PersistQueue>,
	/// A test drainer's `validateSatisfiedProject` (Go's optional drainer interface).
	#[cfg(test)]
	pub(crate) satisfied_validator: Option<crate::solution_drain::SatisfiedValidator>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
	Visiting,
	Visited,
}

/// `BuildSolutionGraph`
///
/// # Errors
///
/// A config does not resolve, read or parse, a reference cycle, or conflicting effective options.
pub fn build_solution_graph(ts_config_path: &str, entry: &ProjectOptions) -> Result<SolutionGraph, Error> {
	build_solution_graph_for_roots(&[ts_config_path.to_owned()], entry)
}

/// `BuildSolutionGraphForRoots`: the dependency-first union of `roots`. Each physical config has
/// one identity even when roots or references reach it through lexical, symlink or case aliases.
///
/// # Errors
///
/// See [`build_solution_graph`].
pub fn build_solution_graph_for_roots(
	ts_config_paths: &[String],
	entry: &ProjectOptions,
) -> Result<SolutionGraph, Error> {
	let roots: Vec<SolutionRoot> = ts_config_paths
		.iter()
		.map(|config_path| SolutionRoot {
			config_path: config_path.clone(),
			options: entry.clone(),
		})
		.collect();
	build_graph(&roots)
}

struct GraphBuilder {
	projects: HashMap<String, SolutionProject>,
	config_paths_by_project: HashMap<String, Vec<String>>,
	root_by_project: HashMap<String, String>,
	order: Vec<String>,
	ordered: HashSet<String>,
}

struct RootContext<'a> {
	options: &'a ProjectOptions,
	root_path: String,
	root_is_coordinator: bool,
	root_rojo_path: String,
	root_include_path: String,
	coordinator_rbxts: Option<crate::rbxts_options::RbxtsOptions>,
}

fn build_graph(roots: &[SolutionRoot]) -> Result<SolutionGraph, Error> {
	if roots.is_empty() {
		return Err(Error::message("compile: solution requires at least one root"));
	}
	let mut builder = GraphBuilder {
		projects: HashMap::new(),
		config_paths_by_project: HashMap::new(),
		root_by_project: HashMap::new(),
		order: Vec::new(),
		ordered: HashSet::new(),
	};
	for root in roots {
		let (root_path, _) = canonical_solution_config_path(&root.config_path).map_err(|error| {
			Error::message(format!(
				"compile: resolve solution config {}: {error}",
				rotor_support::gostr::quote(&root.config_path)
			))
		})?;
		let (_, root_is_coordinator, _) = read_project_reference_paths(&root_path).map_err(|error| {
			Error::message(format!(
				"compile: read project reference {}: {error}",
				rotor_support::gostr::quote(&root_path)
			))
		})?;
		let root_dir = gopath::dir(&root_path);
		let (root_rojo_path, _) = resolve_rojo_config_path(&root_dir, &root.options.rojo_config_path)?;
		let root_include_path = resolve_include_path(&root_dir, &root.options.include_path)?;
		let coordinator_rbxts = if root_is_coordinator && root.options.solution_argv.is_some() {
			read_rbxts_options(&root_path).map_err(|error| {
				Error::message(format!(
					"compile: read solution options {}: {error}",
					rotor_support::gostr::quote(&root_path)
				))
			})?
		} else {
			None
		};
		let context = RootContext {
			options: &root.options,
			root_path: root_path.clone(),
			root_is_coordinator,
			root_rojo_path: root_rojo_path.unwrap_or_default(),
			root_include_path,
			coordinator_rbxts,
		};
		let mut visits = HashMap::new();
		let mut stack = Vec::new();
		builder.visit(&context, &root_path, true, &mut visits, &mut stack)?;
	}

	let projects_by_path: HashMap<String, SolutionProject> = builder
		.projects
		.values()
		.map(|project| (project.config_path.clone(), project.clone()))
		.collect();
	let mut graph = SolutionGraph::default();
	for config_key in &builder.order {
		let mut project = builder.projects[config_key].clone();
		if project.coordinator {
			graph.coordinator_config_chains.push(SolutionConfigChain {
				project_path: project.config_path.clone(),
				config_paths: builder.config_paths_by_project[config_key].clone(),
				references: project.references.clone(),
			});
			continue;
		}
		project.direct_references.clone_from(&project.references);
		project.references = emitting_project_references(&project.references, &projects_by_path);
		graph.projects.push(project);
	}
	Ok(graph)
}

impl GraphBuilder {
	fn visit(
		&mut self,
		root: &RootContext<'_>,
		candidate: &str,
		is_root: bool,
		visits: &mut HashMap<String, Visit>,
		stack: &mut Vec<String>,
	) -> Result<String, Error> {
		let (config_path, config_key) = canonical_solution_config_path(candidate).map_err(|error| {
			Error::message(format!(
				"compile: resolve project reference {}: {error}",
				rotor_support::gostr::quote(candidate)
			))
		})?;
		match visits.get(&config_key) {
			Some(Visit::Visiting) => return Err(solution_cycle_error(stack, &config_path)),
			Some(Visit::Visited) => return Ok(self.projects[&config_key].config_path.clone()),
			None => {}
		}
		visits.insert(config_key.clone(), Visit::Visiting);
		stack.push(config_path.clone());
		let result = self.visit_project(root, config_path, &config_key, is_root, visits, stack);
		stack.pop();
		let config_path = result?;
		visits.insert(config_key.clone(), Visit::Visited);
		if self.ordered.insert(config_key.clone()) {
			self.order.push(config_key);
		}
		Ok(config_path)
	}

	fn visit_project(
		&mut self,
		root: &RootContext<'_>,
		config_path: String,
		config_key: &str,
		is_root: bool,
		visits: &mut HashMap<String, Visit>,
		stack: &mut Vec<String>,
	) -> Result<String, Error> {
		let (references, coordinator, config_paths) = read_project_reference_paths(&config_path).map_err(|error| {
			Error::message(format!(
				"compile: read project reference {}: {error}",
				rotor_support::gostr::quote(&config_path)
			))
		})?;
		let mut options = root.options.clone();
		if !is_root {
			// Derive from the root entry, not the referencing project: a project's salt must not
			// depend on which intermediate reached it.
			options = referenced_project_options(
				root.options,
				&config_path,
				root.root_is_coordinator,
				root.coordinator_rbxts.as_ref(),
			)
			.map_err(|error| {
				Error::message(format!(
					"compile: read referenced project options {}: {error}",
					rotor_support::gostr::quote(&config_path)
				))
			})?;
			let declared = read_rbxts_options(&config_path)?;
			let dir = gopath::dir(&config_path);
			let is_package = project_is_package(&dir).is_ok_and(|(_, is_package)| is_package);
			// Non-game projects and projects declaring their own Rojo keep the same context whether
			// reached as a root or as a reference.
			let mut uses_own_context = declared.as_ref().is_some_and(|declared| declared.rojo.is_some())
				|| matches!(options.project_type, Some(ProjectType::Package | ProjectType::Model))
				|| options.project_type.is_none() && is_package;
			if !uses_own_context && options.project_type.is_none() {
				let (own_rojo_path, _) = resolve_rojo_config_path(&dir, &options.rojo_config_path)?;
				uses_own_context = own_rojo_path.is_some_and(|path| !RojoResolver::from_path(&path).is_game);
			}
			if !uses_own_context {
				options.rojo_config_path.clone_from(&root.root_rojo_path);
			}
			if !uses_own_context && !root.root_rojo_path.is_empty() {
				options.include_path.clone_from(&root.root_include_path);
			}
		}

		if let Some(existing) = self.projects.get(config_key) {
			if !same_solution_project_options(&existing.options, &options) {
				return Err(solution_options_conflict(
					&existing.config_path,
					&self.root_by_project[config_key],
					&root.root_path,
					&existing.options,
					&options,
				));
			}
		} else {
			self.projects.insert(
				config_key.to_owned(),
				SolutionProject {
					config_path,
					references: Vec::new(),
					options,
					coordinator,
					direct_references: Vec::new(),
				},
			);
			self.config_paths_by_project.insert(config_key.to_owned(), config_paths);
			self.root_by_project
				.insert(config_key.to_owned(), root.root_path.clone());
		}

		let mut canonical_references = Vec::with_capacity(references.len());
		for reference in references {
			canonical_references.push(self.visit(root, &reference, false, visits, stack)?);
		}
		let project = self
			.projects
			.get_mut(config_key)
			.ok_or_else(|| Error::message("compile: solution project vanished during discovery"))?;
		project.references = canonical_references;
		Ok(project.config_path.clone())
	}
}

/// `canonicalSolutionConfigPath`: the absolute spelling of a config (its physical spelling when
/// a component is a symlink) and its identity key.
pub(crate) fn canonical_solution_config_path(path: &str) -> Result<(String, String), Error> {
	let absolute = gopath::abs(&gopath::from_slash(path)).map_err(|error| Error::message(error.to_string()))?;
	let mut canonical = gopath::clean(&absolute);
	let identity = gopath::clean(&gopath::from_slash(&crate::sidecar::realpath_or(&canonical)));
	if path_has_explicit_symlink(&canonical) {
		canonical.clone_from(&identity);
	}
	let key = if rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names() {
		identity
	} else {
		identity.to_lowercase()
	};
	Ok((canonical, key))
}

/// `canonicalExplicitPath`: spells a user-given path like solution config paths, so it compares
/// equal to paths derived from them.
pub(crate) fn canonical_explicit_path(path: &str) -> String {
	if !path_has_explicit_symlink(path) {
		return path.to_owned();
	}
	gopath::clean(&gopath::from_slash(&crate::sidecar::realpath_or(path)))
}

/// `pathHasExplicitSymlink`: whether any component of `path` is a symbolic link.
pub(crate) fn path_has_explicit_symlink(path: &str) -> bool {
	let separator = gopath::NATIVE.separator();
	let volume = gopath::volume_name(path);
	let mut current = format!("{volume}{separator}");
	let remainder = path.strip_prefix(&current).unwrap_or(path);
	for part in remainder.split(separator) {
		if part.is_empty() {
			continue;
		}
		current = gopath::join(&[&current, part]);
		match std::fs::symlink_metadata(&current) {
			Ok(metadata) if metadata.file_type().is_symlink() => return true,
			Ok(_) => {}
			Err(_) => return false,
		}
	}
	false
}

/// `sameSolutionProjectOptions`: equal options, ignoring the per-build plumbing.
fn same_solution_project_options(left: &ProjectOptions, right: &ProjectOptions) -> bool {
	left.include_path == right.include_path
		&& left.emit_include_files == right.emit_include_files
		&& left.project_type == right.project_type
		&& left.checkers == right.checkers
		&& left.builders == right.builders
		&& left.single_threaded == right.single_threaded
		&& left.rojo_config_path == right.rojo_config_path
		&& left.log_truthy_changes == right.log_truthy_changes
		&& left.allow_comment_directives == right.allow_comment_directives
		&& left.skip_semantic_diagnostics == right.skip_semantic_diagnostics
		&& left.optimized_array_appends == right.optimized_array_appends
		&& left.native_types == right.native_types
		&& left.luau_types == right.luau_types
		&& left.no_optimized_loops == right.no_optimized_loops
		&& left.lua_extension == right.lua_extension
		&& left.write_only_changed == right.write_only_changed
		&& left.emit_declaration_only == right.emit_declaration_only
		&& left.overlays == right.overlays
		&& left.ignore_unmatched_overlays == right.ignore_unmatched_overlays
}

/// `solutionOptionsConflict`
fn solution_options_conflict(
	config_path: &str,
	first_root: &str,
	second_root: &str,
	first: &ProjectOptions,
	second: &ProjectOptions,
) -> Error {
	let dir = gopath::dir(config_path);
	let rojo = |options: &ProjectOptions| {
		resolve_rojo_config_path(&dir, &options.rojo_config_path)
			.ok()
			.and_then(|(path, _)| path)
			.unwrap_or_default()
	};
	let include = |options: &ProjectOptions| resolve_include_path(&dir, &options.include_path).unwrap_or_default();
	Error::message(format!(
		r#"compile: conflicting effective options for {config_path} reached from {first_root} (Rojo "{}", include "{}") and {second_root} (Rojo "{}", include "{}")"#,
		rojo(first),
		include(first),
		rojo(second),
		include(second),
	))
}

/// `emittingProjectReferences`: references with every coordinator replaced by its emitting
/// references, each once.
fn emitting_project_references(references: &[String], projects: &HashMap<String, SolutionProject>) -> Vec<String> {
	fn append(
		config_path: &str,
		projects: &HashMap<String, SolutionProject>,
		result: &mut Vec<String>,
		seen: &mut HashSet<String>,
		expanded: &mut HashSet<String>,
	) {
		match projects.get(config_path) {
			Some(project) if project.coordinator => {
				if !expanded.insert(config_path.to_owned()) {
					return;
				}
				for reference in &project.references {
					append(reference, projects, result, seen, expanded);
				}
			}
			_ => {
				if seen.insert(config_path.to_owned()) {
					result.push(config_path.to_owned());
				}
			}
		}
	}
	let mut result = Vec::with_capacity(references.len());
	let mut seen = HashSet::new();
	let mut expanded = HashSet::new();
	for reference in references {
		append(reference, projects, &mut result, &mut seen, &mut expanded);
	}
	result
}

/// `solutionCycleError`
fn solution_cycle_error(stack: &[String], repeated: &str) -> Error {
	let start = stack
		.iter()
		.position(|config_path| config_path == repeated)
		.unwrap_or(0);
	let mut cycle: Vec<&str> = stack[start..].iter().map(String::as_str).collect();
	cycle.push(repeated);
	Error::message(format!("compile: circular project reference [{}]", cycle.join(" ")))
}

impl SolutionCoordinator {
	/// `NewSolutionCoordinator`
	///
	/// # Errors
	///
	/// The graph does not build, or the solution fails validation.
	pub fn new(ts_config_path: &str, entry: &ProjectOptions) -> Result<Self, ValidationError> {
		Self::for_roots(&[ts_config_path.to_owned()], entry)
	}

	/// `NewSolutionCoordinatorWithDrainer`
	///
	/// # Errors
	///
	/// The graph does not build.
	pub fn with_drainer(
		ts_config_path: &str,
		entry: &ProjectOptions,
		drainer: Arc<dyn SolutionProjectDrainer>,
	) -> Result<Self, ValidationError> {
		Self::for_roots_with_drainer(&[ts_config_path.to_owned()], entry, drainer)
	}

	/// `NewSolutionCoordinatorForRoots`
	///
	/// # Errors
	///
	/// See [`SolutionCoordinator::new`].
	pub fn for_roots(ts_config_paths: &[String], entry: &ProjectOptions) -> Result<Self, ValidationError> {
		let roots: Vec<SolutionRoot> = ts_config_paths
			.iter()
			.map(|config_path| SolutionRoot {
				config_path: config_path.clone(),
				options: entry.clone(),
			})
			.collect();
		Self::for_root_options(&roots)
	}

	/// `NewSolutionCoordinatorForRootOptions`
	///
	/// # Errors
	///
	/// See [`SolutionCoordinator::new`].
	pub fn for_root_options(roots: &[SolutionRoot]) -> Result<Self, ValidationError> {
		let mut graph = build_graph(roots)?;
		let (mut metadata, cache) = new_cross_project_metadata(&mut graph);
		if let Some(error) = metadata.validation_err.take() {
			return Err(error);
		}
		let drainer = Drainer::Build(SolutionBuildDrainer::new(
			&graph,
			std::mem::take(&mut metadata.restored_metadata_errs),
		));
		let builders = roots.first().map_or_else(
			|| effective_solution_builders(&ProjectOptions::default()),
			|root| effective_solution_builders(&root.options),
		);
		let timings = roots.first().and_then(|root| root.options.timings.clone());
		Ok(Self::assemble(graph, drainer, metadata, cache, builders, timings)?)
	}

	/// `NewSolutionCoordinatorForRootsWithDrainer`
	///
	/// # Errors
	///
	/// The graph does not build.
	pub fn for_roots_with_drainer(
		ts_config_paths: &[String],
		entry: &ProjectOptions,
		drainer: Arc<dyn SolutionProjectDrainer>,
	) -> Result<Self, ValidationError> {
		let mut graph = build_solution_graph_for_roots(ts_config_paths, entry)?;
		let (metadata, cache) = new_cross_project_metadata(&mut graph);
		Ok(Self::assemble(
			graph,
			Drainer::Custom(drainer),
			metadata,
			cache,
			effective_solution_builders(entry),
			entry.timings.clone(),
		)?)
	}

	/// `newSolutionCoordinator`
	pub(crate) fn assemble(
		graph: SolutionGraph,
		drainer: Drainer,
		metadata: SolutionWriteMetadata,
		cache: CrossProjectMetadata,
		builders: i64,
		timings: Option<Arc<BuildTimings>>,
	) -> Result<Self, Error> {
		let mut states = HashMap::with_capacity(graph.projects.len());
		let mut project_paths = HashMap::with_capacity(graph.projects.len());
		for project in &graph.projects {
			states.insert(
				project.config_path.clone(),
				SolutionProjectState {
					project: project.clone(),
					result: None,
					up_to_date: false,
					blocked_by: String::new(),
					err: None,
				},
			);
			let (_, key) = canonical_solution_config_path(&project.config_path)?;
			project_paths.insert(key, project.config_path.clone());
		}
		if let Some(timings) = &timings {
			let config_paths: Vec<String> = graph
				.projects
				.iter()
				.map(|project| project.config_path.clone())
				.collect();
			timings.init_projects(&config_paths);
			timings.set_concurrency_metadata(
				usize::try_from(builders).unwrap_or(0),
				graph.projects.first().and_then(|project| project.options.builders),
				graph.projects.first().and_then(|project| project.options.checkers),
			);
		}
		Ok(Self {
			coordinator_paths: solution_coordinator_paths(&graph),
			graph,
			drainer,
			states,
			project_paths,
			write_roots: metadata.write_roots,
			wait_only_dependencies: metadata.wait_only_dependencies,
			reference_outputs: metadata.reference_outputs,
			metadata: cache,
			pending_projects: HashSet::new(),
			metadata_stale: false,
			watch_sets: HashMap::new(),
			builders,
			timings,
			persist_queue: std::sync::Mutex::default(),
			#[cfg(test)]
			satisfied_validator: None,
		})
	}

	/// The solution graph.
	#[must_use]
	pub const fn graph(&self) -> &SolutionGraph {
		&self.graph
	}

	/// `ProjectState`
	#[must_use]
	pub fn project_state(&self, ts_config_path: &str) -> Option<SolutionProjectState> {
		let (_, key) = canonical_solution_config_path(ts_config_path).ok()?;
		let config_path = self.project_paths.get(&key)?;
		self.states.get(config_path).cloned()
	}

	/// `Invalidate`: marks the projects at `paths`, their dependants, and the emitting projects
	/// of a coordinator at `paths` out of date. Returns them in graph order.
	pub fn invalidate(&mut self, paths: &[String]) -> Vec<String> {
		let mut references: HashMap<String, Vec<String>> = HashMap::new();
		for project in &self.graph.projects {
			references.insert(project.config_path.clone(), project.references.clone());
		}
		for chain in &self.graph.coordinator_config_chains {
			references.insert(chain.project_path.clone(), chain.references.clone());
		}
		let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
		for (config_path, project_references) in &references {
			for reference in project_references {
				reverse.entry(reference.clone()).or_default().push(config_path.clone());
			}
		}
		let mut direct = HashSet::new();
		let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
		let mut add_direct = |config_path: &str, queue: &mut std::collections::VecDeque<String>| {
			if direct.insert(config_path.to_owned()) {
				queue.push_back(config_path.to_owned());
			}
		};
		for path in paths {
			let Ok((mut config_path, key)) = canonical_solution_config_path(path) else {
				continue;
			};
			if let Some(project_path) = self.project_paths.get(&key) {
				config_path.clone_from(project_path);
			}
			if self.states.contains_key(&config_path) {
				add_direct(&config_path, &mut queue);
				continue;
			}
			let Some(coordinator_references) = references.get(&config_path) else {
				continue;
			};
			let mut descendants: std::collections::VecDeque<String> = coordinator_references.iter().cloned().collect();
			let mut descendant_seen = HashSet::new();
			while let Some(descendant) = descendants.pop_front() {
				if !descendant_seen.insert(descendant.clone()) {
					continue;
				}
				if self.states.contains_key(&descendant) {
					add_direct(&descendant, &mut queue);
				}
				if let Some(next) = references.get(&descendant) {
					descendants.extend(next.iter().cloned());
				}
			}
		}
		let mut seen = HashSet::new();
		while let Some(path) = queue.pop_front() {
			if !seen.insert(path.clone()) {
				continue;
			}
			if let Some(dependants) = reverse.get(&path) {
				queue.extend(dependants.iter().cloned());
			}
		}
		let mut result = Vec::new();
		for project in &self.graph.projects {
			if !seen.contains(&project.config_path) {
				continue;
			}
			if let Some(state) = self.states.get_mut(&project.config_path) {
				state.up_to_date = false;
				state.blocked_by.clear();
				state.err = None;
			}
			self.pending_projects.insert(project.config_path.clone());
			self.watch_sets.remove(&project.config_path);
			result.push(project.config_path.clone());
		}
		result
	}

	/// `Reload`: rediscovers the graph, keeping the states of projects that remain.
	///
	/// # Errors
	///
	/// The graph does not build, or a build coordinator's solution fails validation.
	pub fn reload(&mut self, ts_config_path: &str, entry: &ProjectOptions) -> Result<(), ValidationError> {
		let graph = build_solution_graph(ts_config_path, entry)?;
		self.reload_graph(graph, entry)
	}

	/// `reload`
	pub(crate) fn reload_graph(
		&mut self,
		mut graph: SolutionGraph,
		entry: &ProjectOptions,
	) -> Result<(), ValidationError> {
		// Derive into a copy so a failed reload keeps the current metadata.
		let mut cache = CrossProjectMetadata {
			infos: HashMap::new(),
			references: Arc::new(ReferenceRojoCache::new(Some(Arc::clone(
				&self.metadata.references.resolvers,
			)))),
			config_parses: self.metadata.config_parses,
			refreshes: self.metadata.refreshes,
		};
		let mut metadata = cache.rebuild(&mut graph);
		let build_drainer = matches!(self.drainer, Drainer::Build(_));
		if build_drainer && let Some(error) = metadata.validation_err.take() {
			return Err(error);
		}
		let mut states = HashMap::with_capacity(graph.projects.len());
		let mut project_paths = HashMap::with_capacity(graph.projects.len());
		for project in &graph.projects {
			let (_, key) = canonical_solution_config_path(&project.config_path)?;
			let previous = self
				.project_paths
				.get(&key)
				.and_then(|previous_path| self.states.get(previous_path))
				.cloned();
			let state = match previous {
				Some(mut state) => {
					state.project = project.clone();
					state
				}
				None => SolutionProjectState {
					project: project.clone(),
					result: None,
					up_to_date: false,
					blocked_by: String::new(),
					err: None,
				},
			};
			states.insert(project.config_path.clone(), state);
			project_paths.insert(key, project.config_path.clone());
		}
		self.coordinator_paths = solution_coordinator_paths(&graph);
		if build_drainer {
			self.drainer = Drainer::Build(SolutionBuildDrainer::new(
				&graph,
				std::mem::take(&mut metadata.restored_metadata_errs),
			));
			// Go's fresh build drainer starts without the persistence the old one queued.
			*crate::project::lock(&self.persist_queue) = crate::solution_drain::PersistQueue::default();
		}
		self.graph = graph;
		self.states = states;
		self.project_paths = project_paths;
		self.write_roots = metadata.write_roots;
		self.wait_only_dependencies = metadata.wait_only_dependencies;
		self.reference_outputs = metadata.reference_outputs;
		self.metadata = cache;
		self.pending_projects = HashSet::new();
		self.metadata_stale = false;
		self.watch_sets = HashMap::new();
		self.builders = effective_solution_builders(entry);
		Ok(())
	}
}

/// `solutionCoordinatorPaths`
fn solution_coordinator_paths(graph: &SolutionGraph) -> HashMap<String, String> {
	graph
		.coordinator_config_chains
		.iter()
		.filter_map(|chain| {
			canonical_solution_config_path(&chain.project_path)
				.ok()
				.map(|(_, key)| (key, chain.project_path.clone()))
		})
		.collect()
}
