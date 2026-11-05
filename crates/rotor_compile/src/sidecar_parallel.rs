//! `sidecar_parallel.go`: the attested parallel transformer split.
//!
//! A one-shot CLI build whose `rotor.toml` attests two batch-independent workers splits its first
//! external stage between two fresh workers, after both pass a preflight.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rotor_config::config::{Config, ParallelTransformersConfig};
use rotor_support::{gopath, logservice};

use crate::compile::Error;
use crate::project::{Cancellation, ProjectOptions};
use crate::sidecar::{
	RoundTripFailure, SidecarCallStats, SidecarSession, SlotGuard, apply_transformer_response_metrics,
	normalize_overlay_path, spawn_sidecar_session_mode, transformer_round_trip_stage_name,
};
use crate::timings::{log_stage_named, project_label};
use crate::transformer_host::{
	TransformerCall, TransformerDiagnostic, TransformerPluginMetric, TransformerRequest, TransformerResponse,
	TransformerResponseMetrics,
};

/// `cliTransformerBuild`: the one-shot CLI build's split token. Option copies share it.
#[derive(Debug, Default)]
pub struct CliTransformerBuild {
	split: AtomicBool,
}

impl CliTransformerBuild {
	/// `trySplit`: takes the token if no split holds it.
	pub(crate) fn try_split(&self) -> bool {
		self.split
			.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
			.is_ok()
	}

	/// Returns the token.
	pub(crate) fn release_split(&self) {
		self.split.store(false, Ordering::Release);
	}
}

/// `MarkOneShotCLITransformerBuild`: only the non-watch, non-declaration CLI build calls this.
pub fn mark_one_shot_cli_transformer_build(opts: &mut ProjectOptions) {
	opts.cli_transformer_build = Some(Arc::new(CliTransformerBuild::default()));
}

/// `parallelPrefix`: the attested split a prefix stage may take.
pub(crate) struct ParallelPrefix {
	pub(crate) attestation: ParallelTransformersConfig,
	pub(crate) build: Arc<CliTransformerBuild>,
}

/// `parallelPrefixContext`: the split a one-shot CLI build's Flamework prefix stage may take, when
/// that stage is the build's only external call and `rotor.toml` attests it.
pub(crate) fn parallel_prefix_context(
	dir: &str,
	pipeline: Option<&crate::flamework_pipeline::FlameworkPipeline>,
	opts: &ProjectOptions,
) -> Option<ParallelPrefix> {
	let pipeline = pipeline?;
	if pipeline.prefix.is_empty()
		|| !pipeline.suffix.is_empty()
		|| opts.census.is_some()
		|| opts.emit_declaration_only
		|| opts.transformer_callback.is_some()
	{
		return None;
	}
	let build = opts.cli_transformer_build.as_ref()?;
	let config = rotor_config::config::load(dir).ok()?;
	if !parallel_config_supported(&config) {
		return None;
	}
	Some(ParallelPrefix {
		attestation: config.parallel_transformers?,
		build: Arc::clone(build),
	})
}

/// `parallelConfigSupported`: two batch-independent workers, a TypeScript and plugins, and no
/// warning about a `parallelTransformers` key whose meaning this build does not know.
pub(crate) fn parallel_config_supported(config: &Config) -> bool {
	let Some(parallel) = &config.parallel_transformers else {
		return false;
	};
	if parallel.workers != 2
		|| !parallel.batch_independent
		|| parallel.typescript.is_empty()
		|| parallel.plugins.is_empty()
	{
		return false;
	}
	!config
		.warnings
		.iter()
		.any(|warning| warning.contains("\"parallelTransformers"))
}

