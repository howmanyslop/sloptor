//! `sidecar.go`: the Node transformer-plugin worker.
//!
//! A project with transformer plugins sends its sources to a warm Node worker
//! (`tools/sidecar`), one line of JSON per request and response over the worker's stdin and
//! stdout. The worker runs the TypeScript transformers and returns the transformed texts and
//! their trace maps; the build then derives an overlaid program from them. Program construction,
//! the other half of `sidecar.go`, is `rotor_tsrs::project`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use rotor_support::{fsutil, gopath, logservice};
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::tsoptions::ParsedCommandLine;

use crate::afterdeclarations::{
	count_configured_after_declarations, take_after_declarations_scan, warn_unsupported_after_declarations,
};
use crate::compile::{Error, Failure};
use crate::incremental_manifest::normalize_source_file_path;
use crate::overlayfs::rekey_overlays_to_program;
use crate::project::{Cancellation, ProjectOptions, lock, project_source_files};
use crate::source_trace::{DiagnosticTraces, SourceTraceMap};
use crate::timings::{Stage, log_stage, log_stage_named, project_label};
use crate::transformer_host::{
	TRANSFORMER_ORIGINAL_AUTHORED, TransformerCall, TransformerChangedFile, TransformerDiagnostic, TransformerHost,
	TransformerOutputFile, TransformerRebase, TransformerResponse, canonical_transformer_request_paths,
	prepare_transformer_request, prepare_transformer_validation_request,
};

/// `os.Getenv` for the sidecar's settings. Tests override variables per process (Go's
/// `t.Setenv`) without touching the process environment, which Rust 2024 makes `unsafe`.
pub(crate) fn getenv(name: &str) -> Option<String> {
	#[cfg(test)]
	if let Some(value) = lock(test_env()).get(name) {
		return Some(value.clone());
	}
	std::env::var(name).ok()
}

/// The test overrides [`getenv`] and [`sidecar_env`] read.
#[cfg(test)]
pub(crate) fn test_env() -> &'static Mutex<HashMap<String, String>> {
	static ENV: std::sync::OnceLock<Mutex<HashMap<String, String>>> = std::sync::OnceLock::new();
	ENV.get_or_init(Mutex::default)
}

/// `t.Setenv` for [`getenv`] and [`sidecar_env`].
#[cfg(test)]
pub(crate) fn set_test_env(name: &str, value: &str) {
	lock(test_env()).insert(name.to_owned(), value.to_owned());
}

/// `defaultSidecarResponseTimeout`: generous on purpose. rbxtsc awaits the transformer with no
/// timeout; the cap only guards against a hung worker.
const DEFAULT_SIDECAR_RESPONSE_TIMEOUT: Duration = Duration::from_mins(10);

/// `sidecarResponseTimeoutEnv`
pub(crate) const SIDECAR_RESPONSE_TIMEOUT_ENV: &str = "ROTOR_SIDECAR_TIMEOUT";

/// `sidecarCallStats`: what one or more round trips cost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidecarCallStats {
	pub(crate) wait: Duration,
	pub(crate) prep: Duration,
	pub(crate) round_trip: Duration,
	pub(crate) decode: Duration,
	pub(crate) request_bytes: i64,
	pub(crate) response_bytes: i64,
	pub(crate) stats: i64,
	pub(crate) reads: i64,
	pub(crate) changed_files: i64,
	pub(crate) spawned: bool,
	pub(crate) additional_spawns: i64,
	pub(crate) restarted: bool,
	pub(crate) node_wall_ms: i64,
	pub(crate) node_cpu_user_us: i64,
	pub(crate) node_cpu_system_us: i64,
	pub(crate) node_version: String,
}

/// A failed round trip: the error and what the attempt cost.
#[derive(Debug)]
pub(crate) struct SidecarFailure {
	pub(crate) error: Error,
	pub(crate) stats: SidecarCallStats,
}

/// A round trip's response and costs, or its failure.
pub(crate) type SidecarResult = Result<(TransformerResponse, SidecarCallStats), Box<SidecarFailure>>;

/// Boxes a [`SidecarFailure`].
pub(crate) fn sidecar_failure(error: Error, stats: SidecarCallStats) -> Box<SidecarFailure> {
	Box::new(SidecarFailure { error, stats })
}

/// `preparedTransformerProgram`: the program the transform runs on, its source files, and the
/// traces back to the original text.
#[derive(Clone)]
pub struct PreparedTransformerProgram {
	/// The program to transform: the original, or one overlaid with the transformed texts.
	pub program: &'static Program,
	/// Native jecs's rewrites (Stage 6), keyed by normalized path.
	pub rebases: HashMap<String, Arc<TransformerRebase>>,
	/// The selected files, as files of `program`.
	pub source_files: Vec<P<SourceFile>>,
	/// The active Flamework configuration, when native Flamework ran.
	pub flamework: Option<rotor_config::config::FlameworkConfig>,
	/// Traces from reprinted files back to the text on disk.
	pub source_traces: DiagnosticTraces,
	/// The sidecar's costs.
	pub(crate) stats: SidecarCallStats,
	pub(crate) overlay_program_duration: Duration,
	pub(crate) sidecar_round_trip_recorded: bool,
	pub(crate) overlay_program_recorded: bool,
}

impl PreparedTransformerProgram {
	/// The untransformed program.
	pub(crate) fn plain(
		program: &'static Program,
		source_files: Vec<P<SourceFile>>,
		flamework: Option<rotor_config::config::FlameworkConfig>,
	) -> Self {
		Self {
			program,
			rebases: HashMap::new(),
			source_files,
			flamework,
			source_traces: DiagnosticTraces::default(),
			stats: SidecarCallStats::default(),
			overlay_program_duration: Duration::ZERO,
			sidecar_round_trip_recorded: false,
			overlay_program_recorded: false,
		}
	}
}

/// The transformer host of a build: the API callback when one is set, else the Node sidecar.
pub(crate) fn transformer_host_for(opts: &ProjectOptions) -> &dyn TransformerHost {
	match &opts.transformer_callback {
		Some(host) => &**host,
		None => &crate::transformer_host_sidecar::SidecarTransformerHost,
	}
}

/// `prepareTransformerProgramWithHost`: runs the project's transformer plugins, if any, over
/// `source_files`.
pub(crate) fn prepare_transformer_program_with_host(
	cancel: Option<&Cancellation>,
	host: &dyn TransformerHost,
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
) -> Result<PreparedTransformerProgram, Failure> {
	let flamework = crate::flamework_config::prepare_flamework_config(dir, &program.command_line())?;
	if source_files.is_empty() || !project_uses_transformer_plugins(Some(&program.command_line())) {
		return Ok(PreparedTransformerProgram::plain(
			program,
			source_files.to_vec(),
			flamework,
		));
	}
	let mut transformed = apply_transformer_sidecar_with_plugins_and_host(
		cancel,
		host,
		dir,
		program,
		source_files,
		overlays,
		None,
		None,
		TRANSFORMER_ORIGINAL_AUTHORED,
		None,
	)?;
	if std::ptr::eq(transformed.program, program) {
		transformed.source_files = source_files.to_vec();
	} else {
		transformed.source_files =
			remap_program_source_files(transformed.program, source_files).map_err(Failure::bare)?;
	}
	transformed.flamework = flamework;
	Ok(transformed)
}

