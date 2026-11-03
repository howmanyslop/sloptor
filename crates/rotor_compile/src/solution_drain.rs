//! `solution_drain.go`: one drain of a solution, every emitted project built after its
//! dependencies, and the solution build entry point.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, Once};

use rotor_support::gopath;

use crate::census_solution::SolutionCensusDrainer;
use crate::compile::{DiagnosticInfo, Error, diagnostic_info_messages, string_diagnostics};
use crate::crossproject::write_roots_overlap;
use crate::output::{BuildFailure, BuildResult, build_project_with_options};
use crate::project::{PendingPersist, ProjectOptions, lock};
use crate::reference_validation::{validate_projects_reference_rojo_paths, validate_reference_rojo_paths};
use crate::solution::{
	DrainResult, SolutionCoordinator, SolutionGraph, SolutionProject, SolutionProjectDrainer, SolutionProjectResult,
	SolutionProjectSelection, SolutionProjectStatus, canonical_solution_config_path,
};
use crate::solution_cache::SolutionCompileCache;
use crate::solution_scheduler::{SolutionTask, run_solution_tasks};
use crate::timings::{
	BuildTimings, PROJECT_TIMING_STATUS_BLOCKED, PROJECT_TIMING_STATUS_FAILED, PROJECT_TIMING_STATUS_SKIPPED,
	ProjectBuildTimings, Stage, start_stage,
};

/// The coordinator's drainer: Go's `SolutionProjectDrainer` and the internal drainers it type
/// switches on.
pub(crate) enum Drainer {
	/// `*solutionBuildDrainer`
	Build(SolutionBuildDrainer),
	/// `*solutionCensusDrainer`
	Census(Arc<SolutionCensusDrainer>),
	/// A caller's drainer.
	Custom(Arc<dyn SolutionProjectDrainer>),
}

impl Drainer {
	/// Whether the drainer defers persistence to the end of a successful drain (Go's
	/// `appendPersists` and `persist` methods).
	fn queues_persistence(&self) -> bool {
		match self {
			Self::Build(_) => true,
			Self::Census(_) => false,
			Self::Custom(drainer) => drainer.queues_persistence(),
		}
	}
}

/// `solutionBuildDrainer`: builds each project to disk and publishes the persistence the
/// projects defer once the whole solution built.
#[derive(Default)]
pub(crate) struct SolutionBuildDrainer {
	pub(crate) restored_declarations: HashMap<String, Vec<String>>,
	pub(crate) restored_metadata_errs: HashMap<String, String>,
	warm: Arc<WarmWorkers>,
}

/// The persistence each successful project deferred, in first-success order (Go's
/// `persists` and `persistOrder`).
#[derive(Default)]
pub(crate) struct PersistQueue {
	pub(crate) persists: HashMap<String, Vec<PendingPersist>>,
	pub(crate) order: Vec<String>,
}

impl PersistQueue {
	/// `appendPersists`: keeps only the latest generation of a project's persistence.
	pub(crate) fn append(&mut self, config_path: &str, persists: Vec<PendingPersist>) {
		if !self.persists.contains_key(config_path) {
			self.order.push(config_path.to_owned());
		}
		self.persists.insert(config_path.to_owned(), persists);
	}

	/// `persist`: runs the deferred persistence in project order. A failure keeps the failed
	/// step and everything after it queued.
	pub(crate) fn persist(&mut self) -> Result<(), Error> {
		let order = std::mem::take(&mut self.order);
		for (project_index, config_path) in order.iter().enumerate() {
			let mut persists = self.persists.remove(config_path).unwrap_or_default();
			for index in 0..persists.len() {
				if let Err(error) = persists[index]() {
					// The consumed steps are dropped; the failed one runs again.
					persists.drain(..index);
					self.persists.insert(config_path.clone(), persists);
					self.order = order[project_index..].to_vec();
					return Err(Error::message(format!("persist solution build state: {error}")));
				}
			}
		}
		self.persists.clear();
		Ok(())
	}
}

/// `warmWorkers`: the transformer workers to start once a project actually compiles.
struct WarmWorkers {
	once: Once,
	config_paths: Vec<String>,
}

impl Default for WarmWorkers {
	fn default() -> Self {
		Self {
			once: Once::new(),
			config_paths: Vec::new(),
		}
	}
}