/// `parallelRequestSupported`: the split applies only to a whole-project request of authored
/// compile sources.
pub(crate) fn parallel_request_supported(call: &TransformerCall, request: &TransformerRequest) -> bool {
	let Some(parallel) = &call.parallel else {
		return false;
	};
	let compile_file_names = request.compile_file_names.as_deref().unwrap_or_default();
	if parallel.workers != 2
		|| !parallel.batch_independent
		|| compile_file_names.len() < 2
		|| !request.root_file_names.is_empty()
		|| request.plugins.is_empty()
	{
		return false;
	}
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	// Every overlay must be an authored compile source, never a config, declaration, manifest or
	// another stage's synthetic input.
	let mut compile_names = HashSet::with_capacity(compile_file_names.len());
	for name in compile_file_names {
		if !compile_names.insert(normalize_overlay_path(name, case_sensitive)) {
			return false;
		}
	}
	for changed in request.changed_files.as_deref().unwrap_or_default() {
		// Go compares the lowercased name's suffixes.
		let name = changed.file_name.to_lowercase();
		#[expect(
			clippy::case_sensitive_file_extension_comparisons,
			reason = "the name is lowercased first, as Go's strings.HasSuffix check is"
		)]
		let compile_source = (name.ends_with(".ts") || name.ends_with(".tsx")) && !name.ends_with(".d.ts");
		if !compile_names.contains(&normalize_overlay_path(&changed.file_name, case_sensitive)) || !compile_source {
			return false;
		}
	}
	parallel_resolution_scope(&call.dir, compile_file_names)
}

/// `parallelResolutionScope`: every batch's sources share the root's package-resolution scope.
/// Only unique ancestor directories are read; aliases (symlinks included) select serial.
pub(crate) fn parallel_resolution_scope(project_dir: &str, files: &[String]) -> bool {
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let Ok(root) = gopath::abs(&gopath::from_slash(project_dir)) else {
		return false;
	};
	let Ok(physical) = rotor_support::fsutil::eval_symlinks(&root) else {
		return false;
	};
	if !same_parallel_path(&root, &physical) {
		return false;
	}
	let up = format!("..{}", gopath::NATIVE.separator());
	let mut checked = HashSet::new();
	let mut regular_files = HashSet::new();
	for file in files {
		if !gopath::is_abs(file) {
			return false;
		}
		let Ok(relative) = gopath::rel(&root, &gopath::clean(file)) else {
			return false;
		};
		if relative == ".." || relative.starts_with(&up) || gopath::is_abs(&relative) {
			return false;
		}
		if relative
			.split(gopath::NATIVE.separator())
			.any(|segment| segment.eq_ignore_ascii_case("node_modules"))
		{
			return false;
		}
		let mut dir = gopath::dir(file);
		loop {
			let key = normalize_overlay_path(&dir, case_sensitive);
			if checked.contains(&key) {
				break;
			}
			let Ok(actual) = rotor_support::fsutil::eval_symlinks(&dir) else {
				return false;
			};
			if !same_parallel_path(&dir, &actual) {
				return false;
			}
			let Ok(entries) = std::fs::read_dir(&dir) else {
				return false;
			};
			let at_root = same_parallel_path(&dir, &root);
			let mut names = Vec::new();
			for entry in entries {
				let Ok(entry) = entry else {
					return false;
				};
				let Ok(file_type) = entry.file_type() else {
					return false;
				};
				names.push((entry.file_name().to_string_lossy().into_owned(), file_type));
			}
			for (name, file_type) in names {
				if at_root && name.eq_ignore_ascii_case("node_modules") {
					// The common root dependency scope is allowed.
					continue;
				}
				// Conservative: an unrelated symlink in a compile scope selects serial too.
				let irregular = !(file_type.is_file() || file_type.is_dir());
				if file_type.is_symlink()
					|| irregular || !at_root
					&& (name.eq_ignore_ascii_case("package.json") || name.eq_ignore_ascii_case("node_modules"))
				{
					return false;
				}
				if file_type.is_file() {
					regular_files.insert(normalize_overlay_path(&gopath::join(&[&dir, &name]), case_sensitive));
				}
			}
			checked.insert(key);
			if at_root {
				break;
			}
			dir = gopath::dir(&dir);
		}
		if !regular_files.contains(&normalize_overlay_path(file, case_sensitive)) {
			return false;
		}
	}
	true
}

/// `sameParallelPath`: equal after cleaning, aliases kept apart.
fn same_parallel_path(left: &str, right: &str) -> bool {
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	normalize_overlay_path(&gopath::clean(left), case_sensitive)
		== normalize_overlay_path(&gopath::clean(right), case_sensitive)
}