/// `applyTransformerSidecarWithPluginsAndHost`: one transformer round trip, then the program
/// overlaid with the transformed texts.
#[expect(clippy::too_many_arguments, reason = "Go's signature")]
#[expect(clippy::too_many_lines, reason = "one function in Go")]
pub(crate) fn apply_transformer_sidecar_with_plugins_and_host(
	cancel: Option<&Cancellation>,
	host: &dyn TransformerHost,
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
	plugins: Option<&[String]>,
	state: Option<&Arc<Mutex<SidecarBuildState>>>,
	source_origin: &str,
	parallel: Option<&crate::sidecar_parallel::ParallelPrefix>,
) -> Result<PreparedTransformerProgram, Failure> {
	let mut config_path = program.options().config_file_path.clone();
	if config_path.is_empty() {
		config_path = gopath::to_slash(&gopath::join(&[&gopath::from_slash(dir), "tsconfig.json"])).into_owned();
	}

	let emits_declarations = program.options().get_emit_declarations();
	if emits_declarations && take_after_declarations_scan(&config_path) {
		let configured = plugins.map(<[String]>::to_vec).or_else(|| {
			crate::transformer_order::effective_transformer_plugins(&config_path)
				.ok()
				.map(|plugins| {
					plugins
						.iter()
						.map(crate::transformer_order::TransformerPluginConfig::marshal_json)
						.collect()
				})
		});
		warn_unsupported_after_declarations(
			&config_path,
			count_configured_after_declarations(configured.as_deref().unwrap_or_default()),
		);
	}
	let mut call = TransformerCall {
		dir: dir.to_owned(),
		config_path: config_path.clone(),
		compile_files: source_files.to_vec(),
		stamp_files: project_source_files(program),
		overlays: overlays.clone(),
		plugins: plugins.map(<[String]>::to_vec),
		state: state.cloned(),
		source_origin: source_origin.to_owned(),
		..TransformerCall::default()
	};
	// Go's `sidecarTransformerHost.transform` takes the split from the context; a callback host
	// never gets one.
	if let Some(parallel) = parallel
		&& host.uses_subprocess_sidecar()
	{
		call.parallel = Some(parallel.attestation.clone());
		call.parallel_build = Some(Arc::clone(&parallel.build));
	}
	let (response, mut costs) = host
		.transform(cancel, call)
		.map_err(|error| Failure::strings(vec![error.to_string()], error))?;

	if emits_declarations {
		warn_unsupported_after_declarations(
			&config_path,
			usize::try_from(response.after_declarations_transformers).unwrap_or(0),
		);
	}
	let mut error_diags = Vec::new();
	for diagnostic in &response.diagnostics {
		let text = format_sidecar_diagnostic(diagnostic);
		if diagnostic.category.eq_ignore_ascii_case("warning") {
			logservice::warn(&text);
			continue;
		}
		error_diags.push(text);
	}
	if !error_diags.is_empty() {
		return Err(Failure::strings(
			error_diags,
			Error::message("compile: transformer sidecar diagnostics"),
		));
	}
	let stop_decode = log_stage(&config_path, Stage::SidecarResponseDecode);
	let mut source_traces = DiagnosticTraces::default();
	let mut canonical_sources: Option<HashMap<String, P<SourceFile>>> = None;
	let mut transformed_files = response.transformed;
	let rebases: HashMap<String, Arc<TransformerRebase>> = state
		.as_ref()
		.map(|state| lock(state).rebases.clone())
		.unwrap_or_default();
	for file in &mut transformed_files {
		if file.trace_map.is_empty() {
			continue;
		}
		let mut original = program.get_source_file(&file.file_name);
		if original.is_none() {
			let sources = canonical_sources.get_or_insert_with(|| {
				project_source_files(program)
					.into_iter()
					.map(|candidate| (canonical_sidecar_path(candidate.file_name()), candidate))
					.collect()
			});
			original = sources.get(&canonical_sidecar_path(&file.file_name)).copied();
		}
		let Some(original) = original else {
			stop_decode();
			return Err(Failure::bare(Error::message(format!(
				"compile: transformer trace source missing from program: {}",
				file.file_name
			))));
		};
		// A rebased file's trace indexes the text native jecs started from.
		let original_text = rebases
			.get(&normalize_source_file_path(original.file_name()))
			.map_or_else(|| original.text().to_owned(), |rebase| rebase.text.clone());
		let trace = match SourceTraceMap::new(&file.trace_map, original.file_name(), &original_text) {
			Ok(trace) => trace,
			Err(error) => {
				stop_decode();
				return Err(Failure::bare(Error::message(error)));
			}
		};
		// Keep the program's spelling for overlay and diagnostic lookups. The worker works on
		// physical paths, which can differ through a symlink.
		original.file_name().clone_into(&mut file.file_name);
		source_traces
			.0
			.insert(normalize_source_file_path(&file.file_name), Arc::new(trace));
	}
	costs.decode += stop_decode();
	if let Some(state) = &state {
		let mut state = lock(state);
		state.add_call(&costs);
		state.absorb_transformed(&transformed_files);
	}
	let mut prepared = prepared_from_sidecar_stats(program, source_traces, costs);
	if transformed_files.is_empty() {
		return Ok(prepared);
	}

	// The worker returns text only for the files it compiled, a subset on an incremental route.
	// Transformed text wins where it exists; the caller's overlay stands everywhere else, under
	// the exact file names this program holds.
	let (mut program_overlays, _) = rekey_overlays_to_program(program, overlays);
	for file in transformed_files {
		program_overlays.insert(file.file_name, file.text);
	}
	let stop_overlay = log_stage(&config_path, Stage::OverlayProgram);
	let updated = crate::flamework_program_update::update_program_with_text_overlays(program, &program_overlays);
	let overlay_duration = stop_overlay();
	let (transformed_program, _) = updated.map_err(Failure::bare)?;
	prepared.program = transformed_program;
	prepared.overlay_program_duration = overlay_duration;
	prepared.overlay_program_recorded = true;
	Ok(prepared)
}

/// `preparedFromSidecarStats`
fn prepared_from_sidecar_stats(
	program: &'static Program,
	source_traces: DiagnosticTraces,
	stats: SidecarCallStats,
) -> PreparedTransformerProgram {
	PreparedTransformerProgram {
		source_traces,
		stats,
		sidecar_round_trip_recorded: true,
		..PreparedTransformerProgram::plain(program, Vec::new(), None)
	}
}

/// `sidecarFileStamp`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SidecarFileStamp {
	mod_time: Option<SystemTime>,
	size: u64,
}

impl SidecarFileStamp {
	fn of(metadata: &std::fs::Metadata) -> Self {
		Self {
			mod_time: metadata.modified().ok(),
			size: metadata.len(),
		}
	}
}

/// The process half of a session: the Node worker and its pipes.
pub(crate) struct SidecarProcess {
	child: Mutex<Child>,
	/// Locked only while a request is written, so `close` can end the worker's input while a
	/// read waits on its output.
	stdin: Arc<Mutex<Option<ChildStdin>>>,
	stdout: Arc<Mutex<BufReader<ChildStdout>>>,
	pub(crate) stderr: Arc<SidecarStderrTail>,
}

/// Writes one request line to the worker, holding its stdin only for the write.
fn write_request(stdin: &Mutex<Option<ChildStdin>>, line: &[u8]) -> std::io::Result<()> {
	let mut guard = lock(stdin);
	let result = guard.as_mut().map_or_else(
		|| Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
		|stdin| stdin.write_all(line).and_then(|()| stdin.flush()),
	);
	drop(guard);
	result
}

/// `sidecarSession`: one warm Node worker.
///
/// It lives for the process (the worker exits when its stdin closes), so watch rebuilds reuse the
/// JavaScript program. A tracker without a process keeps the stamps of a callback host.
#[derive(Default)]
pub struct SidecarSession {
	pub(crate) process: Option<SidecarProcess>,
	pub(crate) stamps: HashMap<String, SidecarFileStamp>,
	/// The files whose worker text came from a caller overlay, keyed the way
	/// `changed_files_for` looks an overlay up, valued with the path a request carries. The
	/// worker's overrides outlive the round trip that set them, so a dropped overlay is undone by
	/// hand.
	pub(crate) overlaid: BTreeMap<String, String>,
	pub(crate) dead: bool,
	/// The worker's stderr, kept past `close` as Go's session keeps its tail, so a closed or
	/// failed worker's output still reaches the log.
	pub(crate) stderr: Option<Arc<SidecarStderrTail>>,
}

/// `sidecarSlot`: the session of one project (or shard), used by one request at a time.
#[derive(Default)]
pub(crate) struct SidecarSlot {
	state: Mutex<SlotState>,
	released: Condvar,
}

#[derive(Default)]
struct SlotState {
	busy: bool,
	session: Option<SidecarSession>,
	used: bool,
}

/// A held slot: the slot's session and flag, moved out until the guard drops. The guard is
/// `Send`, so a warm-up thread can release a slot its caller took.
pub(crate) struct SlotGuard {
	slot: Arc<SidecarSlot>,
	pub(crate) session: Option<SidecarSession>,
	pub(crate) used: bool,
}

impl SidecarSlot {
	/// Takes the slot, waiting for its current holder.
	pub(crate) fn acquire(self: &Arc<Self>) -> SlotGuard {
		let mut state = lock(&self.state);
		while state.busy {
			state = self
				.released
				.wait(state)
				.unwrap_or_else(std::sync::PoisonError::into_inner);
		}
		state.busy = true;
		SlotGuard {
			slot: Arc::clone(self),
			session: state.session.take(),
			used: state.used,
		}
	}
}

impl Drop for SlotGuard {
	fn drop(&mut self) {
		let mut state = lock(&self.slot.state);
		state.session = self.session.take();
		state.used = self.used;
		state.busy = false;
		drop(state);
		self.slot.released.notify_all();
	}
}