impl SolutionBuildDrainer {
	pub(crate) fn new(graph: &SolutionGraph, restored_metadata_errs: HashMap<String, String>) -> Self {
		Self {
			restored_declarations: HashMap::new(),
			restored_metadata_errs,
			warm: Arc::new(WarmWorkers {
				once: Once::new(),
				config_paths: graph
					.projects
					.iter()
					.map(|project| project.config_path.clone())
					.collect(),
			}),
		}
	}

	/// `Drain`
	fn drain(&self, coordinator: &SolutionCoordinator, project: &SolutionProject) -> DrainResult {
		// Dependencies can copy nested Rojo projects into their outputs. Refresh the consistency
		// check before compiling a consumer against those mounts.
		if let Err(error) = validate_projects_reference_rojo_paths(
			&coordinator.graph,
			std::slice::from_ref(project),
			&coordinator.reference_outputs,
			Some(&coordinator.metadata.references),
		) {
			let message = error.to_string();
			return DrainResult {
				result: Some(BuildResult {
					diagnostics: string_diagnostics(vec![message.clone()]),
					..BuildResult::default()
				}),
				messages: vec![message],
				error: Some(error),
			};
		}
		let mut options = project.options.clone();
		options.ts_config_path.clone_from(&project.config_path);
		options.defer_rojo_cache_persist = true;
		let warm = Arc::clone(&self.warm);
		options.on_compile_start = Some(Arc::new(move || {
			warm.once.call_once(|| warm_transformer_workers(&warm.config_paths));
		}));
		build_result(build_project_with_options(&gopath::dir(&project.config_path), &options))
	}
}

/// `warmTransformerWorkers`: starts the first transformer worker of every project whose plugins
/// run in the sidecar. It runs once a project actually compiles, so an up-to-date solution
/// spawns no Node process.
fn warm_transformer_workers(config_paths: &[String]) {
	for config_path in config_paths {
		let Ok(plugins) = crate::transformer_order::effective_transformer_plugins(config_path) else {
			continue;
		};
		if !plugins
			.iter()
			.any(|plugin| plugin.transform != crate::flamework_config::LEGACY_FLAMEWORK_TRANSFORMER)
		{
			continue;
		}
		let dir = gopath::dir(config_path);
		// A parallel transformer split admits only a worker no request has used yet, so a warmed
		// worker would turn it away.
		if rotor_config::config::load(&dir)
			.is_ok_and(|config| crate::sidecar_parallel::parallel_config_supported(&config))
		{
			continue;
		}
		crate::sidecar_shards::warm_transformer_sidecar_shards(&dir, config_path, None, 0);
	}
}

/// Go's `(*BuildResult, []string, error)` from a build.
pub(crate) fn build_result(built: Result<BuildResult, BuildFailure>) -> DrainResult {
	match built {
		Ok(result) => DrainResult {
			result: Some(result),
			messages: Vec::new(),
			error: None,
		},
		Err(failure) => DrainResult {
			messages: failure.failure.messages(),
			result: failure.result.map(|result| *result),
			error: Some(failure.failure.error),
		},
	}
}

/// What one drain produced: the merged result, one outcome per emitted project, the failure's
/// messages and the first error.
pub struct DrainOutput {
	/// The merged result of every built project.
	pub result: BuildResult,
	/// One terminal outcome per emitted project, in graph order.
	pub project_results: Vec<SolutionProjectResult>,
	/// The failure's messages.
	pub messages: Vec<String>,
	/// The first error.
	pub error: Option<Error>,
}

#[derive(Default)]
struct DrainOutcome {
	skip: bool,
	blockers: Vec<String>,
	result: Option<BuildResult>,
	messages: Vec<String>,
	err: Option<Error>,
	persists: Vec<PendingPersist>,
}

/// `solutionSelectionError`
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SolutionSelectionError {
	pub(crate) config_path: String,
	pub(crate) code: &'static str,
	pub(crate) message: String,
}

/// A test drainer's satisfied-output validation.
#[cfg(test)]
pub(crate) type SatisfiedValidator = Box<dyn Fn(&SolutionProject) -> Option<SolutionSelectionError> + Send + Sync>;

/// `solutionSelectionResolution`
#[derive(Default)]
struct SolutionSelectionResolution {
	selected: HashSet<String>,
	satisfied: HashSet<String>,
	errors: HashMap<String, Vec<SolutionSelectionError>>,
	extra_results: Vec<String>,
}

impl SolutionCoordinator {
	/// `Drain`
	pub fn drain(&mut self) -> (BuildResult, Vec<String>, Option<Error>) {
		let output = self.drain_with_project_results();
		(output.result, output.messages, output.error)
	}

