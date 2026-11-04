//! `sidecar_parallel_test.go`. Go scripts a session over in-memory pipes; the port's sessions
//! own a child process, so a scripted session is a tiny Node worker that records each request,
//! answers from a script and exits (transport EOF) or hangs when the script runs out.

use std::sync::Arc;
use std::time::Duration;

use rotor_config::config::{Config, ParallelTransformerPlugin, ParallelTransformersConfig};
use rotor_support::logservice;

use super::Temp;
use crate::compile::Error;
use crate::project::{Cancellation, ProjectOptions, project_options_for_referenced_config};
use crate::sidecar::{
	SIDECAR_STDERR_LINE_LIMIT, SIDECAR_STDERR_TRUNCATED, SidecarCallStats, SidecarSession, SidecarStderrTail,
	sidecar_slot_for, spawn_sidecar_session,
};
use crate::sidecar_parallel::{
	ParallelOutcome, mark_one_shot_cli_transformer_build, merge_parallel_responses, parallel_config_supported,
	parallel_request_supported, parallel_resolution_scope, run_parallel_transformer_with_spawner,
	split_parallel_request,
};
use crate::transformer_host::{
	TransformerCall, TransformerChangedFile, TransformerDiagnostic, TransformerOutputFile, TransformerPluginMetric,
	TransformerRequest, TransformerResponse, TransformerResponseMetrics,
};

fn diagnostic(category: &str, code: &str, file: &str, message: &str) -> TransformerDiagnostic {
	TransformerDiagnostic {
		category: category.to_owned(),
		code: code.to_owned(),
		file: file.to_owned(),
		message: message.to_owned(),
		..TransformerDiagnostic::default()
	}
}

fn output(file_name: &str, text: &str, trace_map: &str) -> TransformerOutputFile {
	TransformerOutputFile {
		file_name: file_name.to_owned(),
		text: text.to_owned(),
		trace_map: trace_map.to_owned(),
	}
}