/// `sidecarBuildState`: what the stages of one Flamework build share.
#[derive(Clone, Default)]
pub struct SidecarBuildState {
	pub(crate) stage_overlays: BTreeMap<String, String>,
	pub(crate) disk_scanned: bool,
	pub(crate) call: SidecarCallStats,
	/// Native jecs's rewrites, keyed by normalized path.
	pub(crate) rebases: HashMap<String, Arc<TransformerRebase>>,
}

impl SidecarBuildState {
	/// `absorbSourceFiles`
	pub(crate) fn absorb_source_files(&mut self, files: &[P<SourceFile>]) {
		for file in files {
			self.stage_overlays
				.insert(file.file_name().to_owned(), file.text().to_owned());
		}
	}

	/// `absorbTransformed`
	pub(crate) fn absorb_transformed(&mut self, files: &[TransformerOutputFile]) {
		for file in files {
			self.stage_overlays.insert(file.file_name.clone(), file.text.clone());
		}
	}

	/// `addCall`
	pub(crate) fn add_call(&mut self, stats: &SidecarCallStats) {
		let call = &mut self.call;
		call.wait += stats.wait;
		call.prep += stats.prep;
		call.round_trip += stats.round_trip;
		call.decode += stats.decode;
		call.request_bytes += stats.request_bytes;
		call.response_bytes += stats.response_bytes;
		call.stats += stats.stats;
		call.reads += stats.reads;
		call.changed_files += stats.changed_files;
		call.spawned |= stats.spawned;
		call.additional_spawns += stats.additional_spawns;
		call.restarted |= stats.restarted;
		call.node_wall_ms += stats.node_wall_ms;
		call.node_cpu_user_us += stats.node_cpu_user_us;
		call.node_cpu_system_us += stats.node_cpu_system_us;
		if !stats.node_version.is_empty() {
			call.node_version.clone_from(&stats.node_version);
		}
	}

	/// `applyTo`
	pub(crate) fn apply_to(&self, prepared: &mut PreparedTransformerProgram) {
		let node_version = std::mem::take(&mut prepared.stats.node_version);
		prepared.stats = self.call.clone();
		if self.call.node_version.is_empty() {
			prepared.stats.node_version = node_version;
		}
		if !self.call.round_trip.is_zero() || !self.call.prep.is_zero() || !self.call.wait.is_zero() {
			prepared.sidecar_round_trip_recorded = true;
		}
	}
}

fn slots() -> &'static Mutex<HashMap<String, Arc<SidecarSlot>>> {
	static SLOTS: OnceLock<Mutex<HashMap<String, Arc<SidecarSlot>>>> = OnceLock::new();
	SLOTS.get_or_init(Mutex::default)
}

/// `sidecarSlotFor`
pub(crate) fn sidecar_slot_for(key: &str) -> Arc<SidecarSlot> {
	Arc::clone(lock(slots()).entry(key.to_owned()).or_default())
}

/// `sidecarSlotCount`
#[cfg(test)]
pub(crate) fn sidecar_slot_count() -> usize {
	lock(slots()).len()
}

/// `sidecarStderrTail`: the worker's stderr (plugin console output) for two readers: `drain_to`
/// forwards new lines to the compiler log on the calling thread, and the tail backs error
/// messages.
#[derive(Default)]
pub(crate) struct SidecarStderrTail {
	state: Mutex<StderrState>,
	done_signal: Condvar,
}

#[derive(Default)]
struct StderrState {
	tail: Vec<String>,
	pending: Vec<String>,
	done: bool,
}

/// `sidecarStderrLineLimit`: the bytes kept per stderr line; the rest is read and dropped.
pub(crate) const SIDECAR_STDERR_LINE_LIMIT: usize = 64 * 1024;
/// `sidecarStderrTruncated`
pub(crate) const SIDECAR_STDERR_TRUNCATED: &str = " [truncated]";

impl SidecarStderrTail {
	/// The stored lines and how many are pending, for tests.
	#[cfg(test)]
	pub(crate) fn snapshot(&self) -> (Vec<String>, usize) {
		let state = lock(&self.state);
		(state.tail.clone(), state.pending.len())
	}

	/// `newSidecarStderrTail`: reads `pipe` to its end on a thread of its own. A reader that
	/// stops early would leave the pipe full and block the worker on its next write.
	pub(crate) fn spawn(pipe: impl Read + Send + 'static) -> Arc<Self> {
		let tail = Arc::new(Self::default());
		let reader_tail = Arc::clone(&tail);
		let started = std::thread::Builder::new()
			.name("rotor-sidecar-stderr".to_owned())
			.spawn(move || {
				let mut reader = BufReader::new(pipe);
				while let Some(line) = read_sidecar_stderr_line(&mut reader) {
					let mut state = lock(&reader_tail.state);
					state.tail.push(line.clone());
					if state.tail.len() > 50 {
						let excess = state.tail.len() - 50;
						state.tail.drain(..excess);
					}
					state.pending.push(line);
				}
				lock(&reader_tail.state).done = true;
				reader_tail.done_signal.notify_all();
			});
		if started.is_err() {
			lock(&tail.state).done = true;
		}
		tail
	}

	/// `drainTo`: writes the lines buffered since the last drain to the compiler log.
	pub(crate) fn drain_to(&self) {
		let pending = std::mem::take(&mut lock(&self.state).pending);
		for line in pending {
			logservice::write_line(&[&line]);
		}
	}

	/// Waits up to `timeout` for the reader to reach the end of the pipe.
	#[expect(
		clippy::significant_drop_tightening,
		reason = "the condition variable needs the guard until the reader is done"
	)]
	pub(crate) fn wait_done(&self, timeout: Option<Duration>) -> bool {
		let deadline = timeout.map(|timeout| Instant::now() + timeout);
		let mut state = lock(&self.state);
		while !state.done {
			match deadline {
				None => {
					state = self
						.done_signal
						.wait(state)
						.unwrap_or_else(std::sync::PoisonError::into_inner);
				}
				Some(deadline) => {
					let now = Instant::now();
					if now >= deadline {
						return false;
					}
					state = self
						.done_signal
						.wait_timeout(state, deadline - now)
						.unwrap_or_else(std::sync::PoisonError::into_inner)
						.0;
				}
			}
		}
		true
	}
}

impl std::fmt::Display for SidecarStderrTail {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.write_str(&lock(&self.state).tail.join("\n"))
	}
}

/// `readSidecarStderrLine`: one whole line, at most [`SIDECAR_STDERR_LINE_LIMIT`] bytes of it;
/// `None` once no bytes remain.
pub(crate) fn read_sidecar_stderr_line(reader: &mut impl BufRead) -> Option<String> {
	let mut kept: Vec<u8> = Vec::new();
	let mut read = false;
	let mut truncated = false;
	while let Ok(buffer) = reader.fill_buf() {
		if buffer.is_empty() {
			break;
		}
		read = true;
		let (chunk, consumed, ended) = buffer
			.iter()
			.position(|&byte| byte == b'\n')
			.map_or((buffer, buffer.len(), false), |at| (&buffer[..at], at + 1, true));
		let room = SIDECAR_STDERR_LINE_LIMIT - kept.len();
		if chunk.len() > room {
			kept.extend_from_slice(&chunk[..room]);
			truncated = true;
		} else {
			kept.extend_from_slice(chunk);
		}
		reader.consume(consumed);
		if ended {
			break;
		}
	}
	if !read {
		return None;
	}
	let mut line = String::from_utf8_lossy(&kept).into_owned();
	if line.ends_with('\r') {
		line.pop();
	}
	if truncated {
		line.push_str(SIDECAR_STDERR_TRUNCATED);
	}
	Some(line)
}

/// `spawnSidecarSession`
pub(crate) fn spawn_sidecar_session(dir: &str, sidecar_dir: &str) -> Result<SidecarSession, Error> {
	spawn_sidecar_session_mode(dir, sidecar_dir, false)
}

const NODE_NOT_FOUND: &str = "node executable not found; rotor transformer plugins require Node.js on PATH";

