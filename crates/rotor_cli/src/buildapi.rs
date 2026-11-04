//! The port of `internal/buildapi`: the versioned build protocol and its session lifecycle.
//! Callers provide one build function; this module owns the framing, the handshake, request
//! correlation, cancellation and serialization.
//!
//! Messages are JSON values read as a stream (no size limit), and every response is one line, as
//! Go's `json.Encoder` writes it (HTML escaping included). A session builds at most one request
//! at a time, and a build may call back into the client for transformer requests.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rotor_compile::compile::DiagnosticInfo;
use rotor_compile::gojson;
use rotor_compile::project::Cancellation;
use rotor_compile::solution::{SolutionProjectResult, SolutionProjectSelection, SolutionProjectStatus};
use rotor_compile::timings::{BuildTimingCounts, BuildTimingStages, BuildTimings};
use rotor_compile::transformer_host::{
	TransformerCallback, TransformerRequest, TransformerResponse, TransformerTransportMetrics,
};
use rotor_support::gopath;
use serde_json::value::RawValue;

/// `ProtocolVersion`
pub const PROTOCOL_VERSION: i64 = 1;

const CAPABILITIES: [&str; 5] = [
	"build",
	"project-selection",
	"shutdown",
	"terminal-cancel",
	"transformer-callback",
];

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `Diagnostic`: the wire form of one compiler diagnostic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
	/// Relative to the project directory; empty without a location.
	pub file: String,
	/// 1-based.
	pub line: usize,
	/// 1-based.
	pub col: usize,
	/// Omitted when empty.
	pub code: String,
	/// `error` or `warning`.
	pub severity: String,
	/// The message.
	pub message: String,
}

impl Diagnostic {
	fn to_json(&self) -> String {
		gojson::Object::new()
			.string("file", &self.file)
			.int("line", i64::try_from(self.line).unwrap_or(i64::MAX))
			.int("col", i64::try_from(self.col).unwrap_or(i64::MAX))
			.string_omitempty("code", &self.code)
			.string("severity", &self.severity)
			.string("message", &self.message)
			.finish()
	}
}

/// `BuildTelemetry`
#[expect(clippy::struct_field_names, reason = "the wire names")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildTelemetry {
	/// `scheduledProjects`
	pub scheduled_projects: i64,
	/// `selectedProjects`
	pub selected_projects: i64,
	/// `satisfiedProjects`
	pub satisfied_projects: i64,
	/// `transformedProjects`
	pub transformed_projects: i64,
	/// `emittedProjects`
	pub emitted_projects: i64,
}

/// `ProjectResult`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectResult {
	/// Slash-separated.
	pub config: String,
	/// The terminal outcome.
	pub status: SolutionProjectStatus,
	/// Slash-separated.
	pub blockers: Vec<String>,
	/// Relative to the project.
	pub diagnostics: Vec<Diagnostic>,
	/// The project's outputs, as the coordinator reports them.
	pub outputs: Vec<String>,
	/// `len(outputs)`
	pub output_count: usize,
	/// `timings.durationMs`
	pub duration_ms: i64,
	/// `timings.stages`
	pub stages: BuildTimingStages,
	/// `timings.counts`
	pub counts: BuildTimingCounts,
}

/// `BuildResult`: a whole request's response.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildResult {
	/// No project failed.
	pub ok: bool,
	/// Outputs written; 0 on failure.
	pub files: usize,
	/// Wall time.
	pub duration_ms: i64,
	/// The failed projects' diagnostics, or the failure itself.
	pub diagnostics: Vec<Diagnostic>,
	/// Every output, absolute and slash-separated, sorted.
	pub outputs: Vec<String>,
	/// One entry per emitted config.
	pub projects: Vec<ProjectResult>,
	/// The drain's counts.
	pub telemetry: BuildTelemetry,
}

fn strings(values: &[String]) -> String {
	gojson::array(values.iter().map(|value| gojson::string(value)))
}

impl BuildResult {
	/// The result as Go's `json.Encoder` writes it.
	#[must_use]
	pub fn to_json(&self) -> String {
		let telemetry = self.telemetry;
		gojson::Object::new()
			.bool("ok", self.ok)
			.int("files", i64::try_from(self.files).unwrap_or(i64::MAX))
			.int("durationMs", self.duration_ms)
			.raw(
				"diagnostics",
				&gojson::array(self.diagnostics.iter().map(Diagnostic::to_json)),
			)
			.raw("outputs", &strings(&self.outputs))
			.raw(
				"projects",
				&gojson::array(self.projects.iter().map(|project| {
					gojson::Object::new()
						.string("config", &project.config)
						.string("status", project.status.as_str())
						.raw("blockers", &strings(&project.blockers))
						.raw(
							"diagnostics",
							&gojson::array(project.diagnostics.iter().map(Diagnostic::to_json)),
						)
						.raw("outputs", &strings(&project.outputs))
						.int("outputCount", i64::try_from(project.output_count).unwrap_or(i64::MAX))
						.raw(
							"timings",
							&gojson::Object::new()
								.int("durationMs", project.duration_ms)
								.raw("stages", &project.stages.to_json())
								.raw("counts", &project.counts.to_json())
								.finish(),
						)
						.finish()
				})),
			)
			.raw(
				"telemetry",
				&gojson::Object::new()
					.int("scheduledProjects", telemetry.scheduled_projects)
					.int("selectedProjects", telemetry.selected_projects)
					.int("satisfiedProjects", telemetry.satisfied_projects)
					.int("transformedProjects", telemetry.transformed_projects)
					.int("emittedProjects", telemetry.emitted_projects)
					.finish(),
			)
			.finish()
	}
}

