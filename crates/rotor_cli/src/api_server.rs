//! `sloptor api-server` (`cmd/rotor/api_server.go`): the hidden build API over stdio. Each
//! request is a solution build, admitted like `sloptor build`. The session is long-lived
//! (`rotor_tsrs::project::set_long_lived`): each build frees its programs and parse cache, and
//! `memory::trim` runs after it.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use rotor_compile::DiagnosticInfo;
use rotor_compile::solution::{SolutionCoordinator, SolutionProjectResult, SolutionProjectStatus, SolutionRoot};
use rotor_compile::timings::BuildTimings;
use rotor_support::{gopath, logservice};

use crate::VERSION;
use crate::app::Failure;
use crate::buildapi::{self, BuildContext, BuildError, BuildRequest, BuildResult};
use crate::options::{
	MergedOptions, PartialOptions, find_ts_config_path, merge_project_options, project_compile_options,
	read_rbxts_layer,
};

/// `newAPIServerCommand`'s `RunE`.
///
/// # Errors
///
/// A transport or protocol failure.
pub fn run() -> Result<(), Failure> {
	// A session serves build after build, so each frees what it allocated, as watch mode does.
	rotor_tsrs::project::set_long_lived(true);
	// stdout carries the protocol, so the log goes to stderr.
	let previous = logservice::set_output_stderr(true);
	let server = buildapi::Server::new(
		VERSION,
		Arc::new(|context: &BuildContext, request: BuildRequest| {
			let _lease = crate::runtime::admit(&mut |message| {
				let _ = std::io::Write::write_all(&mut std::io::stderr(), format!("{message}\n").as_bytes());
			});
			let built = run_api_build(context, &request);
			crate::memory::trim();
			built
		}),
	);
	let outcome = server.run(std::io::stdin(), std::io::stdout());
	logservice::set_output_stderr(previous);
	outcome.map_err(Failure::Runtime)
}

/// `runAPIBuild`
fn run_api_build(context: &BuildContext, request: &BuildRequest) -> Result<BuildResult, BuildError> {
	let started = Instant::now();
	let timings = Arc::new(BuildTimings::new());
	timings.set_product_version(VERSION);
	let mut roots = Vec::with_capacity(request.roots.len());
	for requested in &request.roots {
		let ts_config_path = match find_ts_config_path(requested) {
			Ok(path) => path,
			Err(error) => return Ok(api_build_setup_failure(request, started, &error)),
		};
		let declared = match read_rbxts_layer(&ts_config_path) {
			Ok(declared) => declared,
			Err(error) => return Ok(api_build_setup_failure(request, started, &error)),
		};
		let mut opts = merge_project_options(MergedOptions::default(), &[declared.as_ref()]);
		opts.watch = false;
		// An empty argv layer makes referenced projects derive from defaults and their own rbxts
		// options, exactly like `build --build` with no flags.
		opts.argv = Some(PartialOptions::default());
		let mut options = project_compile_options(&ts_config_path, &opts);
		options.timings = Some(Arc::clone(&timings));
		options.context = Some(context.cancel.clone());
		if let Some(callback) = &context.transformer_callback {
			rotor_compile::transformer_host::with_transformer_callback(&mut options, Arc::clone(callback));
		}
		roots.push(SolutionRoot {
			config_path: ts_config_path,
			options,
		});
	}

	let mut coordinator = match SolutionCoordinator::for_root_options(&roots) {
		Ok(coordinator) => coordinator,
		Err(error) => return Ok(api_build_setup_failure(request, started, &error.to_string())),
	};
	let drained = coordinator.drain_with_project_results_for_selection(request.selection.as_ref());
	if context.cancel.is_cancelled() {
		// The server turns a cancellation with a cause into that cause's failure.
		return Err(BuildError::Cancelled);
	}
	let error = drained.error.map(|error| error.to_string());
	timings.set_ok(error.is_none());
	Ok(buildapi::result_from_solution(
		&drained.project_results,
		Some(&timings),
		started.elapsed(),
		error.as_deref(),
	))
}

/// `apiBuildSetupFailure`: setup errors come before the coordinator can produce owned outcomes,
/// so every selected config gets a terminal failure the caller can match.
fn api_build_setup_failure(request: &BuildRequest, started: Instant, error: &str) -> BuildResult {
	let selected = request
		.selection
		.as_ref()
		.map_or(&request.roots, |selection| &selection.selected);
	let fs = rotor_tsrs::vfs::osvfs::fs();
	let mut seen = HashSet::new();
	let mut projects = Vec::with_capacity(selected.len());
	for selected_path in selected {
		let config_path = find_ts_config_path(selected_path)
			.unwrap_or_else(|_| gopath::abs(&gopath::from_slash(selected_path)).unwrap_or_default());
		let mut identity = fs.realpath(&gopath::to_slash(&config_path));
		if !fs.use_case_sensitive_file_names() {
			identity = identity.to_lowercase();
		}
		if !seen.insert(identity) {
			continue;
		}
		projects.push(SolutionProjectResult {
			config_path,
			status: SolutionProjectStatus::Failed,
			blockers: Vec::new(),
			diagnostics: vec![DiagnosticInfo {
				message: error.to_owned(),
				..DiagnosticInfo::default()
			}],
			outputs: Vec::new(),
			output_count: 0,
			timings: rotor_compile::timings::ProjectBuildTimings::default(),
		});
	}
	let mut result = buildapi::result_from_solution(&projects, None, started.elapsed(), Some(error));
	if let Some(first) = result.projects.first() {
		result.diagnostics.clone_from(&first.diagnostics);
	}
	result.telemetry.selected_projects = i64::try_from(projects.len()).unwrap_or(i64::MAX);
	result
}