/// `spawnSidecarSessionMode`: starts the worker in `dir`. Go's owned-stderr mode only changes
/// which side closes the pipe; Rust's pipe is always the reader's own.
pub(crate) fn spawn_sidecar_session_mode(
	dir: &str,
	sidecar_dir: &str,
	_owned_stderr: bool,
) -> Result<SidecarSession, Error> {
	let node_command = getenv("ROTOR_NODE_PATH").unwrap_or_default();
	let node_path = if node_command.is_empty() {
		look_path("node").ok_or_else(|| Error::message(NODE_NOT_FOUND))?
	} else {
		if std::fs::metadata(&node_command).is_err() {
			return Err(Error::message(NODE_NOT_FOUND));
		}
		look_path(&node_command).ok_or_else(|| Error::message(NODE_NOT_FOUND))?
	};
	let mut command = Command::new(node_path);
	command
		.arg(gopath::join(&[sidecar_dir, "main.js"]))
		.current_dir(gopath::from_slash(dir).as_ref())
		.env_clear()
		.envs(sidecar_env(dir, sidecar_dir))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child = command
		.spawn()
		.map_err(|error| Error::message(rotor_support::gostr::io_error_text(&error)))?;
	let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
	else {
		let _ = child.kill();
		let _ = child.wait();
		return Err(Error::message("compile: transformer sidecar pipes unavailable"));
	};
	let stderr = SidecarStderrTail::spawn(stderr);
	Ok(SidecarSession {
		process: Some(SidecarProcess {
			child: Mutex::new(child),
			stdin: Arc::new(Mutex::new(Some(stdin))),
			stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
			stderr: Arc::clone(&stderr),
		}),
		stderr: Some(stderr),
		..SidecarSession::default()
	})
}

/// `exec.LookPath`: a name with a separator is checked as is, a bare name searches `PATH`.
fn look_path(name: &str) -> Option<String> {
	let executable = |path: &std::path::Path| {
		std::fs::metadata(path).is_ok_and(|metadata| {
			#[cfg(unix)]
			{
				use std::os::unix::fs::PermissionsExt as _;
				metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
			}
			#[cfg(not(unix))]
			{
				metadata.is_file()
			}
		})
	};
	if name.contains('/') || cfg!(windows) && name.contains('\\') {
		return executable(std::path::Path::new(name)).then(|| name.to_owned());
	}
	let path = std::env::var_os("PATH")?;
	let extensions: Vec<String> = if cfg!(windows) {
		std::env::var("PATHEXT")
			.unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
			.split(';')
			.map(str::to_lowercase)
			.collect()
	} else {
		vec![String::new()]
	};
	for dir in std::env::split_paths(&path) {
		let dir = if dir.as_os_str().is_empty() {
			std::path::PathBuf::from(".")
		} else {
			dir
		};
		for extension in &extensions {
			let candidate = dir.join(format!("{name}{extension}"));
			if executable(&candidate) {
				return Some(candidate.to_string_lossy().into_owned());
			}
		}
	}
	None
}

/// Why a round trip failed: Go's error chain, which tells a timeout from a cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoundTripFailure {
	/// The worker's pipes failed.
	Transport,
	/// `context.DeadlineExceeded`
	DeadlineExceeded,
	/// `context.Canceled`
	Canceled,
}

impl SidecarSession {
	/// `writeAndRead`: one request line out, one response line back, bounded by `deadline` and
	/// `cancel`.
	pub(crate) fn write_and_read(
		&mut self,
		cancel: Option<&Cancellation>,
		deadline: Instant,
		payload: &[u8],
	) -> Result<Vec<u8>, (Error, RoundTripFailure)> {
		self.write_and_read_any(&[cancel], deadline, payload)
	}

	/// [`SidecarSession::write_and_read`], cancelled by any of `cancels`.
	pub(crate) fn write_and_read_any(
		&mut self,
		cancels: &[Option<&Cancellation>],
		deadline: Instant,
		payload: &[u8],
	) -> Result<Vec<u8>, (Error, RoundTripFailure)> {
		let Some(process) = &self.process else {
			return Err((
				Error::message("compile: transformer sidecar has no process"),
				RoundTripFailure::Transport,
			));
		};
		let stdin = Arc::clone(&process.stdin);
		let stdout = Arc::clone(&process.stdout);
		let mut line = payload.to_vec();
		line.push(b'\n');
		let (sender, receiver) = mpsc::channel();
		let started = std::thread::Builder::new()
			.name("rotor-sidecar-io".to_owned())
			.spawn(move || {
				let mut stdout = lock(&stdout);
				let result = (|| {
					write_request(&stdin, &line)?;
					let mut response = Vec::new();
					let read = stdout.read_until(b'\n', &mut response)?;
					if read == 0 || !response.ends_with(b"\n") {
						return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
					}
					Ok(response)
				})();
				let _ = sender.send(result);
			});
		if let Err(error) = started {
			let error = Error::message(rotor_support::gostr::io_error_text(&error));
			return Err((self.fail(&error), RoundTripFailure::Transport));
		}
		loop {
			let now = Instant::now();
			if cancels
				.iter()
				.any(|cancel| cancel.is_some_and(Cancellation::is_cancelled))
			{
				let error = Error::message("transformer sidecar response timed out: context canceled");
				return Err((self.fail(&error), RoundTripFailure::Canceled));
			}
			if now >= deadline {
				let error = Error::message("transformer sidecar response timed out: context deadline exceeded");
				return Err((self.fail(&error), RoundTripFailure::DeadlineExceeded));
			}
			let wait = (deadline - now).min(Duration::from_millis(50));
			match receiver.recv_timeout(wait) {
				Ok(Ok(response)) => return Ok(response),
				Ok(Err(error)) => {
					let error = Error::message(go_io_error(&error));
					return Err((self.fail(&error), RoundTripFailure::Transport));
				}
				Err(mpsc::RecvTimeoutError::Timeout) => {}
				Err(mpsc::RecvTimeoutError::Disconnected) => {
					let error = Error::message("EOF");
					return Err((self.fail(&error), RoundTripFailure::Transport));
				}
			}
		}
	}

	/// `fail`: marks the session dead and attaches the stderr tail.
	pub(crate) fn fail(&mut self, error: &Error) -> Error {
		self.dead = true;
		let tail = self
			.process
			.as_ref()
			.map(|process| process.stderr.to_string())
			.unwrap_or_default();
		if tail.is_empty() {
			return error.clone();
		}
		Error::message(format!("transformer sidecar failed: {error}: {}", tail.trim()))
	}

	/// Forwards the worker's buffered stderr to the compiler log.
	pub(crate) fn drain_stderr(&self) {
		if let Some(stderr) = &self.stderr {
			stderr.drain_to();
		}
	}

	/// `close`
	pub(crate) fn close(&mut self) {
		let _ = self.close_with_expected_kill(false);
	}

	/// `closeAndWait`
	pub(crate) fn close_and_wait(&mut self) -> Result<(), Error> {
		self.close_with_expected_kill(false)
	}

	/// `closeWithExpectedKill`: closes stdin, waits two seconds for the worker, then kills it.
	/// Only a timed-out auxiliary preflight treats its own kill as expected.
	pub(crate) fn close_with_expected_kill(&mut self, expected_kill: bool) -> Result<(), Error> {
		let Some(process) = self.process.take() else {
			return Ok(());
		};
		// Go closes the worker's stdin first. A write still blocked on a full pipe holds the lock;
		// the kill below ends that worker instead.
		let deadline = Instant::now() + Duration::from_millis(100);
		loop {
			if let Ok(mut stdin) = process.stdin.try_lock() {
				*stdin = None;
				break;
			}
			if Instant::now() >= deadline {
				break;
			}
			std::thread::sleep(Duration::from_millis(1));
		}
		let mut errors: Vec<String> = Vec::new();
		{
			let mut child = lock(&process.child);
			let deadline = Instant::now() + Duration::from_secs(2);
			let status = loop {
				match child.try_wait() {
					Ok(Some(status)) => break Some(status),
					Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
					Ok(None) => break None,
					Err(error) => {
						errors.push(rotor_support::gostr::io_error_text(&error));
						break None;
					}
				}
			};
			match status {
				Some(status) if !status.success() => errors.push(exit_status_text(status)),
				None if errors.is_empty() => {
					let _ = child.kill();
					if let Ok(status) = child.wait()
						&& !expected_kill && !status.success()
					{
						errors.push(exit_status_text(status));
					}
				}
				Some(_) | None => {}
			}
		}
		if !process.stderr.wait_done(Some(Duration::from_secs(2))) {
			errors.push("exec: WaitDelay expired before I/O complete".to_owned());
		}
		if errors.is_empty() {
			Ok(())
		} else {
			Err(Error::message(errors.join("\n")))
		}
	}

	/// `changedFilesFor`
	#[cfg(test)]
	pub(crate) fn changed_files_for(
		&mut self,
		file_names: &[String],
		overlays: &BTreeMap<String, String>,
	) -> Result<Vec<TransformerChangedFile>, Error> {
		self.collect_changed_files(file_names, overlays, false).0
	}