/// `ResultFromSolution`: the coordinator's own outcomes, without rebuilding output or diagnostic
/// ownership from the aggregate result.
#[must_use]
pub fn result_from_solution(
	projects: &[SolutionProjectResult],
	timings: Option<&BuildTimings>,
	elapsed: Duration,
	error: Option<&str>,
) -> BuildResult {
	let mut response = BuildResult {
		ok: error.is_none(),
		duration_ms: i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
		..BuildResult::default()
	};
	if let Some(timings) = timings {
		let counts = timings.report().counts;
		response.telemetry = BuildTelemetry {
			scheduled_projects: counts.scheduled_projects,
			selected_projects: counts.selected_projects,
			satisfied_projects: counts.satisfied_projects,
			transformed_projects: counts.transformed_projects,
			emitted_projects: counts.emitted_projects,
		};
	}
	for project in projects {
		let project_dir = gopath::dir(&project.config_path);
		let wire = ProjectResult {
			config: gopath::to_slash(&project.config_path).into_owned(),
			status: project.status,
			blockers: project
				.blockers
				.iter()
				.map(|blocker| gopath::to_slash(blocker).into_owned())
				.collect(),
			diagnostics: diagnostics_from_compile(&project_dir, &project.diagnostics),
			outputs: project.outputs.clone(),
			output_count: project.output_count,
			duration_ms: project.timings.build_wall_ms,
			stages: project.timings.stages,
			counts: project.timings.counts,
		};
		response.files += project.output_count;
		response.outputs.extend(
			project
				.outputs
				.iter()
				.map(|output| project_output_path(&project_dir, output)),
		);
		if project.status == SolutionProjectStatus::Failed {
			response.diagnostics.extend(wire.diagnostics.iter().cloned());
		}
		response.projects.push(wire);
	}
	response.outputs.sort();
	if let Some(error) = error {
		response.files = 0;
		if response.diagnostics.is_empty() {
			response.diagnostics.push(Diagnostic {
				severity: "error".to_owned(),
				message: error.to_owned(),
				..Diagnostic::default()
			});
		}
	}
	response
}

fn project_output_path(project_dir: &str, output: &str) -> String {
	let mut path = gopath::from_slash(output).into_owned();
	if !gopath::is_abs(&path) {
		path = gopath::join(&[project_dir, &path]);
	}
	gopath::to_slash(&path).into_owned()
}

fn diagnostics_from_compile(project_dir: &str, diagnostics: &[DiagnosticInfo]) -> Vec<Diagnostic> {
	diagnostics
		.iter()
		.map(|diagnostic| Diagnostic {
			file: if diagnostic.file_name.is_empty() {
				String::new()
			} else {
				relative_path(project_dir, &diagnostic.file_name)
			},
			line: diagnostic.line,
			col: diagnostic.col,
			code: diagnostic.code.clone(),
			severity: if diagnostic.warning { "warning" } else { "error" }.to_owned(),
			message: diagnostic.message.clone(),
		})
		.collect()
}

fn relative_path(project_dir: &str, file_name: &str) -> String {
	gopath::rel(project_dir, file_name).map_or_else(
		|_| gopath::to_slash(file_name).into_owned(),
		|relative| gopath::to_slash(&relative).into_owned(),
	)
}

/// `BuildRequest`: the canonical request the build adapter gets.
#[derive(Clone, Debug, Default)]
pub struct BuildRequest {
	/// The roots to build.
	pub roots: Vec<String>,
	/// The external scheduler's ownership decision, if any.
	pub selection: Option<SolutionProjectSelection>,
}

/// What a build runs under: its cancellation (with the cause that ended the session, if any) and
/// the transformer callback when the client asked for one.
#[derive(Clone)]
pub struct BuildContext {
	/// Cancelled when the session ends or shuts down.
	pub cancel: Cancellation,
	cause: Arc<Mutex<Option<String>>>,
	/// `TransformerCallbackFromContext`
	pub transformer_callback: Option<TransformerCallback>,
}

impl BuildContext {
	/// `context.Cause`: the transport failure that ended the session; `None` for a plain cancel.
	#[must_use]
	pub fn cause(&self) -> Option<String> {
		lock(&self.cause).clone()
	}

	/// The [`BuildError`] for a failure the build saw: a cancellation if the session was cancelled
	/// without a cause (Go's `errors.Is(err, context.Canceled)`), else the failure's text.
	#[must_use]
	#[cfg_attr(not(test), expect(dead_code, reason = "the CLI's builds fail through their results"))]
	pub fn error(&self, error: impl std::fmt::Display) -> BuildError {
		if self.cancel.is_cancelled() && self.cause().is_none() {
			return BuildError::Cancelled;
		}
		BuildError::Failed(error.to_string())
	}
}

/// Why a build produced no result. Build failures belong in [`BuildResult`]; these are
/// cancellations and server failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
	/// The session cancelled the build.
	Cancelled,
	/// Anything else.
	#[cfg_attr(not(test), expect(dead_code, reason = "the CLI's builds fail through their results"))]
	Failed(String),
}

/// `BuildFunc`
pub type BuildFunc = Arc<dyn Fn(&BuildContext, BuildRequest) -> Result<BuildResult, BuildError> + Send + Sync>;

type CallbackResult = Result<String, String>;

struct Active {
	id: i64,
	cancel: Cancellation,
	cause: Arc<Mutex<Option<String>>>,
}

/// `Server`: one connection and at most one active build.
pub struct Server {
	version: String,
	build: BuildFunc,
	out: Mutex<Option<Box<dyn Write + Send>>>,
	write_error: Mutex<Option<String>>,
	active: Mutex<Option<Active>>,
	builds: (Mutex<usize>, Condvar),
	initialized: AtomicBool,
	callbacks_enabled: AtomicBool,
	next_callback: AtomicU64,
	callbacks: Mutex<HashMap<String, Sender<CallbackResult>>>,
}

/// A message's fields, each as its raw JSON.
type Fields = serde_json::Map<String, serde_json::Value>;

impl Server {
	/// `NewServer`
	pub fn new(version: &str, build: BuildFunc) -> Arc<Self> {
		Arc::new(Self {
			version: version.to_owned(),
			build,
			out: Mutex::new(None),
			write_error: Mutex::new(None),
			active: Mutex::new(None),
			builds: (Mutex::new(0), Condvar::new()),
			initialized: AtomicBool::new(false),
			callbacks_enabled: AtomicBool::new(false),
			next_callback: AtomicU64::new(0),
			callbacks: Mutex::new(HashMap::new()),
		})
	}

	/// `Run`: serves one connection until `shutdown` or the end of input.
	///
	/// # Errors
	///
	/// A malformed or unreadable request stream, a protocol violation, or a failed write.
	pub fn run(self: &Arc<Self>, input: impl Read, output: impl Write + Send + 'static) -> Result<(), String> {
		*lock(&self.out) = Some(Box::new(output));
		let served = self.serve(input);
		// No cause reports the active build as cancelled; any other cause is its failure.
		self.stop_active_build(served.as_ref().err().cloned());
		self.wait_builds();
		served?;
		self.response_error().map_or(Ok(()), Err)
	}

