//! `census_solution.go`: census mode over a whole solution.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rotor_support::gopath;

use crate::census::{ProjectDiagnostics, compile_project_diagnostics};
use crate::compile::{DiagnosticInfo, Error};
use crate::crossproject::new_cross_project_metadata;
use crate::output::BuildResult;
use crate::overlayfs::overlay_keys_in_program;
use crate::project::{ProjectOptions, lock};
use crate::sidecar::normalize_overlay_path;
use crate::solution::{
	DrainResult, SolutionCoordinator, SolutionProject, SolutionProjectDrainer, build_solution_graph,
};
use crate::solution_drain::{Drainer, effective_solution_builders};

/// `solutionOverlayMatches`: the overlay keys that named a file some project of a solution
/// census held.
///
/// A solution partitions its files between projects, so an overlay for one
/// project's file matches nothing in the others; only an overlay that matched nowhere fails.
#[derive(Default)]
pub struct SolutionOverlayMatches {
	matched: Mutex<HashSet<String>>,
}

impl SolutionOverlayMatches {
	/// `record`: one project's matched keys, already normalized.
	pub(crate) fn record(&self, matched: HashSet<String>) {
		lock(&self.matched).extend(matched);
	}

	/// `count`: the distinct keys some project matched.
	pub(crate) fn count(&self) -> usize {
		lock(&self.matched).len()
	}

	/// `unmatched`: the caller's keys, as spelled, that no project matched, sorted.
	pub(crate) fn unmatched(&self, overlays: &BTreeMap<String, String>) -> Vec<String> {
		let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
		let matched = lock(&self.matched);
		overlays
			.keys()
			.filter(|path| !matched.contains(&normalize_overlay_path(path, case_sensitive)))
			.cloned()
			.collect()
	}
}

/// `matchSolutionOverlaysToProgram`: records the keys this program matched; never fails.
pub(crate) fn match_solution_overlays_to_program(
	program: &rotor_tsrs::compiler::Program,
	overlays: &BTreeMap<String, String>,
	tracker: &SolutionOverlayMatches,
) -> usize {
	let (matched, _) = overlay_keys_in_program(program, overlays);
	let count = matched.len();
	tracker.record(matched.into_iter().collect());
	count
}

/// `solutionCensusDrainer`: censuses one project and keeps the result. It never fails: a census
/// reads no other project's output, so a failed project must not block its dependants.
pub(crate) struct SolutionCensusDrainer {
	overlays: BTreeMap<String, String>,
	overlay_matches: Arc<SolutionOverlayMatches>,
	results: Mutex<HashMap<String, (ProjectDiagnostics, Option<Error>)>>,
}

impl SolutionProjectDrainer for SolutionCensusDrainer {
	fn drain(&self, project: SolutionProject) -> DrainResult {
		let mut options = project.options.clone();
		options.ts_config_path.clone_from(&project.config_path);
		options.overlays.clone_from(&self.overlays);
		options.solution_overlays = Some(Arc::clone(&self.overlay_matches));
		// The census policy `rotor diagnostics` applies to a single project: rotor's own comment
		// directive diagnostic is reported, whatever a project's own rbxts key says.
		options.allow_comment_directives = false;

		let (mut census, error) = compile_project_diagnostics(&gopath::dir(&project.config_path), &options);
		if let Some(error) = &error
			&& census.diagnostics.is_empty()
		{
			census.diagnostics.push(DiagnosticInfo {
				message: error.to_string(),
				..DiagnosticInfo::default()
			});
		}
		lock(&self.results).insert(project.config_path, (census, error));
		DrainResult {
			result: Some(BuildResult::default()),
			messages: Vec::new(),
			error: None,
		}
	}
}

/// `SolutionDiagnostics`: the census of a whole solution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SolutionDiagnostics {
	/// One census per project, dependencies first.
	pub projects: Vec<ProjectDiagnostics>,
	/// The distinct overlay keys some project matched. Not the projects' sum: a referenced
	/// project's sources are in its dependants' programs too.
	pub overlay_matches: usize,
}

/// `CompileSolutionDiagnostics`: [`compile_project_diagnostics`] for every project the entry
/// references, transitively, in dependency order.
///
/// It writes nothing. The first project failure is the run's error once every project has had
/// its turn; the result is never empty because of it.
#[must_use]
pub fn compile_solution_diagnostics(
	ts_config_path: &str,
	entry: &ProjectOptions,
) -> (SolutionDiagnostics, Option<Error>) {
	let mut solution = SolutionDiagnostics::default();
	let mut graph = match build_solution_graph(ts_config_path, entry) {
		Ok(graph) => graph,
		Err(error) => return (solution, Some(error)),
	};
	// The cross-project import paths a solution build uses, so a census agrees with
	// `rotor build --build`.
	let (mut metadata, cache) = new_cross_project_metadata(&mut graph);
	let validation_err = metadata.validation_err.take();
	let drainer = Arc::new(SolutionCensusDrainer {
		overlays: entry.overlays.clone(),
		overlay_matches: Arc::new(SolutionOverlayMatches::default()),
		results: Mutex::new(HashMap::new()),
	});
	let projects = graph.projects.clone();
	let mut coordinator = match SolutionCoordinator::assemble(
		graph,
		Drainer::Census(Arc::clone(&drainer)),
		metadata,
		cache,
		effective_solution_builders(entry),
		entry.timings.clone(),
	) {
		Ok(coordinator) => coordinator,
		Err(error) => return (solution, Some(error)),
	};
	let (_, _, error) = coordinator.drain();
	if let Some(error) = error {
		return (solution, Some(error));
	}

	solution.overlay_matches = drainer.overlay_matches.count();
	let mut results = std::mem::take(&mut *lock(&drainer.results));
	let mut first_err = None;
	for project in &projects {
		let Some((census, error)) = results.remove(&project.config_path) else {
			continue;
		};
		solution.projects.push(census);
		if first_err.is_none() {
			first_err = error;
		}
	}
	// Solution validation fails a build, not a census: it is reported after every project.
	let first_err = first_err.or_else(|| validation_err.map(|error| error.error));
	if first_err.is_some() {
		// Ahead of the overlay check: a project that built no program matched nothing, and
		// reporting its overlays would name a symptom instead of the cause.
		return (solution, first_err);
	}
	let unmatched = drainer.overlay_matches.unmatched(&entry.overlays);
	if !unmatched.is_empty() {
		return (
			solution,
			Some(Error::message(format!(
				"compile: overlay matches no file in the solution: {}",
				unmatched.join(", ")
			))),
		);
	}
	(solution, None)
}