	/// `collectChangedFiles`: stat-diffs the project's files against the session's stamps. A
	/// fresh session only records stamps (the worker reads disk); a warm one ships new text.
	/// Overlaid files exist nowhere on disk, so each ships on every round trip and the stat-diff
	/// skips it.
	pub(crate) fn collect_changed_files(
		&mut self,
		file_names: &[String],
		overlays: &BTreeMap<String, String>,
		skip_disk_scan: bool,
	) -> (Result<Vec<TransformerChangedFile>, Error>, SidecarCallStats) {
		let mut io_stats = SidecarCallStats::default();
		// An empty stamp map still means no round trip yet. Reverting a dropped overlay stamps the
		// file it hands back, so a second round trip always has stamps.
		let fresh = self.stamps.is_empty();
		let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
		let mut overlaid: BTreeMap<String, TransformerChangedFile> = BTreeMap::new();
		for (path, text) in overlays {
			overlaid.insert(
				normalize_overlay_path(path, case_sensitive),
				TransformerChangedFile {
					file_name: gopath::from_slash(path).into_owned(),
					text: text.clone(),
					rebase: None,
				},
			);
		}
		let (mut changed, revert_reads, revert_stats) = self.revert_dropped_overlays(&overlaid);
		io_stats.reads += revert_reads;
		io_stats.stats += revert_stats;
		for (key, file) in &overlaid {
			changed.push(file.clone());
			self.overlaid.insert(key.clone(), file.file_name.clone());
		}
		if skip_disk_scan {
			io_stats.changed_files = count(changed.len());
			return (Ok(changed), io_stats);
		}
		// A build without overlays skips keying every file for a lookup that cannot hit.
		let overlay_aware = !overlaid.is_empty();
		for file_name in file_names {
			let path = gopath::from_slash(file_name).into_owned();
			if overlay_aware {
				let key = normalize_overlay_path(&path, case_sensitive);
				if overlaid.contains_key(&key) {
					// The program's spelling, so the stamp a revert writes is the one this loop
					// later looks up.
					self.overlaid.insert(key, path);
					continue;
				}
			}
			io_stats.stats += 1;
			let Ok(metadata) = std::fs::metadata(&path) else {
				continue;
			};
			let stamp = SidecarFileStamp::of(&metadata);
			if !fresh && self.stamps.get(&path) != Some(&stamp) {
				io_stats.reads += 1;
				match std::fs::read(&path) {
					Ok(text) => changed.push(TransformerChangedFile {
						file_name: path.clone(),
						text: String::from_utf8_lossy(&text).into_owned(),
						rebase: None,
					}),
					Err(error) => {
						return (
							Err(Error::message(rotor_support::gostr::path_error("open", &path, &error))),
							io_stats,
						);
					}
				}
			}
			self.stamps.insert(path, stamp);
		}
		io_stats.changed_files = count(changed.len());
		(Ok(changed), io_stats)
	}

	/// `revertDroppedOverlays`: resends the disk text of every overlay this round trip no longer
	/// carries, stamping it so the stat-diff does not send it again. A file overlaid and then
	/// deleted keeps its override in the worker: there is nothing to resend.
	fn revert_dropped_overlays(
		&mut self,
		overlaid: &BTreeMap<String, TransformerChangedFile>,
	) -> (Vec<TransformerChangedFile>, i64, i64) {
		let mut changed = Vec::new();
		let mut reads = 0;
		let mut stats = 0;
		let dropped: Vec<String> = self
			.overlaid
			.keys()
			.filter(|key| !overlaid.contains_key(*key))
			.cloned()
			.collect();
		for key in dropped {
			let Some(path) = self.overlaid.remove(&key) else {
				continue;
			};
			let text = std::fs::read(&path);
			reads += 1;
			let metadata = std::fs::metadata(&path);
			stats += 1;
			let (Ok(text), Ok(metadata)) = (text, metadata) else {
				self.stamps.remove(&path);
				continue;
			};
			changed.push(TransformerChangedFile {
				file_name: path.clone(),
				text: String::from_utf8_lossy(&text).into_owned(),
				rebase: None,
			});
			self.stamps.insert(path, SidecarFileStamp::of(&metadata));
		}
		(changed, reads, stats)
	}
}

fn count(length: usize) -> i64 {
	i64::try_from(length).unwrap_or(i64::MAX)
}

/// Go's text for a pipe error.
fn go_io_error(error: &std::io::Error) -> String {
	match error.kind() {
		std::io::ErrorKind::UnexpectedEof => "EOF".to_owned(),
		std::io::ErrorKind::BrokenPipe => "write |1: broken pipe".to_owned(),
		_ => rotor_support::gostr::io_error_text(error),
	}
}

/// Go's `*exec.ExitError` text.
fn exit_status_text(status: std::process::ExitStatus) -> String {
	#[cfg(unix)]
	{
		use std::os::unix::process::ExitStatusExt as _;
		if let Some(signal) = status.signal() {
			// `ProcessState.String`.
			let core = if status.core_dumped() { " (core dumped)" } else { "" };
			return format!("signal: {}{core}", signal_name(signal));
		}
	}
	status.code().map_or_else(
		|| "exit status unknown".to_owned(),
		|code| format!("exit status {code}"),
	)
}

/// Go's `syscall` signal names (`zerrors_darwin_*.go`).
#[cfg(target_os = "macos")]
const SIGNAL_NAMES: [&str; 32] = [
	"",
	"hangup",
	"interrupt",
	"quit",
	"illegal instruction",
	"trace/BPT trap",
	"abort trap",
	"EMT trap",
	"floating point exception",
	"killed",
	"bus error",
	"segmentation fault",
	"bad system call",
	"broken pipe",
	"alarm clock",
	"terminated",
	"urgent I/O condition",
	"suspended (signal)",
	"suspended",
	"continued",
	"child exited",
	"stopped (tty input)",
	"stopped (tty output)",
	"I/O possible",
	"cputime limit exceeded",
	"filesize limit exceeded",
	"virtual timer expired",
	"profiling timer expired",
	"window size changes",
	"information request",
	"user defined signal 1",
	"user defined signal 2",
];

/// Go's `syscall` signal names (`zerrors_linux_*.go`).
#[cfg(all(unix, not(target_os = "macos")))]
const SIGNAL_NAMES: [&str; 32] = [
	"",
	"hangup",
	"interrupt",
	"quit",
	"illegal instruction",
	"trace/breakpoint trap",
	"aborted",
	"bus error",
	"floating point exception",
	"killed",
	"user defined signal 1",
	"segmentation fault",
	"user defined signal 2",
	"broken pipe",
	"alarm clock",
	"terminated",
	"stack fault",
	"child exited",
	"continued",
	"stopped (signal)",
	"stopped",
	"stopped (tty input)",
	"stopped (tty output)",
	"urgent I/O condition",
	"CPU time limit exceeded",
	"file size limit exceeded",
	"virtual timer expired",
	"profiling timer expired",
	"window changed",
	"I/O possible",
	"power failure",
	"bad system call",
];

/// `syscall.Signal.String`.
#[cfg(unix)]
pub(crate) fn signal_name(signal: i32) -> String {
	usize::try_from(signal)
		.ok()
		.and_then(|index| SIGNAL_NAMES.get(index))
		.filter(|name| !name.is_empty())
		.map_or_else(|| format!("signal {signal}"), |name| (*name).to_owned())
}

/// `sidecarResponseTimeout`: `ROTOR_SIDECAR_TIMEOUT` (a Go duration), else ten minutes.
pub(crate) fn sidecar_response_timeout() -> Result<Duration, Error> {
	let configured = getenv(SIDECAR_RESPONSE_TIMEOUT_ENV).unwrap_or_default();
	if configured.is_empty() {
		return Ok(DEFAULT_SIDECAR_RESPONSE_TIMEOUT);
	}
	parse_go_duration(&configured)
		.filter(|timeout| !timeout.is_zero())
		.ok_or_else(|| {
			Error::message(format!(
				"invalid {SIDECAR_RESPONSE_TIMEOUT_ENV} value {}",
				rotor_support::gostr::quote(&configured)
			))
		})
}