	fn serve(self: &Arc<Self>, input: impl Read) -> Result<(), String> {
		let stream = serde_json::Deserializer::from_reader(std::io::BufReader::new(input)).into_iter::<Box<RawValue>>();
		for message in stream {
			let message = message.map_err(|error| {
				if error.is_io() {
					return format!("build API: read request: {}", io_text(&error));
				}
				if error.is_eof() {
					return "build API: read request: unexpected EOF".to_owned();
				}
				format!("build API: malformed request: {error}")
			})?;
			// A top-level `null` decodes into Go's zero message.
			let fields = if message.get().trim() == "null" {
				Ok(Fields::new())
			} else {
				serde_json::from_str::<Fields>(message.get())
			};
			let Ok(fields) = fields else {
				return Err(format!(
					"build API: malformed request: json: cannot unmarshal {} into Go value of type buildapi.incomingMessage",
					json_kind(message.get())
				));
			};
			let field_string = |name: &str| -> Result<String, String> {
				match fields.get(name) {
					None | Some(serde_json::Value::Null) => Ok(String::new()),
					Some(serde_json::Value::String(text)) => Ok(text.clone()),
					Some(other) => Err(format!(
						"build API: malformed request: json: cannot unmarshal {} into Go struct field incomingMessage.{name} of type string",
						value_kind(other)
					)),
				}
			};
			if field_string("jsonrpc")? != "2.0" {
				return Err("build API: message jsonrpc must be \"2.0\"".to_owned());
			}
			let method = field_string("method")?;
			if method.is_empty() {
				self.complete_callback(&fields, message.get())?;
				continue;
			}
			let id = match fields.get("id") {
				None => return Err("build API: request is missing an id".to_owned()),
				Some(serde_json::Value::Null) => 0,
				Some(value) => value
					.as_i64()
					.ok_or_else(|| "build API: client request id must be an integer".to_owned())?,
			};
			let params = fields.get("params");
			match method.as_str() {
				"initialize" => self.initialize(id, params),
				"build" => self.start_build(id, params),
				"shutdown" => {
					self.stop_active_build(None);
					self.wait_builds();
					self.respond(id, Some("{}"), None);
					return Ok(());
				}
				_ => self.respond_error(id, "METHOD_NOT_FOUND", &format!("unknown method {}", go_quote(&method))),
			}
			if let Some(error) = self.response_error() {
				return Err(error);
			}
		}
		Ok(())
	}

	fn initialize(&self, id: i64, params: Option<&serde_json::Value>) {
		if self.initialized.load(Ordering::SeqCst) {
			self.respond_error(id, "ALREADY_INITIALIZED", "the session is already initialized");
			return;
		}
		let params = match decode_params(params, "initializeParams") {
			Ok(params) => params,
			Err(error) => {
				self.respond_error(id, "INVALID_REQUEST", &error);
				return;
			}
		};
		let protocol = match int_param(&params, "protocolVersion", "initializeParams") {
			Ok(protocol) => protocol,
			Err(error) => return self.respond_error(id, "INVALID_REQUEST", &error),
		};
		let client_version = match string_param(&params, "clientVersion", "initializeParams") {
			Ok(version) => version,
			Err(error) => return self.respond_error(id, "INVALID_REQUEST", &error),
		};
		let callbacks = match bool_param(&params, "transformerCallbacks", "initializeParams") {
			Ok(callbacks) => callbacks,
			Err(error) => return self.respond_error(id, "INVALID_REQUEST", &error),
		};
		if protocol != PROTOCOL_VERSION {
			self.respond_error(
				id,
				"PROTOCOL_VERSION_MISMATCH",
				&format!("client protocol {protocol} is incompatible with server protocol {PROTOCOL_VERSION}"),
			);
			return;
		}
		if client_version != self.version {
			self.respond_error(
				id,
				"VERSION_MISMATCH",
				&format!(
					"client version {client_version} does not match server version {}",
					self.version
				),
			);
			return;
		}
		self.initialized.store(true, Ordering::SeqCst);
		self.callbacks_enabled.store(callbacks, Ordering::SeqCst);
		let result = gojson::Object::new()
			.int("protocolVersion", PROTOCOL_VERSION)
			.string("serverVersion", &self.version)
			.raw(
				"capabilities",
				&gojson::array(CAPABILITIES.iter().map(|capability| gojson::string(capability))),
			)
			.finish();
		self.respond(id, Some(&result), None);
	}

	#[expect(clippy::too_many_lines, reason = "one function in Go")]
	fn start_build(self: &Arc<Self>, id: i64, params: Option<&serde_json::Value>) {
		if !self.initialized.load(Ordering::SeqCst) {
			self.respond_error(id, "NOT_INITIALIZED", "initialize must complete before build");
			return;
		}
		let params = match decode_params(params, "buildParams") {
			Ok(params) => params,
			Err(error) => {
				self.respond_error(id, "INVALID_REQUEST", &error);
				return;
			}
		};
		let decoded = (|| -> Result<_, String> {
			Ok((
				string_param(&params, "project", "buildParams")?,
				strings_param(&params, "roots", "buildParams")?,
				strings_param(&params, "selected", "buildParams")?,
				strings_param(&params, "satisfied", "buildParams")?,
			))
		})();
		let (project, roots, selected, satisfied) = match decoded {
			Ok(decoded) => decoded,
			Err(error) => {
				self.respond_error(id, "INVALID_REQUEST", &error);
				return;
			}
		};
		let roots = roots.unwrap_or_default();
		if !project.is_empty() && !roots.is_empty() {
			self.respond_error(id, "INVALID_REQUEST", "provide project or roots, not both");
			return;
		}
		let roots = if project.is_empty() { roots } else { vec![project] };
		if roots.is_empty() {
			self.respond_error(id, "INVALID_REQUEST", "roots must contain at least one path");
			return;
		}
		if roots.iter().any(String::is_empty) {
			self.respond_error(id, "INVALID_REQUEST", "roots must contain only non-empty paths");
			return;
		}
		if selected.is_none() != satisfied.is_none() {
			self.respond_error(
				id,
				"INVALID_REQUEST",
				"provide selected and satisfied together, or omit both",
			);
			return;
		}
		let selection = match (selected, satisfied) {
			(Some(selected), Some(satisfied)) => {
				if selected.iter().chain(&satisfied).any(String::is_empty) {
					self.respond_error(
						id,
						"INVALID_REQUEST",
						"selected and satisfied must contain only non-empty paths",
					);
					return;
				}
				Some(SolutionProjectSelection { selected, satisfied })
			}
			_ => None,
		};

		let (cancel, cause) = (Cancellation::default(), Arc::new(Mutex::new(None)));
		{
			let mut active = lock(&self.active);
			if active.is_some() {
				drop(active);
				self.respond_error(id, "BUILD_IN_PROGRESS", "the session already has an active build");
				return;
			}
			*active = Some(Active {
				id,
				cancel: cancel.clone(),
				cause: Arc::clone(&cause),
			});
		}
		let context = BuildContext {
			cancel,
			cause,
			transformer_callback: self.callbacks_enabled.load(Ordering::SeqCst).then(|| {
				let server = Arc::clone(self);
				let callback: TransformerCallback =
					Arc::new(move |cancel, request| server.call_transformer(cancel, &request));
				callback
			}),
		};
		*lock(&self.builds.0) += 1;
		let server = Arc::clone(self);
		let request = BuildRequest { roots, selection };
		let spawned = std::thread::Builder::new()
			.name("rotor-build-api".to_owned())
			.spawn(move || {
				let built = (server.build)(&context, request);
				{
					let mut active = lock(&server.active);
					if active.as_ref().is_some_and(|active| active.id == id) {
						*active = None;
					}
				}
				match built {
					Err(BuildError::Cancelled) => match context.cause() {
						Some(cause) => server.respond_error(id, "BUILD_FAILED", &cause),
						None => server.respond_error(id, "BUILD_CANCELLED", "the build was cancelled"),
					},
					Err(BuildError::Failed(error)) => server.respond_error(id, "BUILD_FAILED", &error),
					Ok(result) => server.respond(id, Some(&result.to_json()), None),
				}
				let (count, done) = &server.builds;
				*lock(count) -= 1;
				done.notify_all();
			});
		if spawned.is_err() {
			*lock(&self.active) = None;
			*lock(&self.builds.0) -= 1;
			self.respond_error(id, "BUILD_FAILED", "build API: could not start the build");
		}
	}