fn strings(values: &[&str]) -> Vec<String> {
	values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn parallel_split_preserves_request() {
	let request = TransformerRequest {
		protocol: 1,
		operation: "transform".to_owned(),
		project_dir: "project".to_owned(),
		ts_config_path: "config".to_owned(),
		compile_file_names: Some(strings(&["a", "b", "c", "d", "e"])),
		root_file_names: strings(&["all", "roots"]),
		changed_files: Some(vec![TransformerChangedFile {
			file_name: "c".to_owned(),
			text: "overlay".to_owned(),
			rebase: None,
		}]),
		plugins: strings(&[r#"{"transform":"example","silent":true}"#]),
		..TransformerRequest::default()
	};
	let workers = split_parallel_request(&request);
	assert_eq!(workers[0].compile_file_names, Some(strings(&["a", "b", "c"])));
	assert_eq!(workers[1].compile_file_names, Some(strings(&["d", "e"])));
	for mut worker in workers {
		worker.compile_file_names.clone_from(&request.compile_file_names);
		worker.parallel_worker = false;
		assert_eq!(worker, request, "request changed");
	}
}

#[test]
fn parallel_merge_order_and_batch_diagnostics() {
	let common = diagnostic("error", "transformer-not-found", "", "factory failed");
	let one = diagnostic("", "", "a", "first");
	let two = diagnostic("", "", "b", "second");
	let first = TransformerResponse {
		parallel_batch_diagnostics: 1,
		diagnostics: vec![common.clone(), one.clone()],
		transformed: vec![output("a", "A", "trace-a")],
		after_declarations_transformers: 1,
		metrics: Some(TransformerResponseMetrics {
			wall_ms: 500,
			cpu_user_us: 100,
			plugins: vec![TransformerPluginMetric {
				transform: "example".to_owned(),
				ms: 20,
			}],
			..TransformerResponseMetrics::default()
		}),
		..TransformerResponse::default()
	};
	let mut second = TransformerResponse {
		parallel_batch_diagnostics: 1,
		diagnostics: vec![common.clone(), two.clone()],
		transformed: vec![output("b", "B", "trace-b")],
		after_declarations_transformers: 1,
		metrics: Some(TransformerResponseMetrics {
			wall_ms: 400,
			cpu_user_us: 120,
			plugins: vec![TransformerPluginMetric {
				transform: "example".to_owned(),
				ms: 30,
			}],
			..TransformerResponseMetrics::default()
		}),
		..TransformerResponse::default()
	};
	let (merged, last) = merge_parallel_responses(&first, &second, Duration::from_millis(550));
	let merged = merged.unwrap_or_else(|error| panic!("{last}: {error}"));
	assert_eq!(last, 1);
	assert_eq!(merged.diagnostics, [common, one, two]);
	let mut transformed = first.transformed.clone();
	transformed.extend(second.transformed.clone());
	assert_eq!(merged.transformed, transformed);
	let metrics = merged.metrics.expect("merged metrics");
	assert!(
		metrics.wall_ms == 550 && metrics.cpu_user_us == 220 && metrics.plugins[0].ms == 50 && metrics.phases.is_none(),
		"{metrics:?}"
	);
	"different state".clone_into(&mut second.diagnostics[0].message);
	assert!(
		merge_parallel_responses(&first, &second, Duration::from_secs(1))
			.0
			.is_err(),
		"accepted incompatible factory state"
	);
}

#[test]
fn parallel_merge_internal_failure_discards_all_outputs() {
	let success = TransformerResponse {
		transformed: vec![output("a", "", "")],
		..TransformerResponse::default()
	};
	let failure = TransformerResponse {
		diagnostics: vec![diagnostic("error", "sidecar-internal", "", "exact stack\r\n")],
		..TransformerResponse::default()
	};
	for (first, second, want) in [(&failure, &success, 0), (&success, &failure, 1)] {
		let (merged, last) = merge_parallel_responses(first, second, Duration::from_secs(1));
		let merged = merged.unwrap_or_else(|error| panic!("{error}"));
		assert!(
			last == want && merged.diagnostics == failure.diagnostics && merged.transformed.is_empty(),
			"{merged:?}, {last}"
		);
	}
}

#[test]
fn parallel_merge_preserves_serial_diagnostic_phases() {
	let common = diagnostic("", "transformer-not-found", "", "factory");
	let missing0 = diagnostic("", "source-file-missing", "a", "");
	let missing1 = diagnostic("", "source-file-missing", "b", "");
	let transform0 = diagnostic("", "transform", "a", "");
	let transform1 = diagnostic("", "transform", "b", "");
	let response = |diagnostics: Vec<TransformerDiagnostic>| TransformerResponse {
		parallel_batch_diagnostics: 1,
		parallel_source_diagnostics: 1,
		diagnostics,
		..TransformerResponse::default()
	};
	let first = response(vec![common.clone(), missing0.clone(), transform0.clone()]);
	let second = response(vec![common.clone(), missing1.clone(), transform1.clone()]);
	let merged = merge_parallel_responses(&first, &second, Duration::from_secs(1))
		.0
		.unwrap_or_else(|error| panic!("{error}"));
	assert_eq!(merged.diagnostics, [common, missing0, missing1, transform0, transform1]);
}

#[test]
fn parallel_stderr_uses_serial_line_forwarding() {
	logservice::capture();
	logservice::write("start\n");
	let empty = SidecarStderrTail::spawn(std::io::Cursor::new(Vec::new()));
	empty.wait_done(None);
	empty.drain_to();
	logservice::write_line(&["next"]);
	let output = logservice::take_capture();
	assert_eq!(output, "start\nnext\n", "empty stderr changed log state");
	logservice::capture();
	let payload = format!("{}\r\nunterminated", "x".repeat(SIDECAR_STDERR_LINE_LIMIT + 1));
	let stderr = SidecarStderrTail::spawn(std::io::Cursor::new(payload.into_bytes()));
	stderr.wait_done(None);
	stderr.drain_to();
	let want = format!(
		"{}{SIDECAR_STDERR_TRUNCATED}\nunterminated\n",
		"x".repeat(SIDECAR_STDERR_LINE_LIMIT)
	);
	assert!(
		logservice::take_capture() == want,
		"stderr differs from serial line forwarding"
	);
}

fn canonical_temp(temp: &Temp) -> String {
	rotor_support::fsutil::eval_symlinks(&temp.dir()).expect("the canonical temp dir")
}

fn join(dir: &str, parts: &[&str]) -> String {
	let mut all = vec![dir];
	all.extend_from_slice(parts);
	rotor_support::gopath::join(&all)
}

#[test]
fn parallel_resolution_rejects_source_aliases_and_node_modules() {
	let temp = Temp::new();
	let root = canonical_temp(&temp);
	let source = join(&root, &["a.ts"]);
	std::fs::write(&source, "export {};").expect("write");
	assert!(
		!parallel_resolution_scope(&root, &[join(&root, &["node_modules", "plugin", "a.ts"])]),
		"accepted dependency scope"
	);
	assert!(
		!parallel_resolution_scope(&root, &[join(&root, &["missing", "a.ts"])]),
		"accepted unknown directory"
	);
	assert!(
		!parallel_resolution_scope(&root, &[join(&root, &["missing.ts"])]),
		"accepted missing source"
	);
	#[cfg(unix)]
	{
		let alias = join(&root, &["alias.ts"]);
		std::os::unix::fs::symlink(&source, &alias).expect("symlink");
		assert!(
			!parallel_resolution_scope(&root, &[source, alias]),
			"accepted source alias"
		);
	}
}

fn reviewed_profile() -> ParallelTransformersConfig {
	ParallelTransformersConfig {
		workers: 2,
		batch_independent: true,
		typescript: "reviewed".to_owned(),
		plugins: vec![ParallelTransformerPlugin {
			config: r#"{"transform":"example"}"#.to_owned(),
			..ParallelTransformerPlugin::default()
		}],
		..ParallelTransformersConfig::default()
	}
}

#[test]
fn parallel_admission_default_off_and_resolution_boundary() {
	let temp = Temp::new();
	let root = canonical_temp(&temp);
	let src = join(&root, &["src", "nested"]);
	std::fs::create_dir_all(&src).expect("mkdir");
	let files = vec![join(&src, &["a.ts"]), join(&src, &["b.ts"])];
	for file in &files {
		std::fs::write(file, "export {};").expect("write");
	}
	let mut request = TransformerRequest {
		project_dir: root.clone(),
		compile_file_names: Some(files.clone()),
		plugins: strings(&[r#"{"transform":"example"}"#]),
		..TransformerRequest::default()
	};
	let mut call = TransformerCall {
		dir: root.clone(),
		..TransformerCall::default()
	};
	assert!(
		!parallel_request_supported(&call, &request) && !parallel_config_supported(&Config::default()),
		"default enabled"
	);
	call.parallel = Some(reviewed_profile());
	assert!(
		parallel_request_supported(&call, &request),
		"rejected simple authored scope"
	);
	request.root_file_names = vec![files[0].clone()];
	assert!(!parallel_request_supported(&call, &request), "accepted narrowed roots");
	request.root_file_names.clear();
	request.changed_files = Some(vec![TransformerChangedFile {
		file_name: join(&root, &["tsconfig.json"]),
		text: "{}".to_owned(),
		rebase: None,
	}]);
	assert!(!parallel_request_supported(&call, &request), "accepted config overlay");
	request.changed_files = Some(vec![TransformerChangedFile {
		file_name: files[1].clone(),
		text: "export const overlay = true".to_owned(),
		rebase: None,
	}]);
	assert!(parallel_request_supported(&call, &request), "rejected authored overlay");
	std::fs::write(join(&src, &["package.json"]), "{}").expect("write");
	assert!(
		!parallel_request_supported(&call, &request),
		"accepted nested package boundary"
	);
	if let Some(names) = &mut request.compile_file_names {
		names[0] = join(&rotor_support::gopath::dir(&root), &["outside.ts"]);
	}
	assert!(!parallel_request_supported(&call, &request), "accepted outside source");
	let config = Config {
		parallel_transformers: Some(reviewed_profile()),
		warnings: vec![r#"rotor.toml: unknown key "parallelTransformers.future" (ignored)"#.to_owned()],
		..Config::default()
	};
	assert!(
		!parallel_config_supported(&config),
		"accepted unknown attestation semantics"
	);
}

#[test]
fn parallel_scope_rejects_plain_nested_node_modules() {
	let temp = Temp::new();
	let root = canonical_temp(&temp);
	let x = join(&root, &["src", "a", "x.ts"]);
	let y = join(&root, &["src", "b", "y.ts"]);
	for file in [&x, &y] {
		std::fs::create_dir_all(rotor_support::gopath::dir(file)).expect("mkdir");
		std::fs::write(file, "export {};").expect("write");
	}
	std::fs::create_dir(join(&root, &["node_modules"])).expect("mkdir");
	assert!(
		parallel_resolution_scope(&root, &[x.clone(), y.clone()]),
		"root dependency scope rejected"
	);
	std::fs::create_dir(join(&rotor_support::gopath::dir(&x), &["NoDe_MoDuLeS"])).expect("mkdir");
	assert!(
		!parallel_resolution_scope(&root, &[x, y]),
		"plain nested dependency directory admitted"
	);
}

#[test]
fn parallel_internal_code_requires_serial_catch_shape() {
	let first = TransformerResponse {
		diagnostics: vec![diagnostic("error", "sidecar-internal", "", "")],
		transformed: vec![output("a", "", "")],
		..TransformerResponse::default()
	};
	let second = TransformerResponse {
		transformed: vec![output("b", "", "")],
		..TransformerResponse::default()
	};
	let (merged, last) = merge_parallel_responses(&first, &second, Duration::from_secs(1));
	assert!(
		merged.is_ok_and(|merged| last == 1 && merged.transformed.len() == 2),
		"arbitrary diagnostic code treated as a serial catch"
	);
}

/// The one-shot CLI split token: the port has no profiling labels or prefix context, so the
/// shared provenance and token reuse are what carry over from Go's prefix gate.
#[test]
fn parallel_prefix_gate_preserves_shared_token() {
	let temp = Temp::new();
	let mut opts = ProjectOptions::default();
	assert!(
		opts.cli_transformer_build.is_none(),
		"unmarked/API/watch build admitted"
	);
	mark_one_shot_cli_transformer_build(&mut opts);
	let config_path = temp.join("tsconfig.json").to_string_lossy().into_owned();
	std::fs::write(&config_path, "{}").expect("write");
	let referenced = project_options_for_referenced_config(&opts, &config_path, false)
		.unwrap_or_else(|error| panic!("solution option copy: {error}"));
	let (Some(build), Some(referenced_build)) = (&opts.cli_transformer_build, &referenced.cli_transformer_build) else {
		panic!("solution option copy lost shared provenance");
	};
	assert!(
		Arc::ptr_eq(build, referenced_build),
		"solution option copy lost shared provenance"
	);
	assert!(
		build.try_split() && !referenced_build.try_split(),
		"concurrent split token admitted"
	);
	build.release_split();
	assert!(
		referenced_build.try_split(),
		"released token not reusable by another project"
	);
	referenced_build.release_split();
}

/// A scripted worker: each request is appended to `requests.log` (and its pid written first),
/// each answer comes from `responses`, and when they run out the worker exits (`hang` false,
/// transport EOF) or keeps stdin open forever.
struct Scripted {
	_dir: Temp,
	sidecar_dir: String,
}

impl Scripted {
	fn new(ready: bool, response: Option<&str>, stderr: &str, hang: bool) -> Self {
		let mut lines = vec![format!(
			r#"{{"diagnostics":[],"transformed":[],"parallelReady":{ready}}}"#
		)];
		lines.extend(response.map(str::to_owned));
		Self::with_lines(&lines, stderr, hang)
	}

	fn with_lines(lines: &[String], stderr: &str, hang: bool) -> Self {
		let dir = Temp::new();
		let script = format!(
			r#"const fs = require("fs");
const path = require("path");
const log = path.join(__dirname, "requests.log");
fs.writeFileSync(path.join(__dirname, "pid"), String(process.pid));
const responses = {responses};
const hang = {hang};
process.stderr.write({stderr});
let buffer = "";
process.stdin.on("data", (chunk) => {{
	buffer += chunk;
	let index;
	while ((index = buffer.indexOf("\n")) >= 0) {{
		const line = buffer.slice(0, index);
		buffer = buffer.slice(index + 1);
		fs.appendFileSync(log, line + "\n");
		if (responses.length === 0) {{
			if (!hang) process.exit(0);
			continue;
		}}
		process.stdout.write(responses.shift() + "\n");
	}}
}});
process.stdin.on("end", () => process.exit(0));
"#,
			responses = serde_json::to_string(lines).expect("JSON"),
			hang = hang,
			stderr = serde_json::to_string(stderr).expect("JSON"),
		);
		dir.write("main.js", script);
		let sidecar_dir = dir.dir();
		Self { _dir: dir, sidecar_dir }
	}

	fn spawn(&self) -> SidecarSession {
		let session =
			spawn_sidecar_session(&self.sidecar_dir, &self.sidecar_dir).unwrap_or_else(|error| panic!("{error}"));
		// The worker writes its pid before it reads a request.
		let pid_path = std::path::Path::new(&self.sidecar_dir).join("pid");
		let deadline = std::time::Instant::now() + Duration::from_secs(10);
		while !pid_path.exists() && std::time::Instant::now() < deadline {
			std::thread::sleep(Duration::from_millis(2));
		}
		session
	}

	fn requests(&self) -> Vec<TransformerRequest> {
		std::fs::read_to_string(std::path::Path::new(&self.sidecar_dir).join("requests.log"))
			.unwrap_or_default()
			.lines()
			.map(request_from_json)
			.collect()
	}

	/// Go's `input.closed`: the worker process is gone.
	fn closed(&self) -> bool {
		let pid = std::fs::read_to_string(std::path::Path::new(&self.sidecar_dir).join("pid")).unwrap_or_default();
		!std::process::Command::new("kill")
			.args(["-0", pid.trim()])
			.stderr(std::process::Stdio::null())
			.status()
			.is_ok_and(|status| status.success())
	}
}

/// The fields of a recorded request the tests read.
fn request_from_json(line: &str) -> TransformerRequest {
	let value: serde_json::Value = serde_json::from_str(line).expect("a recorded request");
	let changed_files = value["changedFiles"].as_array().map(|files| {
		files
			.iter()
			.map(|file| TransformerChangedFile {
				file_name: file["fileName"].as_str().unwrap_or_default().to_owned(),
				text: file["text"].as_str().unwrap_or_default().to_owned(),
				rebase: None,
			})
			.collect()
	});
	TransformerRequest {
		operation: value["operation"].as_str().unwrap_or_default().to_owned(),
		changed_files,
		..TransformerRequest::default()
	}
}

type Outcome = (bool, Option<Error>);

fn outcome(outcome: ParallelOutcome) -> Outcome {
	match outcome {
		ParallelOutcome::Handled(result) => (true, result.err()),
		ParallelOutcome::PreflightFailed => (false, Some(Error::message("preflight failed"))),
		ParallelOutcome::Declined => (false, None),
	}
}

/// Runs a split over `first` in a fresh slot; `spawn` supplies worker 1.
fn run(
	key: &str,
	first: SidecarSession,
	request: &TransformerRequest,
	timeout: Duration,
	cancel: Option<&Cancellation>,
	stats: &mut SidecarCallStats,
	spawn: &dyn Fn(&str, &str, bool) -> Result<SidecarSession, Error>,
) -> (Outcome, bool) {
	let slot = sidecar_slot_for(key);
	let mut guard = slot.acquire();
	guard.session = Some(first);
	let result = outcome(run_parallel_transformer_with_spawner(
		cancel, timeout, "", &mut guard, request, None, stats, spawn,
	));
	let kept = guard.session.is_some();
	if let Some(mut session) = guard.session.take() {
		session.close();
	}
	(result, kept)
}

#[test]
fn parallel_preflight_mismatch_keeps_serial_session() {
	let first = Scripted::new(false, None, "", true);
	let spawn =
		|_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> { panic!("spawned before attestation matched") };
	let ((handled, error), kept) = run(
		"mismatch",
		first.spawn(),
		&TransformerRequest::default(),
		Duration::from_secs(1),
		None,
		&mut SidecarCallStats::default(),
		&spawn,
	);
	let requests = first.requests();
	assert!(
		error.is_none() && !handled && kept && requests.len() == 1 && requests[0].operation == "parallel-preflight",
		"handled={handled} err={error:?} requests={requests:?}"
	);
}

#[test]
fn parallel_scripted_close_and_stderr_order() {
	for fail_first in [false, true] {
		let response = if fail_first {
			r#"{"diagnostics":[{"category":"error","code":"sidecar-internal","message":"exact"}],"transformed":[]}"#
		} else {
			r#"{"diagnostics":[],"transformed":[]}"#
		};
		let first = Scripted::new(true, Some(response), "first\r\npartial", false);
		let second = Scripted::new(true, Some("{}"), "second\n", false);
		let spawns = std::sync::atomic::AtomicUsize::new(0);
		let spawn = |_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> {
			spawns.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
			Ok(second.spawn())
		};
		let request = TransformerRequest {
			compile_file_names: Some(strings(&["a", "b"])),
			changed_files: Some(vec![TransformerChangedFile {
				file_name: "b".to_owned(),
				text: "overlay".to_owned(),
				rebase: None,
			}]),
			..TransformerRequest::default()
		};
		let mut stats = SidecarCallStats::default();
		let first_session = first.spawn();
		// Both workers' stderr has to land before the split drains it.
		std::thread::sleep(Duration::from_millis(200));
		logservice::capture();
		let ((handled, error), kept) = run(
			"close-order",
			first_session,
			&request,
			Duration::from_secs(10),
			None,
			&mut stats,
			&spawn,
		);
		let output = logservice::take_capture();
		assert!(
			error.is_none()
				&& handled && spawns.load(std::sync::atomic::Ordering::Relaxed) == 1
				&& !kept && first.closed()
				&& second.closed(),
			"fail_first {fail_first}: {handled} {error:?}"
		);
		assert_eq!(stats.additional_spawns, 1, "second worker missing from spawn metrics");
		let want = if fail_first {
			"first\npartial\n"
		} else {
			"first\npartial\nsecond\n"
		};
		assert_eq!(output, want, "fail_first {fail_first}: stderr");
		for scripted in [&first, &second] {
			let requests = scripted.requests();
			assert!(
				requests.len() == 2 && requests[1].changed_files == request.changed_files,
				"{requests:?}"
			);
		}
	}
}

#[test]
fn parallel_transport_failure_never_retries_serial() {
	let first = Scripted::new(true, None, "", false);
	let second = Scripted::new(true, None, "", false);
	let spawns = std::sync::atomic::AtomicUsize::new(0);
	let spawn = |_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> {
		spawns.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		Ok(second.spawn())
	};
	let request = TransformerRequest {
		compile_file_names: Some(strings(&["a", "b"])),
		..TransformerRequest::default()
	};
	let ((handled, error), kept) = run(
		"transport",
		first.spawn(),
		&request,
		Duration::from_secs(10),
		None,
		&mut SidecarCallStats::default(),
		&spawn,
	);
	assert!(
		handled
			&& error.is_some()
			&& spawns.load(std::sync::atomic::Ordering::Relaxed) == 1
			&& first.closed()
			&& second.closed()
			&& !kept && first.requests().len() <= 2
			&& second.requests().len() <= 2,
		"handled={handled} error={error:?}"
	);
}

#[test]
fn parallel_cancellation_closes_both_workers() {
	let first = Scripted::new(true, None, "", true);
	let second = Scripted::new(true, None, "", true);
	let cancel = Cancellation::default();
	// Go cancels as worker 0's transform request is written; the port cancels once it is
	// recorded.
	let watcher = {
		let cancel = cancel.clone();
		let log = std::path::Path::new(&first.sidecar_dir).join("requests.log");
		std::thread::spawn(move || {
			let deadline = std::time::Instant::now() + Duration::from_secs(10);
			while std::time::Instant::now() < deadline {
				if std::fs::read_to_string(&log).is_ok_and(|text| text.contains(r#""operation":"transform""#)) {
					break;
				}
				std::thread::sleep(Duration::from_millis(2));
			}
			cancel.cancel();
		})
	};
	let spawn = |_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> { Ok(second.spawn()) };
	let request = TransformerRequest {
		operation: "transform".to_owned(),
		compile_file_names: Some(strings(&["a", "b"])),
		..TransformerRequest::default()
	};
	let ((handled, error), kept) = run(
		"cancel",
		first.spawn(),
		&request,
		Duration::from_secs(30),
		Some(&cancel),
		&mut SidecarCallStats::default(),
		&spawn,
	);
	watcher.join().expect("the watcher");
	let error = error.map(|error| error.to_string());
	assert!(
		handled
			&& error.as_deref().is_some_and(|error| error.contains("context canceled"))
			&& first.closed()
			&& second.closed()
			&& !kept,
		"{handled} {error:?}"
	);
}

#[test]
fn parallel_preflight_failure_keeps_bounded_serial_fallback() {
	// Transport EOF before the preflight answer.
	let first = Scripted::with_lines(&[], "", false);
	let spawn = |_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> {
		panic!("auxiliary spawned after primary failed")
	};
	let ((handled, error), kept) = run(
		"preflight-eof",
		first.spawn(),
		&TransformerRequest::default(),
		Duration::from_secs(10),
		None,
		&mut SidecarCallStats::default(),
		&spawn,
	);
	assert!(
		!handled && error.is_some() && !kept && first.closed() && first.requests().len() == 1,
		"handled={handled} err={error:?}"
	);
	// Auxiliary timeout must leave worker 0 untouched for a fresh ordinary transform.
	let first = Scripted::new(true, Some(r#"{"diagnostics":[],"transformed":[]}"#), "", true);
	let second = Scripted::with_lines(&[], "", true);
	let spawn = |_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> { Ok(second.spawn()) };
	let slot = sidecar_slot_for("aux-timeout");
	let mut guard = slot.acquire();
	guard.session = Some(first.spawn());
	let (handled, error) = outcome(run_parallel_transformer_with_spawner(
		None,
		Duration::from_millis(500),
		"",
		&mut guard,
		&TransformerRequest::default(),
		None,
		&mut SidecarCallStats::default(),
		&spawn,
	));
	assert!(
		!handled && error.is_none() && !first.closed() && guard.session.is_some() && second.closed(),
		"auxiliary timeout: {handled} {error:?}"
	);
	let mut session = guard.session.take().expect("worker 0");
	let request = TransformerRequest {
		operation: "transform".to_owned(),
		..TransformerRequest::default()
	};
	let response = super::sidecar::round_trip(&mut session, &request, Duration::from_secs(10));
	assert!(
		response.is_ok() && first.requests().len() == 2,
		"ordinary serial request failed: {:?}",
		response.err()
	);
	session.close();
}