/// Go's `time.ParseDuration` for non-negative durations.
pub(crate) fn parse_go_duration(text: &str) -> Option<Duration> {
	if text == "0" {
		return Some(Duration::ZERO);
	}
	let mut rest = text.strip_prefix('+').unwrap_or(text);
	if rest.is_empty() || rest.starts_with('-') {
		return None;
	}
	let mut total = 0f64;
	while !rest.is_empty() {
		let number_end = rest
			.find(|character: char| !(character.is_ascii_digit() || character == '.'))
			.unwrap_or(rest.len());
		let number: f64 = rest[..number_end].parse().ok()?;
		rest = &rest[number_end..];
		let unit_end = rest
			.find(|character: char| character.is_ascii_digit() || character == '.')
			.unwrap_or(rest.len());
		let scale = match &rest[..unit_end] {
			"ns" => 1e-9,
			"us" | "µs" | "μs" => 1e-6,
			"ms" => 1e-3,
			"s" => 1.0,
			"m" => 60.0,
			"h" => 3600.0,
			_ => return None,
		};
		rest = &rest[unit_end..];
		total = number.mul_add(scale, total);
	}
	Duration::try_from_secs_f64(total).ok()
}

/// `runTransformerSidecarCall`
pub(crate) fn run_transformer_sidecar_call(cancel: Option<&Cancellation>, call: &TransformerCall) -> SidecarResult {
	run_transformer_sidecar_shard(cancel, 0, call)
}

/// `runTransformerSidecarShard`: one round trip through the worker that owns `shard` of a
/// project. Shard 0 is the project's ordinary worker, which unsharded requests also use.
#[expect(clippy::too_many_lines, reason = "one function in Go")]
pub(crate) fn run_transformer_sidecar_shard(
	cancel: Option<&Cancellation>,
	shard: usize,
	call: &TransformerCall,
) -> SidecarResult {
	let mut stats = SidecarCallStats::default();
	let sidecar_dir = match crate::sidecar_install::resolve_sidecar_dir() {
		Ok(dir) => dir,
		Err(error) => return Err(sidecar_failure(error, stats)),
	};
	let timeout = match sidecar_response_timeout() {
		Ok(timeout) => timeout,
		Err(error) => return Err(sidecar_failure(error, stats)),
	};
	let config_path = &call.config_path;
	let plugins = call.plugins.as_deref();
	let round_trip_stage = transformer_round_trip_stage_name(config_path, plugins);
	let (sidecar_dir_path, sidecar_config_path, mut key) = canonical_transformer_request_paths(&call.dir, config_path);
	if shard > 0 {
		key = format!("{key}#{shard}");
	}
	let slot = sidecar_slot_for(&key);
	let stop_wait = log_stage(config_path, Stage::SidecarSessionWait);
	let mut guard = slot.acquire();
	stats.wait = stop_wait();
	let fresh = !guard.used && guard.session.is_none();
	guard.used = true;

	let mut attempt = 0;
	loop {
		let mut stop_prep = Some(log_stage(config_path, Stage::SidecarPreparation));
		if guard.session.as_ref().is_none_or(|session| session.dead) {
			if let Some(mut session) = guard.session.take() {
				session.close();
			}
			// Plugins can derive project-relative paths from the working directory. Node starts in
			// the physical directory the request sends, so cwd, options and file paths agree.
			match spawn_sidecar_session_mode(&sidecar_dir_path, &sidecar_dir, fresh && call.parallel.is_some()) {
				Ok(session) => guard.session = Some(session),
				Err(error) => {
					if let Some(stop) = stop_prep.take() {
						stats.prep += stop();
					}
					return Err(sidecar_failure(
						node_requirement_error(error, config_path, plugins),
						stats,
					));
				}
			}
			if attempt == 0 {
				stats.spawned = true;
			} else {
				stats.restarted = true;
			}
		}
		let Some(session) = guard.session.as_mut() else {
			return Err(sidecar_failure(
				Error::message("compile: transformer sidecar session vanished"),
				stats,
			));
		};
		let request =
			match prepare_transformer_request(call, session, &sidecar_dir_path, &sidecar_config_path, &mut stats) {
				Ok(request) => request,
				Err(error) => {
					if let Some(stop) = stop_prep.take() {
						stats.prep += stop();
					}
					return Err(sidecar_failure(error, stats));
				}
			};
		if attempt == 0
			&& fresh && crate::sidecar_parallel::parallel_request_supported(call, &request)
			&& let Some(build) = &call.parallel_build
			&& build.try_split()
		{
			if let Some(stop) = stop_prep.take() {
				stats.prep += stop();
			}
			let outcome = crate::sidecar_parallel::run_parallel_transformer(
				cancel,
				timeout,
				&sidecar_dir,
				&mut guard,
				&request,
				call.parallel.as_ref(),
				&mut stats,
			);
			// An admitted split reaped both sessions before returning.
			build.release_split();
			match outcome {
				crate::sidecar_parallel::ParallelOutcome::Handled(result) => {
					if result.is_ok()
						&& let Some(state) = &call.state
					{
						lock(state).disk_scanned = true;
					}
					return result
						.map(|response| (*response, stats.clone()))
						.map_err(|error| sidecar_failure(error, stats));
				}
				crate::sidecar_parallel::ParallelOutcome::PreflightFailed => {
					// Worker 0 failed before any transform: the next attempt is the one remaining
					// ordinary serial attempt.
					attempt += 1;
					continue;
				}
				crate::sidecar_parallel::ParallelOutcome::Declined => {
					stop_prep = Some(log_stage(config_path, Stage::SidecarPreparation));
				}
			}
		}
		let payload = request.to_json();
		if let Some(stop) = stop_prep.take() {
			stats.prep += stop();
		}
		stats.request_bytes += count(payload.len());

		let Some(session) = guard.session.as_mut() else {
			return Err(sidecar_failure(
				Error::message("compile: transformer sidecar session vanished"),
				stats,
			));
		};
		let stop_round_trip = log_stage_named(config_path, &round_trip_stage);
		let line = session.write_and_read(cancel, Instant::now() + timeout, payload.as_bytes());
		stats.round_trip += stop_round_trip();
		session.drain_stderr();
		let line = match line {
			Ok(line) => line,
			Err((error, failure)) => {
				if let Some(mut session) = guard.session.take() {
					session.close();
				}
				if failure == RoundTripFailure::DeadlineExceeded || attempt > 0 {
					return Err(sidecar_failure(error, stats));
				}
				attempt += 1;
				continue;
			}
		};
		if let Some(state) = &call.state {
			lock(state).disk_scanned = true;
		}
		stats.response_bytes += count(line.len());
		let decode_started = Instant::now();
		let response = match TransformerResponse::from_json(String::from_utf8_lossy(&line).trim()) {
			Ok(response) => response,
			Err(error) => {
				let error = Error::message(error);
				let failed = guard
					.session
					.as_mut()
					.map_or_else(|| error.clone(), |session| session.fail(&error));
				return Err(sidecar_failure(failed, stats));
			}
		};
		stats.decode += decode_started.elapsed();
		apply_transformer_response_metrics(&mut stats, &response, config_path);
		return Ok((response, stats));
	}
}

/// `validateTransformerHost`: verifies the configured transformer modules without building a
/// JavaScript program or running a factory. Declarations come from the original program, so a
/// transform would only run plugin code that cannot affect them.
pub(crate) fn validate_transformer_host(
	cancel: Option<&Cancellation>,
	host: &dyn TransformerHost,
	dir: &str,
	program: &Program,
) -> Result<(), Failure> {
	let mut config_path = program.options().config_file_path.clone();
	if config_path.is_empty() {
		config_path = gopath::to_slash(&gopath::join(&[&gopath::from_slash(dir), "tsconfig.json"])).into_owned();
	}
	let (response, _) = host
		.validate(cancel, dir, &config_path)
		.map_err(|error| Failure::strings(vec![error.to_string()], error))?;
	if response.after_declarations_transformers > 0 {
		warn_unsupported_after_declarations(
			&config_path,
			usize::try_from(response.after_declarations_transformers).unwrap_or(0),
		);
	}
	let mut error_diags = Vec::new();
	for diagnostic in &response.diagnostics {
		let text = format_sidecar_diagnostic(diagnostic);
		if diagnostic.category.eq_ignore_ascii_case("warning") {
			logservice::warn(&text);
			continue;
		}
		error_diags.push(text);
	}
	if !error_diags.is_empty() {
		return Err(Failure::strings(
			error_diags,
			Error::message("compile: transformer sidecar diagnostics"),
		));
	}
	Ok(())
}

