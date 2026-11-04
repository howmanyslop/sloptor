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
	let outcome = server().run(std::io::stdin(), std::io::stdout());
	logservice::set_output_stderr(previous);
	outcome.map_err(Failure::Runtime)
}

/// The build API server, each build admitted like `sloptor build` and trimmed after.
fn server() -> Arc<buildapi::Server> {
	buildapi::Server::new(
		VERSION,
		Arc::new(|context: &BuildContext, request: BuildRequest| {
			let _lease = crate::runtime::admit(&mut |message| {
				let _ = std::io::Write::write_all(&mut std::io::stderr(), format!("{message}\n").as_bytes());
			});
			let built = run_api_build(context, &request);
			crate::memory::trim();
			built
		}),
	)
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
		if let Some(host) = &context.transformer_host {
			rotor_compile::transformer_host::with_transformer_host(&mut options, host);
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

#[cfg(test)]
mod tests {
	use std::io::{BufRead, BufReader, Write};

	use serde_json::{Value, json};

	use crate::test_fixtures::{buildable_project, must_write};

	/// A callback client keeps the text it was last sent (`tools/sidecar`'s session), so a build
	/// after an edit must send the edited file. A transformer host made per build started every
	/// build fresh, sent nothing, and the client transformed the text from before the edit.
	#[test]
	fn a_callback_build_sends_the_files_edited_since_the_last_build() {
		let (_temp, dir) = buildable_project("export const phase = \"start\";\n");
		let config_path = dir.join("tsconfig.json");
		let mut config: Value =
			serde_json::from_str(&std::fs::read_to_string(&config_path).expect("tsconfig")).expect("JSON");
		config["compilerOptions"]["plugins"] = json!([{"transform": "./transformer.js"}]);
		must_write(&config_path, &config.to_string());
		must_write(
			dir.join("transformer.js"),
			"module.exports = () => () => (file) => file;\n",
		);
		let main = dir.join("src/main.ts");

		let (input, to_server) = std::io::pipe().expect("pipe");
		let (from_server, output) = std::io::pipe().expect("pipe");
		let serving = std::thread::spawn(move || super::server().run(input, output));
		let mut client = Client {
			to_server,
			lines: BufReader::new(from_server).lines(),
		};

		client.send(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": 1, "clientVersion": crate::VERSION, "transformerCallbacks": true}}));
		assert!(client.next()["error"].is_null(), "initialize");
		let root = dir.to_string_lossy().into_owned();
		assert_eq!(
			client.build(2, &root),
			Vec::<Value>::new(),
			"the client's worker reads the first build from disk"
		);
		must_write(&main, "export const phase = \"next\";\n");
		let changed = client.build(3, &root);
		assert!(
			changed
				.iter()
				.any(|file| file["text"].as_str().is_some_and(|text| text.contains("next"))),
			"the second build sends the edit: {changed:?}"
		);
		client.send(&json!({"jsonrpc": "2.0", "id": 4, "method": "shutdown", "params": {}}));
		let _ = client.next();
		drop(client);
		let _ = serving.join();
	}

	/// The API client's end of the session.
	struct Client {
		to_server: std::io::PipeWriter,
		lines: std::io::Lines<BufReader<std::io::PipeReader>>,
	}

	impl Client {
		fn send(&mut self, message: &Value) {
			// One write: a split newline can reach a server that `shutdown` just made close its input.
			self.to_server
				.write_all(format!("{message}\n").as_bytes())
				.expect("send");
		}

		fn next(&mut self) -> Value {
			serde_json::from_str(&self.lines.next().expect("a line").expect("read")).expect("JSON")
		}

		/// Runs build `id`, answering its transformer callbacks; the files the transform requests
		/// sent as changed.
		fn build(&mut self, id: i64, root: &str) -> Vec<Value> {
			self.send(&json!({"jsonrpc": "2.0", "id": id, "method": "build", "params": {"roots": [root]}}));
			let mut changed = Vec::new();
			loop {
				let message = self.next();
				if message["method"] == "transform" {
					if message["params"]["operation"] == "transform" {
						changed.extend(
							message["params"]["changedFiles"]
								.as_array()
								.cloned()
								.unwrap_or_default(),
						);
					}
					self.send(&json!({"jsonrpc": "2.0", "id": message["id"], "result": {}}));
				} else if message["id"] == id {
					assert!(message["error"].is_null(), "build {id}: {message}");
					assert_eq!(message["result"]["ok"], json!(true), "build {id}: {message}");
					return changed;
				}
			}
		}
	}
}