	/// `DrainWithProjectResults`: drains the union, one terminal outcome per emitted project in
	/// graph order.
	pub fn drain_with_project_results(&mut self) -> DrainOutput {
		self.drain_with_project_results_for_selection(None)
	}

	/// `DrainWithProjectResultsForSelection`: applies one external ownership decision without
	/// changing the coordinator's warm-build state.
	pub fn drain_with_project_results_for_selection(
		&mut self,
		selection: Option<&SolutionProjectSelection>,
	) -> DrainOutput {
		let timings = self.timings.clone();
		let output = self.drain_inner(selection, timings.as_ref());
		if let Some(timings) = &timings {
			timings.finish();
		}
		output
	}

	#[expect(clippy::too_many_lines, reason = "one function in Go")]
	fn drain_inner(
		&mut self,
		selection: Option<&SolutionProjectSelection>,
		timings: Option<&Arc<BuildTimings>>,
	) -> DrainOutput {
		let mut result = BuildResult::default();
		if matches!(self.drainer, Drainer::Build(_)) {
			let refreshed = if !self.pending_projects.is_empty() || self.metadata_stale {
				self.refresh_pending_metadata()
			} else {
				validate_reference_rojo_paths(&self.graph, &self.reference_outputs, Some(&self.metadata.references))
			};
			if let Err(error) = refreshed {
				let message = error.to_string();
				result.diagnostics = string_diagnostics(vec![message.clone()]);
				return DrainOutput {
					result,
					project_results: Vec::new(),
					messages: vec![message],
					error: Some(error),
				};
			}
		}
		let cache = Arc::new(SolutionCompileCache::new());
		if selection.is_some()
			&& let Some(timings) = timings
		{
			timings.apply_project_selection(&HashSet::new(), &HashSet::new());
		}
		let resolution = self.resolve_project_selection(selection);
		if selection.is_some()
			&& let Some(timings) = timings
		{
			timings.apply_project_selection(&resolution.selected, &resolution.satisfied);
		}
		let satisfied_errors = self.validate_satisfied_outputs(&resolution.satisfied, &cache);
		let mut index_by_config_path: HashMap<String, usize> = HashMap::with_capacity(self.graph.projects.len());
		for (index, project) in self.graph.projects.iter().enumerate() {
			if selection.is_some() && !resolution.selected.contains(&project.config_path) {
				continue;
			}
			index_by_config_path.insert(project.config_path.clone(), index);
		}
		let mut tasks = Vec::with_capacity(index_by_config_path.len());
		for (index, project) in self.graph.projects.iter().enumerate() {
			if !index_by_config_path.contains_key(&project.config_path) {
				continue;
			}
			let mut task = SolutionTask {
				index,
				..SolutionTask::default()
			};
			for reference in &project.references {
				if let Some(&predecessor) = index_by_config_path.get(reference) {
					task.predecessors.push(predecessor);
				}
			}
			if let Some(dependencies) = self.wait_only_dependencies.get(&project.config_path) {
				for dependency in dependencies {
					if let Some(&predecessor) = index_by_config_path.get(dependency) {
						task.wait_only.push(predecessor);
					}
				}
			}
			tasks.push(task);
		}

		let outcomes: Vec<Mutex<DrainOutcome>> = (0..self.graph.projects.len())
			.map(|_| Mutex::new(DrainOutcome::default()))
			.collect();
		{
			let coordinator: &Self = self;
			run_solution_tasks(&tasks, coordinator.builders, |index| -> Result<(), ()> {
				let outcome = coordinator.drain_task(
					index,
					&resolution,
					&satisfied_errors,
					&index_by_config_path,
					&outcomes,
					&cache,
					timings,
				);
				let failed = outcome.err.is_some();
				*lock(&outcomes[index]) = outcome;
				if failed { Err(()) } else { Ok(()) }
			});
		}
		let mut outcomes: Vec<DrainOutcome> = outcomes
			.into_iter()
			.map(|outcome| outcome.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner))
			.collect();
		if let Some(timings) = timings {
			timings.add_parse_cache_counts(cache.hits(), cache.misses());
		}