/// `runTransformerSidecarValidationWithContext`
pub(crate) fn run_transformer_sidecar_validation(
	cancel: Option<&Cancellation>,
	dir: &str,
	config_path: &str,
) -> Result<(TransformerResponse, SidecarCallStats), Error> {
	let mut stats = SidecarCallStats::default();
	let sidecar_dir = crate::sidecar_install::resolve_sidecar_dir()?;
	let timeout = sidecar_response_timeout()?;
	let (request, sidecar_dir_path, _, key) = prepare_transformer_validation_request(dir, config_path);
	let slot = sidecar_slot_for(&key);
	let stop_wait = log_stage(config_path, Stage::SidecarSessionWait);
	let mut guard = slot.acquire();
	stats.wait = stop_wait();
	guard.used = true;
	let mut attempt = 0;
	loop {
		let stop_prep = log_stage(config_path, Stage::SidecarPreparation);
		if guard.session.as_ref().is_none_or(|session| session.dead) {
			if let Some(mut session) = guard.session.take() {
				session.close();
			}
			match spawn_sidecar_session(&sidecar_dir_path, &sidecar_dir) {
				Ok(session) => guard.session = Some(session),
				Err(error) => {
					stats.prep += stop_prep();
					return Err(node_requirement_error(error, config_path, None));
				}
			}
			if attempt == 0 {
				stats.spawned = true;
			} else {
				stats.restarted = true;
			}
		}
		let payload = request.to_json();
		stats.prep += stop_prep();
		stats.request_bytes += count(payload.len());
		let Some(session) = guard.session.as_mut() else {
			return Err(Error::message("compile: transformer sidecar session vanished"));
		};
		let stop_round_trip = log_stage_named(config_path, Stage::SidecarRoundTrip.trace_name());
		let line = session.write_and_read(cancel, Instant::now() + timeout, payload.as_bytes());
		stats.round_trip += stop_round_trip();
		session.drain_stderr();
		let line = match line {
			Ok(line) => line,
			Err((error, failure)) => {
				if let Some(mut session) = guard.session.take() {
					session.close();
				}
				if failure == RoundTripFailure::DeadlineExceeded || attempt > 0 {
					return Err(error);
				}
				attempt += 1;
				continue;
			}
		};
		stats.response_bytes += count(line.len());
		let decode_started = Instant::now();
		let response = match TransformerResponse::from_json(String::from_utf8_lossy(&line).trim()) {
			Ok(response) => response,
			Err(error) => {
				let error = Error::message(error);
				return Err(guard
					.session
					.as_mut()
					.map_or_else(|| error.clone(), |session| session.fail(&error)));
			}
		};
		stats.decode += decode_started.elapsed();
		apply_transformer_response_metrics(&mut stats, &response, config_path);
		return Ok((response, stats));
	}
}

/// `canonicalSidecarPath`: the physical spelling the worker's resolver sees after `chdir`
/// (macOS `/var` is `/private/var`; Windows resolves junctions and short names).
pub(crate) fn canonical_sidecar_path(path: &str) -> String {
	let cleaned = gopath::clean(&gopath::from_slash(path));
	fsutil::eval_symlinks(&cleaned).map_or_else(
		|_| gopath::to_slash(&cleaned).into_owned(),
		|resolved| gopath::to_slash(&resolved).into_owned(),
	)
}

/// `transformerRoundTripStageName`: verbose builds name the plugins the round trip runs.
pub(crate) fn transformer_round_trip_stage_name(config_path: &str, plugins: Option<&[String]>) -> String {
	let mut stage = Stage::SidecarRoundTrip.trace_name().to_owned();
	if logservice::verbose() {
		let names = sidecar_plugin_names(plugins, config_path);
		if !names.is_empty() {
			stage = format!("{stage} ({})", names.join(", "));
		}
	}
	stage
}

/// `logPluginMetrics`
fn log_plugin_metrics(config_path: &str, plugins: &[crate::transformer_host::TransformerPluginMetric]) {
	let label = project_label(config_path);
	for plugin in plugins {
		logservice::write_stage_done_if_verbose(
			&label,
			&format!("transformer plugin {}", plugin.transform),
			millis(plugin.ms),
		);
	}
}