/// `splitParallelRequest`: the first half (rounded up) and the rest.
pub(crate) fn split_parallel_request(request: &TransformerRequest) -> [TransformerRequest; 2] {
	let names = request.compile_file_names.clone().unwrap_or_default();
	let mid = names.len().div_ceil(2);
	let mut first = request.clone();
	let mut second = request.clone();
	first.compile_file_names = Some(names[..mid].to_vec());
	second.compile_file_names = Some(names[mid..].to_vec());
	first.parallel_worker = true;
	second.parallel_worker = true;
	[first, second]
}

/// What a split attempt did.
pub(crate) enum ParallelOutcome {
	/// A transform was sent; its outcome is final.
	Handled(Result<Box<TransformerResponse>, Error>),
	/// Worker 0 failed its preflight: the slot's session is gone and the serial retry is spent.
	PreflightFailed,
	/// Nothing was sent and worker 0 is unchanged.
	Declined,
}

/// One worker's transport result, its decode result inside, and its costs.
type WorkerResult = (
	Result<Result<TransformerResponse, String>, (Error, RoundTripFailure)>,
	SidecarCallStats,
);

/// A spawner of worker sessions: `(dir, sidecar_dir, owned_stderr)`.
pub(crate) type Spawner<'a> = &'a dyn Fn(&str, &str, bool) -> Result<SidecarSession, Error>;

/// `runParallelTransformer`
pub(crate) fn run_parallel_transformer(
	cancel: Option<&Cancellation>,
	timeout: Duration,
	sidecar_dir: &str,
	guard: &mut SlotGuard,
	request: &TransformerRequest,
	attestation: Option<&ParallelTransformersConfig>,
	stats: &mut SidecarCallStats,
) -> ParallelOutcome {
	run_parallel_transformer_with_spawner(
		cancel,
		timeout,
		sidecar_dir,
		guard,
		request,
		attestation,
		stats,
		&spawn_sidecar_session_mode,
	)
}

fn join_errors(errors: impl IntoIterator<Item = Option<String>>) -> Option<Error> {
	let errors: Vec<String> = errors.into_iter().flatten().collect();
	(!errors.is_empty()).then(|| Error::message(errors.join("\n")))
}