		let mut first_err: Option<Error> = None;
		let mut project_results = Vec::with_capacity(self.graph.projects.len() + resolution.extra_results.len());
		let projects = self.graph.projects.clone();
		for (index, project) in projects.iter().enumerate() {
			if selection.is_some() {
				if resolution.satisfied.contains(&project.config_path) {
					let mut project_result = SolutionProjectResult {
						config_path: project.config_path.clone(),
						status: SolutionProjectStatus::Satisfied,
						blockers: Vec::new(),
						diagnostics: Vec::new(),
						outputs: Vec::new(),
						output_count: 0,
						timings: self.project_timing(&project.config_path, SolutionProjectStatus::Satisfied.as_str()),
					};
					if let Some(errors) = satisfied_errors.get(&project.config_path)
						&& !errors.is_empty()
					{
						project_result.status = SolutionProjectStatus::Failed;
						project_result.diagnostics = solution_selection_diagnostics(errors);
						if let Some(timings) = timings {
							timings.set_project_status(&project.config_path, PROJECT_TIMING_STATUS_FAILED, "");
							project_result.timings =
								self.project_timing(&project.config_path, PROJECT_TIMING_STATUS_FAILED);
						}
						result.diagnostics.extend(project_result.diagnostics.iter().cloned());
						if first_err.is_none() {
							first_err = solution_selection_errors_error(errors);
						}
					}
					project_results.push(project_result);
					continue;
				}
				if !resolution.selected.contains(&project.config_path) {
					continue;
				}
			}
			let outcome = std::mem::take(&mut outcomes[index]);
			if outcome.skip {
				let outputs = solution_owned_outputs(
					self.states
						.get(&project.config_path)
						.and_then(|state| state.result.as_ref()),
				);
				project_results.push(SolutionProjectResult {
					config_path: project.config_path.clone(),
					status: SolutionProjectStatus::NoChange,
					blockers: Vec::new(),
					diagnostics: Vec::new(),
					output_count: outputs.len(),
					outputs,
					timings: self.project_timing(&project.config_path, PROJECT_TIMING_STATUS_SKIPPED),
				});
				continue;
			}
			if outcome.err.is_none() && self.drainer.queues_persistence() {
				lock(&self.persist_queue).append(&project.config_path, outcome.persists);
			}
			if let Some(built) = &outcome.result {
				merge_solution_build_result(&mut result, project, built);
			}
			let Some(state) = self.states.get_mut(&project.config_path) else {
				continue;
			};
			state.result.clone_from(&outcome.result);
			if let Some(error) = &outcome.err {
				if let Some(blocker) = outcome.blockers.first() {
					state.blocked_by.clone_from(blocker);
					state.err = Some(error.clone());
					result.diagnostics.push(DiagnosticInfo {
						message: error.to_string(),
						..DiagnosticInfo::default()
					});
				} else {
					state.err = Some(error.clone());
					if outcome.result.as_ref().is_none_or(|built| built.diagnostics.is_empty()) {
						for message in &outcome.messages {
							result.diagnostics.push(DiagnosticInfo {
								message: message.clone(),
								..DiagnosticInfo::default()
							});
						}
					}
				}
				if first_err.is_none() {
					first_err = Some(error.clone());
				}
			} else {
				state.blocked_by.clear();
				state.err = None;
				state.up_to_date = true;
			}

			let mut project_result = SolutionProjectResult {
				config_path: project.config_path.clone(),
				status: SolutionProjectStatus::Success,
				blockers: outcome.blockers.clone(),
				diagnostics: Vec::new(),
				outputs: Vec::new(),
				output_count: 0,
				timings: ProjectBuildTimings::default(),
			};
			if let Some(built) = &outcome.result {
				project_result.diagnostics.clone_from(&built.diagnostics);
				project_result.outputs = solution_owned_outputs(Some(built));
				project_result.output_count = project_result.outputs.len();
			}
			if !outcome.blockers.is_empty() {
				project_result.status = SolutionProjectStatus::Blocked;
				project_result.diagnostics = Vec::new();
			} else if outcome.err.is_some() {
				project_result.status = SolutionProjectStatus::Failed;
			}
			project_result.timings = self.project_timing(&project.config_path, project_result.status.as_str());
			let counts = project_result.timings.counts;
			if project_result.status == SolutionProjectStatus::Success
				&& counts.total_sources > 0
				&& counts.selected_sources == 0
				&& counts.emitted_entries == 0
			{
				project_result.status = SolutionProjectStatus::NoChange;
				SolutionProjectStatus::NoChange
					.as_str()
					.clone_into(&mut project_result.timings.status);
			}
			project_results.push(project_result);
		}
		for config_path in &resolution.extra_results {
			let Some(errors) = resolution.errors.get(config_path).filter(|errors| !errors.is_empty()) else {
				continue;
			};
			let diagnostics = solution_selection_diagnostics(errors);
			project_results.push(SolutionProjectResult {
				config_path: config_path.clone(),
				status: SolutionProjectStatus::Failed,
				blockers: Vec::new(),
				diagnostics: diagnostics.clone(),
				outputs: Vec::new(),
				output_count: 0,
				timings: self.project_timing(config_path, SolutionProjectStatus::Failed.as_str()),
			});
			result.diagnostics.extend(diagnostics);
			if first_err.is_none() {
				first_err = solution_selection_errors_error(errors);
			}
		}
		if let Some(error) = first_err {
			return DrainOutput {
				messages: diagnostic_info_messages(&result.diagnostics),
				result,
				project_results,
				error: Some(error),
			};
		}
		if self.drainer.queues_persistence()
			&& let Err(error) = lock(&self.persist_queue).persist()
		{
			return DrainOutput {
				messages: diagnostic_info_messages(&result.diagnostics),
				result,
				project_results,
				error: Some(error),
			};
		}
		DrainOutput {
			result,
			project_results,
			messages: Vec::new(),
			error: None,
		}
	}

	/// One project's task of a drain.
	#[expect(clippy::too_many_arguments, reason = "the closure's captures in Go")]
	#[expect(clippy::too_many_lines, reason = "the closure's body in Go")]
	fn drain_task(
		&self,
		index: usize,
		resolution: &SolutionSelectionResolution,
		satisfied_errors: &HashMap<String, Vec<SolutionSelectionError>>,
		index_by_config_path: &HashMap<String, usize>,
		outcomes: &[Mutex<DrainOutcome>],
		cache: &Arc<SolutionCompileCache>,
		timings: Option<&Arc<BuildTimings>>,
	) -> DrainOutcome {
		let mut project = self.graph.projects[index].clone();
		let up_to_date = self
			.states
			.get(&project.config_path)
			.is_some_and(|state| state.up_to_date);
		let mut outcome = DrainOutcome::default();
		if let Some(errors) = resolution.errors.get(&project.config_path)
			&& !errors.is_empty()
		{
			let diagnostics = solution_selection_diagnostics(errors);
			outcome.messages = diagnostic_info_messages(&diagnostics);
			outcome.result = Some(BuildResult {
				diagnostics,
				..BuildResult::default()
			});
			outcome.err = solution_selection_errors_error(errors);
			if let Some(timings) = timings {
				timings.set_project_status(&project.config_path, PROJECT_TIMING_STATUS_FAILED, "");
			}
			return outcome;
		}
		for reference in &project.references {
			if let Some(&predecessor) = index_by_config_path.get(reference)
				&& lock(&outcomes[predecessor]).err.is_some()
			{
				outcome.blockers.push(reference.clone());
				continue;
			}
			if resolution.satisfied.contains(reference)
				&& satisfied_errors.get(reference).is_some_and(|errors| !errors.is_empty())
			{
				outcome.blockers.push(reference.clone());
			}
		}
		if let Some(blocker) = outcome.blockers.first() {
			outcome.err = Some(Error::message(format!(
				"compile: project {} blocked by failed dependency {blocker}",
				project.config_path
			)));
			if let Some(timings) = timings {
				timings.set_project_status(&project.config_path, PROJECT_TIMING_STATUS_BLOCKED, blocker);
			}
			return outcome;
		}
		if up_to_date {
			outcome.skip = true;
			if let Some(timings) = timings {
				timings.set_project_status(&project.config_path, PROJECT_TIMING_STATUS_SKIPPED, "");
			}
			return outcome;
		}

		// Each project's incremental manifest tracks referenced declarations: it selects affected
		// sources after dependencies emitted, as a fresh one-shot solution build does.
		let child = timings.map(|timings| timings.new_project(&project.config_path));
		if let Some(child) = &child {
			project.options.timings = Some(Arc::clone(child));
		}
		let persists: Arc<Mutex<Vec<PendingPersist>>> = Arc::new(Mutex::new(Vec::new()));
		let dependency_persists: Arc<Mutex<Vec<PendingPersist>>> = Arc::new(Mutex::new(Vec::new()));
		project.options.pending_solution_persists = Some(Arc::clone(&persists));
		project.options.pending_solution_dependency_persists = Some(Arc::clone(&dependency_persists));
		project.options.defer_rojo_cache_persist = true;
		project.options.compile_cache = Some(Arc::clone(cache));
		let drained = match &self.drainer {
			Drainer::Build(drainer) => drainer.drain(self, &project),
			Drainer::Census(drainer) => drainer.drain(project.clone()),
			Drainer::Custom(drainer) => drainer.drain(project.clone()),
		};
		outcome.result = drained.result;
		outcome.messages = drained.messages;
		outcome.err = drained.error;
		let mut persists = std::mem::take(&mut *lock(&persists));
		let mut dependency_persists = std::mem::take(&mut *lock(&dependency_persists));
		if let Some(child) = &child {
			if outcome.err.is_some() {
				child.set_project_status(&project.config_path, PROJECT_TIMING_STATUS_FAILED, "");
			}
			if !child.finished() {
				child.finish();
			}
			persists = persists
				.into_iter()
				.map(|persist| timed_persist(child, persist))
				.collect();
			dependency_persists = dependency_persists
				.into_iter()
				.map(|persist| timed_persist(child, persist))
				.collect();
		}
		outcome.persists = persists;
		if outcome.err.is_none() {
			for mut persist in dependency_persists {
				if let Err(error) = persist() {
					let error = Error::message(format!(
						"compile: publish dependency state for project {}: {error}",
						project.config_path
					));
					outcome.messages.push(error.to_string());
					outcome.err = Some(error);
					break;
				}
			}
		}
		if let Some(error) = &outcome.err
			&& outcome.result.as_ref().is_none_or(|built| built.diagnostics.is_empty())
		{
			let mut diagnostics = string_diagnostics(outcome.messages.clone());
			if diagnostics.is_empty() {
				diagnostics = vec![DiagnosticInfo {
					message: error.to_string(),
					..DiagnosticInfo::default()
				}];
			}
			outcome.result.get_or_insert_with(BuildResult::default).diagnostics = diagnostics;
		}
		outcome
	}

	/// `resolveProjectSelection`
	#[expect(clippy::too_many_lines, reason = "one function in Go")]
	fn resolve_project_selection(&self, selection: Option<&SolutionProjectSelection>) -> SolutionSelectionResolution {
		let Some(selection) = selection else {
			return SolutionSelectionResolution::default();
		};
		let mut errors: HashMap<String, Vec<SolutionSelectionError>> = HashMap::new();
		let mut extra_results = Vec::new();
		let mut extra_seen = HashSet::new();
		let mut append = |error: SolutionSelectionError| {
			let existing = errors.entry(error.config_path.clone()).or_default();
			if existing
				.iter()
				.any(|existing| existing.code == error.code && existing.message == error.message)
			{
				return;
			}
			let config_path = error.config_path.clone();
			existing.push(error);
			if self.states.contains_key(&config_path) {
				return;
			}
			if extra_seen.insert(config_path.clone()) {
				extra_results.push(config_path);
			}
		};
		let resolve = |paths: &[String], set_name: &str, append: &mut dyn FnMut(SolutionSelectionError)| {
			let mut resolved = HashSet::with_capacity(paths.len());
			for path in paths {
				if path.trim().is_empty() {
					append(SolutionSelectionError {
						config_path: path.clone(),
						code: "SOLUTION_SELECTION_INVALID_PATH",
						message: format!("{set_name} configs must contain only non-empty paths"),
					});
					continue;
				}
				let (config_path, key) = match canonical_solution_config_path(path) {
					Ok(canonical) => canonical,
					Err(error) => {
						append(SolutionSelectionError {
							config_path: path.clone(),
							code: "SOLUTION_SELECTION_INVALID_PATH",
							message: format!(
								"{set_name} config {} cannot be canonicalized: {error}",
								rotor_support::gostr::quote(path)
							),
						});
						continue;
					}
				};
				if let Some(project_path) = self.project_paths.get(&key) {
					resolved.insert(project_path.clone());
					continue;
				}
				if let Some(coordinator_path) = self.coordinator_paths.get(&key) {
					append(SolutionSelectionError {
						config_path: coordinator_path.clone(),
						code: "SOLUTION_SELECTION_COORDINATOR",
						message: format!(
							"{set_name} config {coordinator_path} is coordinator-only and cannot own compiler work or restored outputs"
						),
					});
					continue;
				}
				append(SolutionSelectionError {
					message: format!("{set_name} config {config_path} is not part of the discovered solution graph"),
					config_path,
					code: "SOLUTION_SELECTION_UNKNOWN",
				});
			}
			resolved
		};
		let selected = resolve(&selection.selected, "selected", &mut append);
		let mut satisfied = resolve(&selection.satisfied, "satisfied", &mut append);
		for project in &self.graph.projects {
			if selected.contains(&project.config_path) && satisfied.contains(&project.config_path) {
				append(SolutionSelectionError {
					config_path: project.config_path.clone(),
					code: "SOLUTION_SELECTION_OVERLAP",
					message: format!(
						"project {} is both selected and satisfied; assign each config to exactly one set",
						project.config_path
					),
				});
				satisfied.remove(&project.config_path);
			}
		}
		for project in &self.graph.projects {
			if !selected.contains(&project.config_path) {
				continue;
			}
			for reference in &project.references {
				if selected.contains(reference) || satisfied.contains(reference) {
					continue;
				}
				append(SolutionSelectionError {
					config_path: project.config_path.clone(),
					code: "SOLUTION_SELECTION_MISSING_DEPENDENCY",
					message: format!(
						"selected project {} depends on {reference}, which has no owner; add the dependency to selected or satisfied",
						project.config_path
					),
				});
			}
		}
		let empty = Vec::new();
		for selected_project in &self.graph.projects {
			if !selected.contains(&selected_project.config_path) {
				continue;
			}
			for satisfied_project in &self.graph.projects {
				if !satisfied.contains(&satisfied_project.config_path) {
					continue;
				}
				let selected_roots = self.write_roots.get(&selected_project.config_path).unwrap_or(&empty);
				let satisfied_roots = self.write_roots.get(&satisfied_project.config_path).unwrap_or(&empty);
				if !write_roots_overlap(selected_roots, satisfied_roots) {
					continue;
				}
				append(SolutionSelectionError {
					config_path: selected_project.config_path.clone(),
					code: "SOLUTION_SELECTION_WRITE_ROOT_OVERLAP",
					message: format!(
						"selected project {} has overlapping write roots [{}] with satisfied project {} write roots [{}]; satisfied outputs are read-only, so assign disjoint output and include directories",
						selected_project.config_path,
						selected_roots.join(" "),
						satisfied_project.config_path,
						satisfied_roots.join(" "),
					),
				});
			}
		}
		errors.retain(|_, errors| !errors.is_empty());
		SolutionSelectionResolution {
			selected,
			satisfied,
			errors,
			extra_results,
		}
	}

	/// `refreshPendingMetadata`: re-parses the invalidated projects' configs only, and derives the
	/// solution metadata again only when a config or root file set changed.
	fn refresh_pending_metadata(&mut self) -> Result<(), Error> {
		let pending: Vec<SolutionProject> = self
			.graph
			.projects
			.iter()
			.filter(|project| self.pending_projects.contains(&project.config_path))
			.cloned()
			.collect();
		if self.metadata.parse(&pending) {
			self.metadata_stale = true;
		}
		// Imports can change a project's emitted set without changing its config, so its
		// satisfied declarations are listed again.
		if let Drainer::Build(drainer) = &mut self.drainer {
			for project in &pending {
				drainer.restored_declarations.remove(&project.config_path);
			}
		}
		self.metadata.references.forget(&pending);
		if !self.metadata_stale {
			validate_projects_reference_rojo_paths(
				&self.graph,
				&pending,
				&self.reference_outputs,
				Some(&self.metadata.references),
			)?;
			self.pending_projects.clear();
			return Ok(());
		}
		let mut metadata = self.metadata.derive(&mut self.graph, &pending);
		if let Some(error) = metadata.validation_err.take() {
			return Err(error.error);
		}
		self.reference_outputs = metadata.reference_outputs;
		self.write_roots = metadata.write_roots;
		self.wait_only_dependencies = metadata.wait_only_dependencies;
		if let Drainer::Build(drainer) = &mut self.drainer {
			drainer.restored_declarations.clear();
			drainer.restored_metadata_errs = metadata.restored_metadata_errs;
		}
		for project in &self.graph.projects {
			if let Some(state) = self.states.get_mut(&project.config_path) {
				state.project = project.clone();
			}
		}
		self.pending_projects.clear();
		self.metadata_stale = false;
		Ok(())
	}

	/// `validateSatisfiedOutputs`: only the build drainer checks restored outputs.
	fn validate_satisfied_outputs(
		&mut self,
		satisfied: &HashSet<String>,
		cache: &SolutionCompileCache,
	) -> HashMap<String, Vec<SolutionSelectionError>> {
		let mut errors: HashMap<String, Vec<SolutionSelectionError>> = HashMap::new();
		#[cfg(test)]
		if let Some(validator) = &self.satisfied_validator {
			for project in &self.graph.projects {
				if satisfied.contains(&project.config_path)
					&& let Some(error) = validator(project)
				{
					errors.entry(project.config_path.clone()).or_default().push(error);
				}
			}
			return errors;
		}
		let Drainer::Build(drainer) = &mut self.drainer else {
			return errors;
		};
		for project in &self.graph.projects {
			if !satisfied.contains(&project.config_path) {
				continue;
			}
			if let Some(error) = drainer.validate_satisfied_project(project, cache) {
				errors.entry(project.config_path.clone()).or_default().push(error);
			}
		}
		errors
	}

	/// `projectTiming`
	fn project_timing(&self, config_path: &str, fallback_status: &str) -> ProjectBuildTimings {
		self.timings
			.as_ref()
			.and_then(|timings| timings.project_timing(config_path))
			.unwrap_or_else(|| ProjectBuildTimings {
				config_path: config_path.to_owned(),
				status: fallback_status.to_owned(),
				..ProjectBuildTimings::default()
			})
	}
}