fn millis(ms: i64) -> Duration {
	Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

/// `applyTransformerResponseMetrics`
pub(crate) fn apply_transformer_response_metrics(
	stats: &mut SidecarCallStats,
	response: &TransformerResponse,
	config_path: &str,
) {
	let Some(metrics) = &response.metrics else {
		return;
	};
	stats.node_wall_ms = metrics.wall_ms;
	stats.node_cpu_user_us = metrics.cpu_user_us;
	stats.node_cpu_system_us = metrics.cpu_system_us;
	stats.node_version.clone_from(&metrics.node_version);
	log_plugin_metrics(config_path, &metrics.plugins);
	if let Some(phases) = &metrics.phases {
		let label = project_label(config_path);
		for (name, ms) in [
			("transformer worker config", phases.config_ms),
			("transformer worker program", phases.program_ms),
			("transformer worker factories", phases.factories_ms),
			(
				"transformer worker transform (includes parent repair)",
				phases.transform_ms,
			),
			("transformer worker parent repair", phases.parent_repair_ms),
			("transformer worker print/trace map", phases.print_ms),
		] {
			logservice::write_stage_done_if_verbose(&label, name, millis(ms));
		}
	}
}

/// `nodeRequirementError`: names the plugin when exactly one needs the missing Node.
pub(crate) fn node_requirement_error(error: Error, config_path: &str, plugins: Option<&[String]>) -> Error {
	if !error.to_string().contains("node executable not found") {
		return error;
	}
	let names: Vec<String> = plugins.map_or_else(
		|| config_file_transformer_plugins(&normalize_source_file_path(config_path)).unwrap_or_default(),
		|plugins| plugins.iter().filter_map(|plugin| plugin_transform(plugin)).collect(),
	);
	if let [name] = names.as_slice() {
		return Error::message(format!(
			"node executable not found; external transformer {} requires Node.js on PATH",
			rotor_support::gostr::quote(name)
		));
	}
	error
}

fn plugin_transform(plugin: &str) -> Option<String> {
	let value = crate::gojson::parse(plugin.as_bytes()).ok()?;
	let transform = value.field("transform")?.as_str()?;
	(!transform.is_empty()).then(|| transform.to_owned())
}

/// `closeSidecarSessions`: closes every worker; the process is exiting or a test is done.
pub fn close_sidecar_sessions() {
	let taken: Vec<Arc<SidecarSlot>> = std::mem::take(&mut *lock(slots())).into_values().collect();
	for slot in taken {
		let mut guard = slot.acquire();
		if let Some(mut session) = guard.session.take() {
			session.close();
		}
	}
}

/// `mergeSidecarOverlays`: the caller's overlays, the stage overlays of earlier stages, and the
/// in-memory text of compiled files that differs from disk; and how many files it read.
pub(crate) fn merge_sidecar_overlays(
	compile_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
	state: Option<&SidecarBuildState>,
	include_stage_overlays: bool,
) -> (BTreeMap<String, String>, i64) {
	let mut merged = overlays.clone();
	// Go's `overlayByPath` scans every key for a spelling it does not hold; the normalized keys
	// answer the same question without that scan, which is quadratic over a project's overlays.
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let mut normalized: HashSet<String> = merged
		.keys()
		.map(|key| normalize_overlay_path(key, case_sensitive))
		.collect();
	let has_overlay = |merged: &BTreeMap<String, String>, normalized: &HashSet<String>, path: &str| {
		merged.contains_key(path) || normalized.contains(&normalize_overlay_path(path, case_sensitive))
	};
	if let Some(state) = state {
		if include_stage_overlays {
			for (path, text) in &state.stage_overlays {
				if has_overlay(&merged, &normalized, path) {
					continue;
				}
				normalized.insert(normalize_overlay_path(path, case_sensitive));
				merged.insert(path.clone(), text.clone());
			}
		}
		if state.disk_scanned {
			return (merged, 0);
		}
	}
	let mut reads = 0;
	for file in compile_files {
		if has_overlay(&merged, &normalized, file.file_name()) {
			continue;
		}
		let on_disk = std::fs::read(file.file_name());
		reads += 1;
		let Ok(on_disk) = on_disk else {
			continue;
		};
		if file.text().as_bytes() == on_disk.as_slice() {
			continue;
		}
		merged.insert(file.file_name().to_owned(), file.text().to_owned());
	}
	(merged, reads)
}

/// `ancestorNodeModules`: the `node_modules` directories Node searches from `project_dir`,
/// nearest first. A plugin resolves its own `typescript` from its realpath, which under pnpm is
/// outside the workspace, so `NODE_PATH` carries the walk.
pub(crate) fn ancestor_node_modules(project_dir: &str) -> Vec<String> {
	if project_dir.is_empty() {
		return Vec::new();
	}
	let mut paths = Vec::new();
	let mut dir = gopath::clean(&gopath::from_slash(project_dir));
	loop {
		paths.push(gopath::join(&[&dir, "node_modules"]));
		let parent = gopath::dir(&dir);
		if parent == dir {
			return paths;
		}
		dir = parent;
	}
}

/// `sidecarEnv`: the process environment with `NODE_PATH` extended by the project's
/// `node_modules` ancestry and the worker's own, and `NODE_OPTIONS` without the package map.
pub(crate) fn sidecar_env(project_dir: &str, sidecar_dir: &str) -> Vec<(String, String)> {
	let vars = std::env::vars_os()
		.map(|(name, value)| {
			(
				name.to_string_lossy().into_owned(),
				value.to_string_lossy().into_owned(),
			)
		})
		.collect();
	#[cfg(test)]
	let vars = {
		let mut vars: Vec<(String, String)> = vars;
		for (name, value) in lock(test_env()).iter() {
			vars.retain(|(existing, _)| existing != name);
			vars.push((name.clone(), value.clone()));
		}
		vars
	};
	sidecar_env_from(vars, project_dir, sidecar_dir)
}

/// [`sidecar_env`] over the environment `env`.
pub(crate) fn sidecar_env_from(
	mut env: Vec<(String, String)>,
	project_dir: &str,
	sidecar_dir: &str,
) -> Vec<(String, String)> {
	let mut node_paths = ancestor_node_modules(project_dir);
	node_paths.push(gopath::join(&[sidecar_dir, "node_modules"]));
	let mut filtered = Vec::new();
	let mut seen = HashSet::new();
	for path in node_paths {
		if path.is_empty() {
			continue;
		}
		let path = gopath::clean(&path);
		if std::fs::metadata(&path).is_err() {
			continue;
		}
		let key = if cfg!(windows) {
			path.to_lowercase()
		} else {
			path.clone()
		};
		if seen.insert(key) {
			filtered.push(path);
		}
	}
	for (name, value) in &mut env {
		if name.eq_ignore_ascii_case("NODE_OPTIONS") {
			*value = sidecar_node_options(value);
		}
	}
	if filtered.is_empty() {
		return env;
	}
	let separator = if cfg!(windows) { ";" } else { ":" };
	let mut node_path = filtered.join(separator);
	if let Some((_, value)) = env.iter_mut().find(|(name, _)| name == "NODE_PATH") {
		if !value.is_empty() {
			node_path = format!("{node_path}{separator}{value}");
		}
		*value = node_path;
		return env;
	}
	env.push(("NODE_PATH".to_owned(), node_path));
	env
}

/// `sidecarNodeOptions`: `NODE_OPTIONS` without `--experimental-package-map`. The extracted
/// worker lives outside the project's package map, and Node ignores `require.resolve`'s paths
/// under one. Malformed options are left for Node to report.
pub(crate) fn sidecar_node_options(options: &str) -> String {
	let bytes = options.as_bytes();
	let mut kept: Vec<&str> = Vec::new();
	let mut removed = false;
	let mut skip_value = false;
	let mut index = 0;
	while index < bytes.len() {
		if bytes[index] == b' ' {
			index += 1;
			continue;
		}
		let start = index;
		let mut value: Vec<u8> = Vec::new();
		let mut quoted = false;
		while index < bytes.len() {
			let character = bytes[index];
			if character == b' ' && !quoted {
				break;
			}
			index += 1;
			if character == b'\\' && quoted {
				if index == bytes.len() {
					return options.to_owned();
				}
				value.push(bytes[index]);
				index += 1;
			} else if character == b'"' {
				quoted = !quoted;
			} else {
				value.push(character);
			}
		}
		if quoted {
			return options.to_owned();
		}
		let value = String::from_utf8_lossy(&value).into_owned();
		if skip_value {
			// Node ignores empty quoted arguments. A following option or no value at all is
			// malformed; keep it for Node's own error.
			if value.is_empty() {
				continue;
			}
			if value.starts_with('-') {
				return options.to_owned();
			}
			skip_value = false;
			continue;
		}
		// Node accepts underscores in option names as aliases for hyphens.
		let (name, has_value) = value
			.split_once('=')
			.map_or((value.as_str(), false), |(name, _)| (name, true));
		if name.replace('_', "-") == "--experimental-package-map" {
			removed = true;
			skip_value = !has_value;
			continue;
		}
		kept.push(&options[start..index]);
	}
	if !removed || skip_value {
		return options.to_owned();
	}
	kept.join(" ")
}

/// `remapProgramSourceFiles`: the same files, by path, in a derived program.
///
/// # Errors
///
/// A file is missing from the derived program.
pub fn remap_program_source_files(
	program: &Program,
	source_files: &[P<SourceFile>],
) -> Result<Vec<P<SourceFile>>, Error> {
	let by_path: HashMap<String, P<SourceFile>> = project_source_files(program)
		.into_iter()
		.map(|file| (normalize_source_file_path(file.file_name()), file))
		.collect();
	source_files
		.iter()
		.map(|file| {
			let path = normalize_source_file_path(file.file_name());
			by_path.get(&path).copied().ok_or_else(|| {
				Error::message(format!(
					"compile: transformed source file missing from overlay program: {path}"
				))
			})
		})
		.collect()
}

/// `projectUsesTransformerPlugins`: whether this project has a transformer to run. The project's
/// own config answers when it declares `plugins`; otherwise any config of its extends chain that
/// declares a transform does.
pub(crate) fn project_uses_transformer_plugins(parsed: Option<&ParsedCommandLine>) -> bool {
	let Some(parsed) = parsed else {
		return false;
	};
	let config_name = parsed.config_name();
	if !config_name.is_empty()
		&& let (declares_transform, true) = config_file_plugins_declaration(&normalize_source_file_path(config_name))
	{
		return declares_transform;
	}
	let mut seen = HashSet::new();
	for config_path in parsed.extended_source_files() {
		if config_path.is_empty() {
			continue;
		}
		let path = normalize_source_file_path(&config_path);
		if !seen.insert(path.clone()) {
			continue;
		}
		if config_file_plugins_declaration(&path).0 {
			return true;
		}
	}
	false
}

/// `configFilePluginsDeclaration`: whether the config's own `plugins` names a transform, and
/// whether it declares the key at all.
pub(crate) fn config_file_plugins_declaration(config_path: &str) -> (bool, bool) {
	config_file_transformer_plugins(config_path).map_or((false, false), |plugins| (!plugins.is_empty(), true))
}

/// `configFileTransformerPlugins`: the `transform` names of the config's own
/// `compilerOptions.plugins`; `None` when it does not declare an array there.
pub(crate) fn config_file_transformer_plugins(config_path: &str) -> Option<Vec<String>> {
	let data = std::fs::read(config_path).ok()?;
	let text = rotor_tsrs::tsconfig::strip_jsonc(&String::from_utf8_lossy(&data));
	let root: serde_json::Value = serde_json::from_str(&text).ok()?;
	let plugins = root.get("compilerOptions")?.as_object()?.get("plugins")?.as_array()?;
	Some(
		plugins
			.iter()
			.filter_map(|plugin| plugin.get("transform")?.as_str())
			.filter(|transform| !transform.is_empty())
			.map(str::to_owned)
			.collect(),
	)
}

/// `sidecarPluginNames`: the explicit plugins' `transform`s, else the config chain's.
pub(crate) fn sidecar_plugin_names(plugins: Option<&[String]>, config_path: &str) -> Vec<String> {
	if let Some(plugins) = plugins
		&& !plugins.is_empty()
	{
		return plugins.iter().filter_map(|plugin| plugin_transform(plugin)).collect();
	}
	crate::transformer_order::effective_transformer_plugins(config_path)
		.map(|plugins| plugins.into_iter().map(|plugin| plugin.transform).collect())
		.unwrap_or_default()
}

/// `formatSidecarDiagnostic`
pub(crate) fn format_sidecar_diagnostic(diagnostic: &TransformerDiagnostic) -> String {
	if diagnostic.file.is_empty() {
		return diagnostic.message.clone();
	}
	format!("{}: {}", gopath::from_slash(&diagnostic.file), diagnostic.message)
}

/// `normalizeOverlayPath`
pub(crate) fn normalize_overlay_path(path: &str, case_sensitive: bool) -> String {
	let path = normalize_source_file_path(path);
	if case_sensitive { path } else { path.to_lowercase() }
}

/// `osvfs.FS().Realpath`: the physical spelling of an absolute path, the path itself for a
/// relative one (the file system asserts a rooted argument).
pub(crate) fn realpath_or(path: &str) -> String {
	let slashed = gopath::to_slash(path);
	if rotor_tsrs::tspath::get_root_length(&slashed) == 0 {
		return path.to_owned();
	}
	gopath::from_slash(&rotor_tsrs::vfs::osvfs::fs().realpath(&slashed)).into_owned()
}