/// `runParallelTransformerWithSpawner`
#[expect(clippy::too_many_arguments, reason = "Go's signature")]
#[expect(clippy::too_many_lines, reason = "one function in Go")]
pub(crate) fn run_parallel_transformer_with_spawner(
	cancel: Option<&Cancellation>,
	timeout: Duration,
	sidecar_dir: &str,
	guard: &mut SlotGuard,
	request: &TransformerRequest,
	attestation: Option<&ParallelTransformersConfig>,
	stats: &mut SidecarCallStats,
	spawn: Spawner<'_>,
) -> ParallelOutcome {
	let Some(mut first) = guard.session.take() else {
		return ParallelOutcome::Declined;
	};
	let parent_cancelled = || cancel.is_some_and(Cancellation::is_cancelled);
	let deadline = Instant::now() + timeout;
	let mut preflight = request.clone();
	"parallel-preflight".clone_into(&mut preflight.operation);
	preflight.parallel_attestation = attestation.cloned();
	let check = |session: &mut SidecarSession, stats: &mut SidecarCallStats| match parallel_round_trip(
		&[cancel],
		deadline,
		session,
		&preflight,
		stats,
	) {
		Ok(Ok(response)) => Ok(response.parallel_ready && response.diagnostics.is_empty()),
		Ok(Err(error)) => Err((Error::message(error), RoundTripFailure::Transport)),
		Err(failure) => Err(failure),
	};
	match check(&mut first, stats) {
		Err((error, failure)) => {
			first.close();
			first.drain_stderr();
			if parent_cancelled() || failure == RoundTripFailure::DeadlineExceeded {
				return ParallelOutcome::Handled(Err(error));
			}
			return ParallelOutcome::PreflightFailed;
		}
		Ok(false) => {
			if parent_cancelled() {
				first.close();
				first.drain_stderr();
				return ParallelOutcome::Handled(Err(Error::message("context canceled")));
			}
			guard.session = Some(first);
			return ParallelOutcome::Declined;
		}
		Ok(true) => {}
	}
	let Ok(mut second) = spawn(&request.project_dir, sidecar_dir, true) else {
		guard.session = Some(first);
		return ParallelOutcome::Declined;
	};
	stats.additional_spawns += 1;
	let checked = check(&mut second, stats);
	if !matches!(checked, Ok(true)) {
		let deadline_exceeded = matches!(&checked, Err((_, RoundTripFailure::DeadlineExceeded)));
		let cleanup_err = second.close_with_expected_kill(deadline_exceeded).err();
		let parent_err = parent_cancelled().then(|| "context canceled".to_owned());
		if parent_err.is_some() || cleanup_err.is_some() {
			let primary_cleanup_err = first.close_and_wait().err();
			first.drain_stderr();
			return ParallelOutcome::Handled(Err(join_errors([
				parent_err,
				cleanup_err.map(|error| error.to_string()),
				primary_cleanup_err.map(|error| error.to_string()),
			])
			.unwrap_or_else(|| Error::message("context canceled"))));
		}
		guard.session = Some(first);
		return ParallelOutcome::Declined;
	}

	let deadline = Instant::now() + timeout;
	let sibling = Cancellation::default();
	let [first_request, second_request] = split_parallel_request(request);
	let stage_name = transformer_round_trip_stage_name(&request.ts_config_path, Some(&request.plugins));
	let stop_round_trip = log_stage_named(&request.ts_config_path, &stage_name);
	let mut workers = [first, second];
	let results: [WorkerResult; 2] = std::thread::scope(|scope| {
		let [first_worker, second_worker] = &mut workers;
		let sibling = &sibling;
		let run = |worker: &mut SidecarSession, request: &TransformerRequest| {
			let mut worker_stats = SidecarCallStats::default();
			let result = parallel_round_trip(&[cancel, Some(sibling)], deadline, worker, request, &mut worker_stats);
			if result.is_err() {
				sibling.cancel();
			}
			(result, worker_stats)
		};
		let second_handle = scope.spawn(move || run(second_worker, &second_request));
		let first_result = run(first_worker, &first_request);
		let second_result = second_handle.join().unwrap_or_else(|_| {
			(
				Err((
					Error::message("compile: transformer worker panicked"),
					RoundTripFailure::Transport,
				)),
				SidecarCallStats::default(),
			)
		});
		[first_result, second_result]
	});
	let wall = stop_round_trip();
	// Both are reaped before stderr is forwarded, so neither range loses its final bytes.
	let cleanup_errors: Vec<Option<String>> = workers
		.iter_mut()
		.enumerate()
		.map(|(index, worker)| {
			worker
				.close_and_wait()
				.err()
				.map(|error| format!("transformer worker {index} cleanup: {error}"))
		})
		.collect();
	for (_, worker_stats) in &results {
		stats.request_bytes += worker_stats.request_bytes;
		stats.response_bytes += worker_stats.response_bytes;
		stats.decode += worker_stats.decode;
	}
	stats.round_trip += wall;
	let [(first_result, _), (second_result, _)] = results;
	let decode = |result: Result<Result<TransformerResponse, String>, (Error, RoundTripFailure)>| match result {
		Ok(Ok(response)) => Ok(response),
		Ok(Err(error)) => Err((Error::message(error), RoundTripFailure::Transport)),
		Err(error) => Err(error),
	};
	let first_result = decode(first_result);
	let second_result = decode(second_result);
	if let Ok(response) = &first_result
		&& parallel_internal_failure(Some(response))
	{
		let mut response = response.clone();
		response.parallel_batch_diagnostics = 0;
		response.parallel_source_diagnostics = 0;
		let second_metrics = second_result
			.as_ref()
			.ok()
			.and_then(|response| response.metrics.as_ref());
		response.metrics = merge_parallel_metrics(response.metrics.as_ref(), second_metrics, wall);
		workers[0].drain_stderr();
		apply_transformer_response_metrics(stats, &response, &request.ts_config_path);
		return ParallelOutcome::Handled(Ok(Box::new(response)));
	}
	// A real transport failure wins over its sibling's cancellation; two failures keep range
	// order.
	let failures = [&first_result, &second_result];
	let failure = failures
		.iter()
		.find_map(|result| match result {
			Err((error, kind)) if *kind != RoundTripFailure::Canceled => Some(error.clone()),
			_ => None,
		})
		.or_else(|| {
			failures.iter().find_map(|result| match result {
				Err((error, _)) => Some(error.clone()),
				Ok(_) => None,
			})
		});
	if let Some(error) = failure {
		for worker in &workers {
			worker.drain_stderr();
		}
		let mut errors = vec![Some(error.to_string())];
		errors.extend(cleanup_errors);
		return ParallelOutcome::Handled(Err(join_errors(errors).unwrap_or(error)));
	}
	let (Ok(first_response), Ok(second_response)) = (first_result, second_result) else {
		return ParallelOutcome::Handled(Err(Error::message("compile: transformer workers failed")));
	};
	let (merged, last) = merge_parallel_responses(&first_response, &second_response, wall);
	for worker in workers.iter().take(last + 1) {
		worker.drain_stderr();
	}
	// An ordered in-band catch already fails the build with the serial shape. Otherwise an
	// incomplete drain or abnormal exit must not produce green output.
	if let Ok(response) = &merged
		&& parallel_internal_failure(Some(response))
	{
		let response = response.clone();
		apply_transformer_response_metrics(stats, &response, &request.ts_config_path);
		return ParallelOutcome::Handled(Ok(Box::new(response)));
	}
	let mut errors = vec![merged.as_ref().err().map(ToString::to_string)];
	errors.extend(cleanup_errors);
	if let Some(error) = join_errors(errors) {
		return ParallelOutcome::Handled(Err(error));
	}
	let Ok(response) = merged else {
		return ParallelOutcome::Handled(Err(Error::message("compile: transformer workers failed")));
	};
	apply_transformer_response_metrics(stats, &response, &request.ts_config_path);
	logservice::write_line_if_verbose(&[&format!(
		"{}: transformer parallel batch completed (workers=2)",
		project_label(&request.ts_config_path)
	)]);
	ParallelOutcome::Handled(Ok(Box::new(response)))
}