	/// `callTransformer`: sends the request back to the client and waits for its answer, or for
	/// the build's cancellation.
	fn call_transformer(
		&self,
		cancel: Option<&Cancellation>,
		request: &TransformerRequest,
	) -> Result<TransformerResponse, rotor_compile::Error> {
		let id = format!("transform-{}", self.next_callback.fetch_add(1, Ordering::SeqCst) + 1);
		let (sender, answer) = mpsc::channel();
		lock(&self.callbacks).insert(id.clone(), sender);
		let mut payload = String::from("{\"jsonrpc\":\"2.0\",\"id\":");
		gojson::write_string(&mut payload, &id);
		payload.push_str(",\"method\":\"transform\",\"params\":");
		payload.push_str(&request.to_json());
		payload.push('}');
		let request_bytes = i64::try_from(payload.len()).unwrap_or(i64::MAX);
		self.write_line(&payload, "callback");
		if let Some(error) = self.response_error() {
			lock(&self.callbacks).remove(&id);
			return Err(rotor_compile::Error::message(error));
		}
		let cause = || {
			lock(&self.active)
				.as_ref()
				.and_then(|active| lock(&active.cause).clone())
				.unwrap_or_else(|| "context canceled".to_owned())
		};
		loop {
			match answer.recv_timeout(Duration::from_millis(20)) {
				Ok(Ok(result)) => {
					let mut response = TransformerResponse::from_json(&result).map_err(|error| {
						rotor_compile::Error::message(format!(
							"build API: invalid transformer callback response: {error}"
						))
					})?;
					response.transport = Some(TransformerTransportMetrics {
						request_bytes,
						response_bytes: i64::try_from(result.len()).unwrap_or(i64::MAX),
					});
					return Ok(response);
				}
				Ok(Err(error)) => return Err(rotor_compile::Error::message(error)),
				Err(mpsc::RecvTimeoutError::Timeout) if cancel.is_some_and(Cancellation::is_cancelled) => {
					lock(&self.callbacks).remove(&id);
					return Err(rotor_compile::Error::message(format!(
						"build API: transformer callback cancelled: {}",
						cause()
					)));
				}
				Err(mpsc::RecvTimeoutError::Timeout) => {}
				Err(mpsc::RecvTimeoutError::Disconnected) => {
					return Err(rotor_compile::Error::message(format!(
						"build API: transformer callback cancelled: {}",
						cause()
					)));
				}
			}
		}
	}

	/// `completeCallback`
	fn complete_callback(&self, fields: &Fields, raw: &str) -> Result<(), String> {
		let id = match fields.get("id") {
			Some(serde_json::Value::String(id)) if !id.is_empty() => id.clone(),
			_ => return Err("build API: callback response id must be a string".to_owned()),
		};
		let Some(pending) = lock(&self.callbacks).remove(&id) else {
			return Err(format!("build API: callback response has unknown id {}", go_quote(&id)));
		};
		if let Some(error) = fields.get("error").filter(|error| !error.is_null()) {
			let code = error
				.get("code")
				.and_then(serde_json::Value::as_str)
				.unwrap_or_default();
			let message = error
				.get("message")
				.and_then(serde_json::Value::as_str)
				.unwrap_or_default();
			let _ = pending.send(Err(format!("transformer callback {code}: {message}")));
			return Ok(());
		}
		// The result's own bytes, as the client sent them.
		let result = serde_json::from_str::<HashMap<String, Box<RawValue>>>(raw)
			.ok()
			.and_then(|mut fields| fields.remove("result"))
			.map(|result| result.get().to_owned());
		match result {
			None => {
				let _ = pending.send(Err("transformer callback response is missing a result".to_owned()));
			}
			Some(result) => {
				let _ = pending.send(Ok(result));
			}
		}
		Ok(())
	}

	fn stop_active_build(&self, cause: Option<String>) {
		if let Some(active) = lock(&self.active).as_ref() {
			if let Some(cause) = cause {
				*lock(&active.cause) = Some(cause);
			}
			active.cancel.cancel();
		}
	}

	#[expect(
		clippy::significant_drop_tightening,
		reason = "the condition variable needs the guard"
	)]
	fn wait_builds(&self) {
		let (count, done) = &self.builds;
		let mut count = lock(count);
		while *count > 0 {
			count = done.wait(count).unwrap_or_else(PoisonError::into_inner);
		}
	}

	fn respond_error(&self, id: i64, code: &str, message: &str) {
		let error = gojson::Object::new()
			.string("code", code)
			.string("message", message)
			.finish();
		self.respond(id, None, Some(&error));
	}

	/// `respond`: one response line.
	fn respond(&self, id: i64, result: Option<&str>, error: Option<&str>) {
		let mut message = gojson::Object::new().string("jsonrpc", "2.0").int("id", id);
		if let Some(result) = result {
			message = message.raw("result", result);
		}
		if let Some(error) = error {
			message = message.raw("error", error);
		}
		self.write_line(&message.finish(), "response");
	}

	fn write_line(&self, message: &str, what: &str) {
		let mut guard = lock(&self.out);
		let Some(out) = guard.as_mut() else {
			return;
		};
		let mut line = message.as_bytes().to_vec();
		line.push(b'\n');
		let written = out.write_all(&line).and_then(|()| out.flush());
		drop(guard);
		if let Err(error) = written {
			let mut stored = lock(&self.write_error);
			if stored.is_none() {
				*stored = Some(format!("build API: write {what}: {error}"));
			}
		}
	}

	fn response_error(&self) -> Option<String> {
		lock(&self.write_error).clone()
	}
}