/// `timedPersist` over an owned child report.
fn timed_persist(timings: &Arc<BuildTimings>, mut persist: PendingPersist) -> PendingPersist {
	let timings = Arc::clone(timings);
	Box::new(move || {
		let stop = start_stage(Some(&timings), Stage::Persistence);
		let result = persist();
		stop.stop();
		result
	})
}

/// `solutionOwnedOutputs`: the project's owned outputs, else its compiled outputs, sorted and
/// slash-separated.
fn solution_owned_outputs(result: Option<&BuildResult>) -> Vec<String> {
	let Some(result) = result else {
		return Vec::new();
	};
	let mut outputs: Vec<String> = if result.owned_outputs.is_empty() {
		result
			.outputs
			.keys()
			.map(|output| gopath::to_slash(output).into_owned())
			.collect()
	} else {
		result.owned_outputs.clone()
	};
	outputs.sort();
	outputs
}

/// `solutionSelectionDiagnostics`
fn solution_selection_diagnostics(errors: &[SolutionSelectionError]) -> Vec<DiagnosticInfo> {
	errors
		.iter()
		.map(|error| DiagnosticInfo {
			code: error.code.to_owned(),
			message: error.message.clone(),
			..DiagnosticInfo::default()
		})
		.collect()
}

/// `solutionSelectionErrorsError`
fn solution_selection_errors_error(errors: &[SolutionSelectionError]) -> Option<Error> {
	errors
		.first()
		.map(|error| Error::message(format!("compile: {}", error.message)))
}