/// `parallelRoundTrip`: the transport result, and within it the decode result.
fn parallel_round_trip(
	cancels: &[Option<&Cancellation>],
	deadline: Instant,
	session: &mut SidecarSession,
	request: &TransformerRequest,
	stats: &mut SidecarCallStats,
) -> Result<Result<TransformerResponse, String>, (Error, RoundTripFailure)> {
	if cancels
		.iter()
		.any(|cancel| cancel.is_some_and(Cancellation::is_cancelled))
	{
		let error = Error::message("transformer sidecar response timed out: context canceled");
		return Err((session.fail(&error), RoundTripFailure::Canceled));
	}
	if Instant::now() >= deadline {
		let error = Error::message("transformer sidecar response timed out: context deadline exceeded");
		return Err((session.fail(&error), RoundTripFailure::DeadlineExceeded));
	}
	let payload = request.to_json();
	stats.request_bytes += i64::try_from(payload.len()).unwrap_or(i64::MAX);
	let started = Instant::now();
	let line = session.write_and_read_any(cancels, deadline, payload.as_bytes());
	stats.round_trip += started.elapsed();
	let line = line?;
	stats.response_bytes += i64::try_from(line.len()).unwrap_or(i64::MAX);
	let started = Instant::now();
	let response = TransformerResponse::from_json(String::from_utf8_lossy(&line).trim());
	stats.decode += started.elapsed();
	Ok(response)
}