/// The text of an I/O error inside a `serde_json` error.
fn io_text(error: &serde_json::Error) -> String {
	let text = error.to_string();
	// serde_json appends " at line L column C" to some errors; an I/O error has its own text.
	text.split(" at line ").next().unwrap_or(&text).to_owned()
}

fn json_kind(raw: &str) -> &'static str {
	match raw.trim_start().as_bytes().first() {
		Some(b'[') => "array",
		Some(b'"') => "string",
		Some(b't' | b'f') => "bool",
		Some(b'n') => "null",
		_ => "number",
	}
}

const fn value_kind(value: &serde_json::Value) -> &'static str {
	match value {
		serde_json::Value::Null => "null",
		serde_json::Value::Bool(_) => "bool",
		serde_json::Value::Number(_) => "number",
		serde_json::Value::String(_) => "string",
		serde_json::Value::Array(_) => "array",
		serde_json::Value::Object(_) => "object",
	}
}

/// `%q` of a string.
fn go_quote(text: &str) -> String {
	let mut out = String::new();
	gojson::write_string_escaping(&mut out, text, false);
	out
}

/// `decodeParams`
fn decode_params(params: Option<&serde_json::Value>, owner: &str) -> Result<Fields, String> {
	match params {
		None => Err("params are required".to_owned()),
		Some(serde_json::Value::Null) => Ok(Fields::new()),
		Some(serde_json::Value::Object(fields)) => Ok(fields.clone()),
		Some(other) => Err(format!(
			"invalid params: json: cannot unmarshal {} into Go value of type buildapi.{owner}",
			value_kind(other)
		)),
	}
}

/// `json.UnmarshalTypeError`'s text; a number names its literal, as Go's does.
fn type_error(owner: &str, name: &str, value: &serde_json::Value, want: &str) -> String {
	let kind = match value {
		serde_json::Value::Number(number) => format!("number {number}"),
		other => value_kind(other).to_owned(),
	};
	format!("invalid params: json: cannot unmarshal {kind} into Go struct field {owner}.{name} of type {want}")
}

fn int_param(params: &Fields, name: &str, owner: &str) -> Result<i64, String> {
	match params.get(name) {
		None | Some(serde_json::Value::Null) => Ok(0),
		Some(value) => value.as_i64().ok_or_else(|| type_error(owner, name, value, "int")),
	}
}

fn string_param(params: &Fields, name: &str, owner: &str) -> Result<String, String> {
	match params.get(name) {
		None | Some(serde_json::Value::Null) => Ok(String::new()),
		Some(serde_json::Value::String(text)) => Ok(text.clone()),
		Some(value) => Err(type_error(owner, name, value, "string")),
	}
}

fn bool_param(params: &Fields, name: &str, owner: &str) -> Result<bool, String> {
	match params.get(name) {
		None | Some(serde_json::Value::Null) => Ok(false),
		Some(serde_json::Value::Bool(value)) => Ok(*value),
		Some(value) => Err(type_error(owner, name, value, "bool")),
	}
}

/// A `[]string` field: `None` when absent or `null` (Go's nil slice).
fn strings_param(params: &Fields, name: &str, owner: &str) -> Result<Option<Vec<String>>, String> {
	match params.get(name) {
		None | Some(serde_json::Value::Null) => Ok(None),
		Some(serde_json::Value::Array(values)) => values
			.iter()
			.map(|value| {
				value
					.as_str()
					.map(str::to_owned)
					.ok_or_else(|| type_error(owner, name, value, "string"))
			})
			.collect::<Result<Vec<_>, _>>()
			.map(Some),
		Some(value) => Err(type_error(owner, name, value, "[]string")),
	}
}

#[cfg(test)]
mod tests {
	//! `server_test.go`

	use std::io::{self, Read, Write};
	use std::sync::mpsc::{self, Receiver, Sender};
	use std::sync::{Arc, Condvar, Mutex};
	use std::time::{Duration, Instant};

	use rotor_compile::solution::{SolutionProjectResult, SolutionProjectStatus};
	use rotor_compile::transformer_host::TransformerRequest;
	use rotor_support::gopath;
	use serde_json::{Value, json};

	use super::{BuildContext, BuildError, BuildFunc, BuildRequest, BuildResult, Server, result_from_solution};

	/// `serverTestTimeout`: bounds every wait on the server, so a regression fails instead of
	/// hanging the suite.
	const SERVER_TEST_TIMEOUT: Duration = Duration::from_secs(5);

	/// The client's end of the request stream (`io.Pipe`'s writer): each chunk, or the read error
	/// `CloseWithError` hands the server; dropping it is EOF.
	struct Input(Sender<io::Result<Vec<u8>>>);

	impl Input {
		fn write(&self, data: &[u8]) {
			let _ = self.0.send(Ok(data.to_vec()));
		}

		fn close_with_error(&self, message: &str) {
			let _ = self.0.send(Err(io::Error::other(message.to_owned())));
		}
	}

	struct InputReader {
		chunks: Receiver<io::Result<Vec<u8>>>,
		pending: io::Cursor<Vec<u8>>,
	}