/// `EffectiveSolutionBuilders`: 1 single-threaded, else `--builders`, else 4.
#[must_use]
pub fn effective_solution_builders(entry: &ProjectOptions) -> i64 {
	if entry.single_threaded == Some(true) {
		return 1;
	}
	entry.builders.unwrap_or(4)
}

/// `BuildSolutionWithOptions`: discovers the solution of `ts_config_path` and builds it.
///
/// # Errors
///
/// A [`BuildFailure`] carrying the merged result and the first error.
pub fn build_solution_with_options(ts_config_path: &str, entry: &ProjectOptions) -> Result<BuildResult, BuildFailure> {
	let mut coordinator = match SolutionCoordinator::new(ts_config_path, entry) {
		Ok(coordinator) => coordinator,
		Err(error) => {
			return Err(match error.diagnostic {
				Some(diagnostic) => BuildFailure {
					result: Some(Box::new(BuildResult {
						diagnostics: vec![(*diagnostic).clone()],
						..BuildResult::default()
					})),
					failure: crate::compile::Failure {
						diagnostics: vec![*diagnostic],
						error: error.error,
					},
				},
				None => BuildFailure::from(error.error),
			});
		}
	};
	let (result, _, error) = coordinator.drain();
	match error {
		None => Ok(result),
		Some(error) => Err(BuildFailure {
			failure: crate::compile::Failure {
				diagnostics: result.diagnostics.clone(),
				error,
			},
			result: Some(Box::new(result)),
		}),
	}
}

/// `mergeSolutionBuildResult`
fn merge_solution_build_result(result: &mut BuildResult, project: &SolutionProject, built: &BuildResult) {
	let dir = gopath::dir(&project.config_path);
	let outputs: BTreeMap<String, Vec<u8>> = built
		.outputs
		.iter()
		.map(|(path, text)| (gopath::join(&[&dir, path]), text.clone()))
		.collect();
	result.outputs.extend(outputs);
	result.emitted_files.extend(built.emitted_files.iter().cloned());
	result.diagnostics.extend(built.diagnostics.iter().cloned());
	result.wrote_rotor_types = result.wrote_rotor_types || built.wrote_rotor_types;
}