/// `mergeParallelResponses`: the merged response and the last range whose stderr belongs to it.
pub(crate) fn merge_parallel_responses(
	first: &TransformerResponse,
	second: &TransformerResponse,
	wall: Duration,
) -> (Result<TransformerResponse, Error>, usize) {
	for (index, response) in [first, second].into_iter().enumerate() {
		if parallel_internal_failure(Some(response)) {
			// The serial catch discards the whole batch; keep the first failing range's exact
			// diagnostic and drop later ranges' stderr.
			let mut copy = response.clone();
			copy.parallel_batch_diagnostics = 0;
			copy.parallel_source_diagnostics = 0;
			copy.metrics = merge_parallel_metrics(first.metrics.as_ref(), second.metrics.as_ref(), wall);
			return (Ok(copy), index);
		}
	}
	let (a, b) = (first.parallel_batch_diagnostics, second.parallel_batch_diagnostics);
	let (x, y) = (first.parallel_source_diagnostics, second.parallel_source_diagnostics);
	let len = |diagnostics: &[TransformerDiagnostic]| i64::try_from(diagnostics.len()).unwrap_or(i64::MAX);
	let incompatible =
		a < 0
			|| b < 0 || a > len(&first.diagnostics)
			|| b > len(&second.diagnostics)
			|| a != b || x < 0
			|| y < 0 || x > len(&first.diagnostics) - a
			|| y > len(&second.diagnostics) - b
			|| first.after_declarations_transformers != second.after_declarations_transformers;
	let index = |value: i64| usize::try_from(value).unwrap_or(0);
	if incompatible || a != 0 && first.diagnostics[..index(a)] != second.diagnostics[..index(b)] {
		return (
			Err(Error::message(
				"transformer workers returned incompatible batch diagnostics or declaration stages",
			)),
			1,
		);
	}
	let (a, b, x, y) = (index(a), index(b), index(x), index(y));
	let mut merged = first.clone();
	merged.parallel_batch_diagnostics = 0;
	merged.parallel_source_diagnostics = 0;
	// Shared factory and config diagnostics come once, before the ordered per-file ones; serial
	// collects every missing-source diagnostic before transforming.
	merged.diagnostics = first.diagnostics[..a + x].to_vec();
	merged.diagnostics.extend_from_slice(&second.diagnostics[b..b + y]);
	merged.diagnostics.extend_from_slice(&first.diagnostics[a + x..]);
	merged.diagnostics.extend_from_slice(&second.diagnostics[b + y..]);
	merged.transformed.clone_from(&first.transformed);
	merged.transformed.extend_from_slice(&second.transformed);
	merged.metrics = merge_parallel_metrics(first.metrics.as_ref(), second.metrics.as_ref(), wall);
	(Ok(merged), 1)
}

/// `parallelInternalFailure`: the worker's in-band internal error.
pub(crate) fn parallel_internal_failure(response: Option<&TransformerResponse>) -> bool {
	response.is_some_and(|response| {
		response.transformed.is_empty()
			&& response.diagnostics.len() == 1
			&& response.diagnostics[0].category == "error"
			&& response.diagnostics[0].code == "sidecar-internal"
	})
}

/// `mergeParallelMetrics`: summed CPU and plugin work, the fan-out's wall time; phases overlap
/// across workers and are left out.
pub(crate) fn merge_parallel_metrics(
	first: Option<&TransformerResponseMetrics>,
	second: Option<&TransformerResponseMetrics>,
	wall: Duration,
) -> Option<TransformerResponseMetrics> {
	let (first, second) = (first?, second?);
	let mut plugins: Vec<TransformerPluginMetric> = first.plugins.clone();
	if first.plugins.len() == second.plugins.len() {
		for (plugin, other) in plugins.iter_mut().zip(&second.plugins) {
			if plugin.transform != other.transform {
				plugins = Vec::new();
				break;
			}
			plugin.ms += other.ms;
		}
	} else {
		plugins = Vec::new();
	}
	Some(TransformerResponseMetrics {
		wall_ms: i64::try_from(wall.as_millis()).unwrap_or(i64::MAX),
		cpu_user_us: first.cpu_user_us + second.cpu_user_us,
		cpu_system_us: first.cpu_system_us + second.cpu_system_us,
		node_version: first.node_version.clone(),
		plugins,
		phases: None,
	})
}