	impl Read for InputReader {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			loop {
				let count = self.pending.read(buf)?;
				if count > 0 || buf.is_empty() {
					return Ok(count);
				}
				match self.chunks.recv() {
					Ok(chunk) => self.pending = io::Cursor::new(chunk?),
					Err(_) => return Ok(0),
				}
			}
		}
	}

	fn pipe() -> (InputReader, Input) {
		let (sender, chunks) = mpsc::channel();
		(
			InputReader {
				chunks,
				pending: io::Cursor::default(),
			},
			Input(sender),
		)
	}

	/// What the server wrote, and how far the test has read it.
	type Written = (Mutex<(Vec<u8>, usize)>, Condvar);

	/// The server's output, read line by line with a timeout.
	#[derive(Clone, Default)]
	struct Output(Arc<Written>);

	impl Write for Output {
		fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
			let (state, wake) = &*self.0;
			state.lock().expect("output").0.extend_from_slice(buf);
			wake.notify_all();
			Ok(buf.len())
		}

		fn flush(&mut self) -> io::Result<()> {
			Ok(())
		}
	}

	impl Output {
		/// `readJSONLine`
		fn read_json_line(&self) -> Value {
			let deadline = Instant::now() + SERVER_TEST_TIMEOUT;
			let (state, wake) = &*self.0;
			let mut guard = state.lock().expect("output");
			loop {
				let (data, read) = &mut *guard;
				if let Some(end) = data[*read..].iter().position(|&byte| byte == b'\n') {
					let line = data[*read..*read + end].to_vec();
					*read += end + 1;
					return serde_json::from_slice(&line).expect("one JSON line");
				}
				let now = Instant::now();
				assert!(now < deadline, "no response from server");
				guard = wake.wait_timeout(guard, deadline - now).expect("output").0;
			}
		}
	}

	/// `writeJSONLine`
	fn write_json_line(input: &Input, value: &Value) {
		let mut line = serde_json::to_vec(value).expect("JSON");
		line.push(b'\n');
		input.write(&line);
	}

	fn empty_build_result() -> BuildResult {
		BuildResult::default()
	}

	fn server(
		build: impl Fn(&BuildContext, BuildRequest) -> Result<BuildResult, BuildError> + Send + Sync + 'static,
	) -> Arc<Server> {
		let build: BuildFunc = Arc::new(build);
		Server::new("2.7.1", build)
	}

	fn start(
		server: &Arc<Server>,
		reader: impl Read + Send + 'static,
		output: impl Write + Send + 'static,
	) -> Receiver<Result<(), String>> {
		let (done, finished) = mpsc::channel();
		let server = Arc::clone(server);
		std::thread::spawn(move || {
			let _ = done.send(server.run(reader, output));
		});
		finished
	}

	struct CallbackSession {
		input: Input,
		output: Output,
		done: Receiver<Result<(), String>>,
		callback_id: String,
	}

	impl CallbackSession {
		fn wait(&self) -> Result<(), String> {
			self.done
				.recv_timeout(SERVER_TEST_TIMEOUT)
				.expect("server did not stop")
		}
	}

	/// `startCallbackSession`: initializes the server, starts build 2, and reads the first
	/// transformer callback request.
	fn start_callback_session(server: &Arc<Server>) -> CallbackSession {
		let (reader, input) = pipe();
		let output = Output::default();
		let done = start(server, reader, output.clone());
		write_json_line(
			&input,
			&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": 1, "clientVersion": "2.7.1", "transformerCallbacks": true}}),
		);
		output.read_json_line();
		write_json_line(
			&input,
			&json!({"jsonrpc": "2.0", "id": 2, "method": "build", "params": {"roots": ["fixture"]}}),
		);
		let callback = output.read_json_line();
		let callback_id = callback["id"].as_str().map(str::to_owned);
		assert!(
			callback["method"] == "transform" && callback_id.is_some(),
			"callback request = {callback}"
		);
		CallbackSession {
			input,
			output,
			done,
			callback_id: callback_id.unwrap_or_default(),
		}
	}

	fn call(
		context: &BuildContext,
		request: TransformerRequest,
	) -> Result<rotor_compile::transformer_host::TransformerResponse, rotor_compile::Error> {
		let callback = context
			.transformer_callback
			.as_ref()
			.expect("build context has no transformer callback");
		callback(Some(&context.cancel), request)
	}

	fn request(operation: &str, project_dir: &str, ts_config_path: &str) -> TransformerRequest {
		TransformerRequest {
			protocol: 1,
			operation: operation.to_owned(),
			project_dir: project_dir.to_owned(),
			ts_config_path: ts_config_path.to_owned(),
			..TransformerRequest::default()
		}
	}

	#[test]
	fn server_routes_transformer_callbacks() {
		let server = server(|context, request| {
			let root = &request.roots[0];
			let response = call(
				context,
				self::request("validate", root, &format!("{root}/tsconfig.json")),
			)
			.map_err(|error| context.error(error))?;
			assert!(
				response.diagnostics.len() == 1 && response.diagnostics[0].code == "fixture",
				"callback response = {response:?}"
			);
			assert!(
				response
					.transport
					.is_some_and(|transport| transport.request_bytes != 0 && transport.response_bytes != 0),
				"callback transport metrics = {:?}, want measured request and response bytes",
				response.transport
			);
			Ok(empty_build_result())
		});
		let session = start_callback_session(&server);
		write_json_line(
			&session.input,
			&json!({
				"jsonrpc": "2.0",
				"id": session.callback_id,
				"result": {
					"diagnostics": [{"category": "error", "code": "fixture", "message": "fixture"}],
					"transformed": [],
				},
			}),
		);
		let build = session.output.read_json_line();
		assert!(
			build["id"] == 2 && build.get("error").is_none(),
			"build response = {build}"
		);
		write_json_line(
			&session.input,
			&json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}}),
		);
		session.output.read_json_line();
		session.wait().expect("Run()");
	}

	#[test]
	fn server_transforms_natively_unless_the_client_asks_for_callbacks() {
		// Given: a client that does not ask for transformer callbacks.
		let (has_callback, told) = mpsc::channel();
		let has_callback = Mutex::new(has_callback);
		let server = server(move |context, _| {
			let _ = has_callback
				.lock()
				.expect("sender")
				.send(context.transformer_callback.is_some());
			Ok(empty_build_result())
		});
		let (reader, input) = pipe();
		let output = Output::default();
		let _done = start(&server, reader, output.clone());

		// When: it initializes and builds.
		write_json_line(
			&input,
			&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": 1, "clientVersion": "2.7.1"}}),
		);
		output.read_json_line();
		write_json_line(
			&input,
			&json!({"jsonrpc": "2.0", "id": 2, "method": "build", "params": {"roots": ["fixture"]}}),
		);
		let build = output.read_json_line();

		// Then: the build transforms in the server, never calling back.
		assert!(
			build["id"] == 2 && build.get("error").is_none(),
			"build response = {build}"
		);
		assert!(
			!told.recv_timeout(SERVER_TEST_TIMEOUT).expect("build ran"),
			"build context has a transformer callback the client did not ask for"
		);
	}

	#[test]
	fn server_eof_unblocks_transformer_callback() {
		let (started, callback_started) = mpsc::channel();
		let (returned, build_returned) = mpsc::channel();
		let (started, returned) = (Mutex::new(started), Mutex::new(returned));
		let server = server(move |context, _| {
			let _ = started.lock().expect("sender").send(());
			let called = call(context, request("validate", "fixture", "fixture/tsconfig.json"));
			let _ = returned
				.lock()
				.expect("sender")
				.send(called.as_ref().err().map(ToString::to_string));
			Err(called.map_or_else(|error| context.error(error), |_| BuildError::Failed(String::new())))
		});
		let (reader, input) = pipe();
		let done = start(&server, reader, Output::default());
		write_json_line(
			&input,
			&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": 1, "clientVersion": "2.7.1", "transformerCallbacks": true}}),
		);
		write_json_line(
			&input,
			&json!({"jsonrpc": "2.0", "id": 2, "method": "build", "params": {"roots": ["fixture"]}}),
		);
		callback_started
			.recv_timeout(SERVER_TEST_TIMEOUT)
			.expect("callback started");
		drop(input);

		let error = build_returned
			.recv_timeout(SERVER_TEST_TIMEOUT)
			.expect("callback stayed blocked after EOF");
		assert!(error.is_some(), "callback unexpectedly succeeded after EOF");
		let outcome = done
			.recv_timeout(SERVER_TEST_TIMEOUT)
			.expect("server stayed blocked after EOF");
		assert_eq!(outcome, Ok(()), "Run() error, want nil after EOF");
	}

	#[test]
	fn server_rejects_invalid_jsonrpc_requests() {
		for (name, envelope) in [
			("missing", "{\"id\":1,\"method\":\"initialize\",\"params\":{}}\n"),
			(
				"wrong",
				"{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
			),
		] {
			let server = server(|_, _| panic!("build must not run"));
			let error = server.run(envelope.as_bytes(), io::sink());
			assert!(
				error.as_ref().is_err_and(|error| error.contains("jsonrpc")),
				"{name}: Run() error = {error:?}, want invalid jsonrpc error"
			);
		}
	}

	#[test]
	fn server_rejects_invalid_jsonrpc_callback_responses() {
		let server = server(|context, _| {
			let called = call(context, request("validate", "", ""));
			Err(called.map_or_else(|error| context.error(error), |_| BuildError::Failed(String::new())))
		});
		let session = start_callback_session(&server);
		write_json_line(
			&session.input,
			&json!({"jsonrpc": "1.0", "id": session.callback_id, "result": {}}),
		);
		session.output.read_json_line();
		let error = session.wait();
		assert!(
			error.as_ref().is_err_and(|error| error.contains("jsonrpc")),
			"Run() error = {error:?}, want invalid jsonrpc error"
		);
	}

	#[test]
	fn server_decodes_callback_responses_larger_than_16_mib() {
		// Given: a transformer callback whose response is larger than 16 MiB.
		let transformed = "x".repeat(17 * 1024 * 1024);
		let want = transformed.clone();
		let server = server(move |context, _| {
			let response = call(context, request("transform", "", "")).map_err(|error| context.error(error))?;
			assert!(
				response.transformed.len() == 1 && response.transformed[0].text == want,
				"callback response lost the large transformed text"
			);
			Ok(empty_build_result())
		});
		let session = start_callback_session(&server);

		// When: the client answers the callback with the large payload.
		write_json_line(
			&session.input,
			&json!({
				"jsonrpc": "2.0",
				"id": session.callback_id,
				"result": {"diagnostics": [], "transformed": [{"fileName": "main.ts", "text": transformed}]},
			}),
		);

		// Then: the server decodes it and completes the build.
		let build = session.output.read_json_line();
		assert!(
			build["id"] == 2 && build.get("error").is_none(),
			"build response = {build}"
		);
		write_json_line(
			&session.input,
			&json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}}),
		);
		session.output.read_json_line();
		session.wait().expect("Run()");
	}

	#[test]
	fn server_transport_failure_names_cause_in_build_response() {
		type Fail = fn(&Input);
		let failures: [(&str, Fail); 2] = [
			("read error", |input| input.close_with_error("pipe broke")),
			("malformed", |input| input.write(b"{not json\n")),
		];
		for (name, fail) in failures {
			// Given: a build waiting on a transformer callback.
			let (sender, callback_error) = mpsc::channel();
			let sender = Mutex::new(sender);
			let server = server(move |context, _| {
				let called = call(context, request("transform", "", ""));
				let _ = sender
					.lock()
					.expect("sender")
					.send(called.as_ref().err().map(ToString::to_string));
				Err(called.map_or_else(|error| context.error(error), |_| BuildError::Failed(String::new())))
			});
			let session = start_callback_session(&server);

			// When: the request stream fails before the callback completes.
			fail(&session.input);

			// Then: the build response names the transport failure, not a client cancellation, and
			// Run returns the same cause.
			let build = session.output.read_json_line();
			let build_error = &build["error"];
			assert!(
				build_error.is_object() && build_error["code"] != "BUILD_CANCELLED",
				"{name}: build response = {build}, want a transport failure"
			);
			let message = build_error["message"].as_str().unwrap_or_default().to_owned();
			let error = session.wait().expect_err("Run() error");
			assert!(
				message.contains(&error),
				"{name}: build error message = {message:?}, Run() error = {error}; want the Run error in the message"
			);
			// The callback itself fails with the cause, not a bare cancel.
			let got = callback_error
				.recv_timeout(SERVER_TEST_TIMEOUT)
				.expect("callback returned");
			assert!(
				got.as_ref().is_some_and(|got| got.contains(&error)),
				"{name}: callback error = {got:?}, want it to name {error}"
			);
		}
	}

	#[test]
	fn result_from_solution_uses_project_anchored_aggregate_outputs_for_every_batch_size() {
		let root = tempfile::tempdir().expect("temp dir");
		let root = root.path().to_string_lossy().into_owned();
		let project = |name: &str| SolutionProjectResult {
			config_path: gopath::join(&[&root, name, "tsconfig.json"]),
			status: SolutionProjectStatus::Success,
			blockers: Vec::new(),
			diagnostics: Vec::new(),
			outputs: vec!["out/main.luau".to_owned()],
			output_count: 1,
			timings: rotor_compile::timings::ProjectBuildTimings::default(),
		};
		let (first, second) = (project("first"), project("second"));
		let want_first = gopath::to_slash(&gopath::join(&[&root, "first", "out", "main.luau"])).into_owned();
		let want_second = gopath::to_slash(&gopath::join(&[&root, "second", "out", "main.luau"])).into_owned();

		let got = result_from_solution(std::slice::from_ref(&first), None, Duration::ZERO, None).outputs;
		assert_eq!(
			got,
			std::slice::from_ref(&want_first),
			"batch of one outputs, want project-anchored"
		);
		let got = result_from_solution(&[first, second], None, Duration::ZERO, None).outputs;
		assert_eq!(
			got,
			[want_first, want_second],
			"batch of two outputs, want project-anchored"
		);
	}

	/// `timing_schema_contract_test.go`: Go's reflection over the JSON tags becomes the keys a zero
	/// value writes (required) and the extra keys an all-ones value writes (`omitempty`).
	mod timing_schema_contract {
		use rotor_compile::timings::{BuildTimingCounts, BuildTimingStages};
		use serde_json::Value;

		fn keys(json: &str) -> Vec<String> {
			let value: Value = serde_json::from_str(json).expect("timing JSON");
			let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
			keys.sort();
			keys
		}

		/// `goTimingFields`
		fn rust_timing_fields(zero: &str, full: &str) -> (Vec<String>, Vec<String>) {
			let required = keys(zero);
			let optional = keys(full).into_iter().filter(|key| !required.contains(key)).collect();
			(required, optional)
		}

		fn read(name: &str) -> String {
			let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../..")
				.join(name);
			std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
		}

		/// `javascriptTimingFields`: the quoted identifiers of `const <constant> = [...];`.
		fn javascript_timing_fields(source: &str, constant: &str) -> Vec<String> {
			let start = source
				.match_indices("const ")
				.map(|(at, _)| &source[at + "const ".len()..])
				.find_map(|rest| {
					let rest = rest.trim_start().strip_prefix(constant)?;
					rest.trim_start().strip_prefix('=')?.trim_start().strip_prefix('[')
				})
				.unwrap_or_else(|| panic!("JavaScript timing schema has no {constant} array"));
			let body = &start[..start.find("];").expect("array end")];
			let mut fields: Vec<String> = body
				.split('"')
				.skip(1)
				.step_by(2)
				.filter(|field| {
					field.chars().next().is_some_and(|first| first.is_ascii_alphabetic())
						&& field.chars().all(|char| char.is_ascii_alphanumeric())
				})
				.map(str::to_owned)
				.collect();
			fields.sort();
			fields
		}

		/// `typescriptTimingFields`: `readonly name(?): number( | undefined);` inside
		/// `export interface <name> { ... }`.
		fn typescript_timing_fields(source: &str, interface: &str) -> (Vec<String>, Vec<String>) {
			let start = source
				.match_indices("export interface ")
				.map(|(at, _)| &source[at + "export interface ".len()..])
				.find_map(|rest| rest.strip_prefix(interface)?.trim_start().strip_prefix('{'))
				.unwrap_or_else(|| panic!("TypeScript timing schema has no {interface} interface"));
			let body = &start[..start.find('}').expect("interface end")];
			let (mut required, mut optional) = (Vec::new(), Vec::new());
			for declaration in body.split(';') {
				let Some(field) = declaration.trim().strip_prefix("readonly") else {
					continue;
				};
				let Some((name, kind)) = field.split_once(':') else {
					continue;
				};
				let kind = kind.trim();
				if kind != "number" && kind.replace(' ', "") != "number|undefined" {
					continue;
				}
				let name = name.trim();
				match name.strip_suffix('?') {
					Some(name) => optional.push(name.to_owned()),
					None => required.push(name.to_owned()),
				}
			}
			required.sort();
			optional.sort();
			(required, optional)
		}

		#[test]
		fn project_timing_schema_matches_javascript_and_typescript() {
			let javascript = read("api.js");
			let typescript = read("api.d.ts");

			let full_stages = BuildTimingStages {
				initial_program_ms: 1,
				incremental_selection_ms: 1,
				cleanup_ms: 1,
				include_copy_ms: 1,
				non_compiled_copy_ms: 1,
				sidecar_session_wait_ms: 1,
				sidecar_preparation_ms: 1,
				sidecar_round_trip_ms: 1,
				sidecar_response_decode_ms: 1,
				overlay_program_ms: 1,
				project_context_ms: 1,
				semantic_diagnostics_ms: 1,
				native_transform_render_ms: 1,
				compiled_output_writes_ms: 1,
				declaration_emit_writes_ms: 1,
				declaration_emit_ms: 1,
				incremental_manifest_ms: 1,
				persistence_ms: 1,
			};
			let (stages, optional_stages) =
				rust_timing_fields(&BuildTimingStages::default().to_json(), &full_stages.to_json());
			let (ts_stages, ts_optional_stages) = typescript_timing_fields(&typescript, "BuildTimingStages");
			assert_eq!(
				stages,
				javascript_timing_fields(&javascript, "REQUIRED_TIMING_STAGES"),
				"required stages in Rust and JavaScript"
			);
			assert_eq!(stages, ts_stages, "required stages in Rust and TypeScript");
			assert_eq!(
				optional_stages, ts_optional_stages,
				"optional stages in Rust and TypeScript"
			);

			let full_counts = BuildTimingCounts {
				scheduled_projects: 1,
				selected_projects: 1,
				satisfied_projects: 1,
				transformed_projects: 1,
				emitted_projects: 1,
				total_sources: 1,
				selected_sources: 1,
				emitted_entries: 1,
				scheduled_source_map_writes: 1,
				scheduled_declaration_writes: 1,
				actual_writes: 1,
				hash_skips: 1,
				unique_prepared_directories: 1,
				effective_write_workers: 1,
				sidecar_request_bytes: 1,
				sidecar_response_bytes: 1,
				sidecar_stats: 1,
				sidecar_source_reads: 1,
				sidecar_changed_files: 1,
				sidecar_spawns: 1,
				sidecar_restarts: 1,
				node_wall_ms: 1,
				node_cpu_user_us: 1,
				node_cpu_system_us: 1,
				parse_cache_hits: 1,
				parse_cache_misses: 1,
			};
			let (counts, optional_counts) =
				rust_timing_fields(&BuildTimingCounts::default().to_json(), &full_counts.to_json());
			let (ts_counts, ts_optional_counts) = typescript_timing_fields(&typescript, "BuildTimingCounts");
			assert_eq!(
				counts,
				javascript_timing_fields(&javascript, "REQUIRED_TIMING_COUNTS"),
				"required counts in Rust and JavaScript"
			);
			assert_eq!(counts, ts_counts, "required counts in Rust and TypeScript");
			assert_eq!(
				optional_counts, ts_optional_counts,
				"optional counts in Rust and TypeScript"
			);
		}
	}
}
