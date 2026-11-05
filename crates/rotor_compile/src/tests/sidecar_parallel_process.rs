//! `sidecar_parallel_process_test.go`: the parallel split over real Node workers.

use std::time::Duration;

use rotor_config::config::{ParallelTransformerPlugin, ParallelTransformersConfig};
use rotor_support::logservice;

use super::sidecar::{PREFIX_STRING_PLUGIN, repo_sidecar_dir, round_trip};
use super::{Temp, write_project};
use crate::compile::Error;
use crate::sidecar::{SidecarCallStats, SidecarSession, sidecar_env, sidecar_slot_for, spawn_sidecar_session_mode};
use crate::sidecar_parallel::{ParallelOutcome, run_parallel_transformer_with_spawner};
use crate::transformer_host::{TransformerChangedFile, TransformerRequest};

/// `parallelFixtureIdentity`: the TypeScript version and the plugin's package digest.
fn parallel_fixture_identity(sidecar_dir: &str, dir: &str, entry: &str) -> (String, String) {
	let mut command = std::process::Command::new("node");
	command
		.args([
			"-e",
			"const path = require('node:path');\nconst [worker, project, entry] = process.argv.slice(1);\nconst ts = require(path.join(worker, 'index.js')).resolveTypeScript(project);\nconst digest = require(path.join(worker, 'lib/parallel.js')).packageDigest(entry);\nprocess.stdout.write(JSON.stringify({ version: ts.version, digest }));",
			sidecar_dir,
			dir,
			entry,
		])
		.current_dir(dir)
		.env_clear()
		.envs(sidecar_env(dir, sidecar_dir));
	let output = command
		.output()
		.unwrap_or_else(|error| panic!("fixture identity: {error}"));
	assert!(
		output.status.success(),
		"fixture identity: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let identity: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
	(
		identity["version"].as_str().unwrap_or_default().to_owned(),
		identity["digest"].as_str().unwrap_or_default().to_owned(),
	)
}

fn path(dir: &str, rel: &str) -> String {
	rotor_support::gopath::join(&[dir, rel])
}

#[test]
fn parallel_real_node_whole_response_matches_serial() {
	let sidecar_dir = repo_sidecar_dir();
	let temp = write_project("@scope/parallel-fixture", "");
	let dir = temp.dir();
	// Use the ordinary sidecar TypeScript fallback.
	for (name, text) in [
		("src/main.ts", r#"export const a = "first";"#),
		("src/second.ts", r#"export const b = "second";"#),
		("plugins/package.json", r#"{"name":"reviewed-parallel-fixture"}"#),
		("plugins/bad.js", "module.exports = {};"),
		("plugins/prefix-string.js", PREFIX_STRING_PLUGIN),
		(
			"tsconfig.json",
			r#"{"compilerOptions":{"noLib":true,"target":"ESNext","module":"CommonJS"},"include":["src"]}"#,
		),
	] {
		temp.write(name, text);
	}
	let plugins = vec![
		r#"{"transform":"./plugins/prefix-string.js","prefix":"parallel"}"#.to_owned(),
		r#"{"transform":"./plugins/bad.js"}"#.to_owned(),
	];
	let (version, digest) = parallel_fixture_identity(&sidecar_dir, &dir, &path(&dir, "plugins/prefix-string.js"));
	let profile = ParallelTransformersConfig {
		workers: 2,
		batch_independent: true,
		typescript: version,
		plugins: plugins
			.iter()
			.map(|plugin| ParallelTransformerPlugin {
				config: plugin.clone(),
				sha256: digest.clone(),
			})
			.collect(),
		..ParallelTransformersConfig::default()
	};
	let request = TransformerRequest {
		protocol: 1,
		operation: "transform".to_owned(),
		project_dir: dir.clone(),
		ts_config_path: path(&dir, "tsconfig.json"),
		plugins,
		compile_file_names: Some(vec![
			path(&dir, "src/main.ts"),
			path(&dir, "src/missing-a.ts"),
			path(&dir, "src/second.ts"),
			path(&dir, "src/missing-b.ts"),
		]),
		changed_files: Some(vec![TransformerChangedFile {
			file_name: path(&dir, "src/second.ts"),
			text: r#"export const b = "overlaid";"#.to_owned(),
			rebase: None,
		}]),
		..TransformerRequest::default()
	};
	let mut serial = spawn_sidecar_session_mode(&dir, &sidecar_dir, true).unwrap_or_else(|error| panic!("{error}"));
	let baseline = round_trip(&mut serial, &request, Duration::from_secs(30)).unwrap_or_else(|error| panic!("{error}"));
	serial.close_and_wait().unwrap_or_else(|error| panic!("{error}"));
	let verbose = logservice::verbose();
	logservice::set_verbose(false);
	logservice::capture();
	serial.drain_stderr();
	let baseline_stderr = logservice::take_capture();
	logservice::capture();
	let first = spawn_sidecar_session_mode(&dir, &sidecar_dir, true).unwrap_or_else(|error| panic!("{error}"));
	let spawn = |dir: &str, worker: &str, owned: bool| spawn_sidecar_session_mode(dir, worker, owned);
	let slot = sidecar_slot_for("whole-response");
	let mut guard = slot.acquire();
	guard.session = Some(first);
	let outcome = run_parallel_transformer_with_spawner(
		None,
		Duration::from_secs(30),
		&sidecar_dir,
		&mut guard,
		&request,
		Some(&profile),
		&mut SidecarCallStats::default(),
		&spawn,
	);
	let forwarded = logservice::take_capture();
	logservice::set_verbose(verbose);
	let ParallelOutcome::Handled(Ok(response)) = outcome else {
		panic!("parallel: not handled or failed");
	};
	let mut baseline = baseline;
	let mut response = *response;
	baseline.metrics = None;
	response.metrics = None;
	baseline.transport = None;
	response.transport = None;
	assert!(
		baseline.diagnostics.len() == 3
			&& baseline.transformed.len() == 2
			&& baseline == response
			&& baseline_stderr == forwarded,
		"whole serial/parallel response or forwarded stderr differs:\n{baseline:?}\n{response:?}"
	);
	// Both workers were reaped: the split leaves no session in the slot.
	assert!(guard.session.is_none(), "worker was not successfully waited/reaped");
}

const CLEANUP_SCRIPT: &str = r"const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const modeFile=path.join(process.cwd(),'cleanup-mode');
const mode=fs.existsSync(modeFile) ? fs.readFileSync(modeFile,'utf8') : '';
function holdStderr() {
 const child=require('node:child_process').spawn(process.execPath,['-e','setTimeout(()=>{},60000)'],{detached:true,windowsHide:true,stdio:['ignore','ignore',2]});
 fs.writeFileSync(path.join(process.cwd(),'descendant.pid'),String(child.pid));
 child.unref();
}
const rl = readline.createInterface({input:process.stdin});
rl.on('line', line => {
 const request=JSON.parse(line);
 if (request.operation==='parallel-preflight') {
  if (mode.startsWith('preflight-timeout')) {
   if (mode.endsWith('inherited-stderr')) holdStderr();
   setInterval(()=>{},60000);
   return;
  }
  process.stdout.write(JSON.stringify({parallelReady:!mode.startsWith('negative-preflight'),diagnostics:[]})+'\n');
 } else {
  fs.appendFileSync(path.join(process.cwd(),'transform-dispatch'),'transform\n');
  process.stdout.write(JSON.stringify({diagnostics:[],transformed:[]})+'\n');
 }
});
rl.on('close', () => {
 if (!mode || mode.startsWith('preflight-timeout')) return;
 if (mode.endsWith('abnormal-exit')) process.exit(23);
 holdStderr();
});
";

/// Kills a descendant a mode left holding stderr.
fn kill_descendant(dir: &str) {
	if let Ok(pid) = std::fs::read_to_string(path(dir, "descendant.pid")) {
		let _ = std::process::Command::new("kill").arg(pid.trim()).status();
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test's mode table")]
fn parallel_real_process_cleanup_cannot_be_green() {
	for mode in [
		"inherited-stderr",
		"abnormal-exit",
		"negative-preflight-inherited-stderr",
		"negative-preflight-abnormal-exit",
		"preflight-timeout",
		"preflight-timeout-inherited-stderr",
	] {
		let (dir_temp, other_temp, worker_temp) = (Temp::new(), Temp::new(), Temp::new());
		let (dir, other, worker_dir) = (dir_temp.dir(), other_temp.dir(), worker_temp.dir());
		worker_temp.write("main.js", CLEANUP_SCRIPT);
		let negative = mode.starts_with("negative-preflight");
		let timed_out = mode.starts_with("preflight-timeout");
		let mode_dir = if negative || timed_out {
			other.clone()
		} else {
			dir.clone()
		};
		std::fs::write(path(&mode_dir, "cleanup-mode"), mode).expect("write");
		let first = spawn_sidecar_session_mode(&dir, &worker_dir, true).unwrap_or_else(|error| panic!("{error}"));
		let second_spawned = std::sync::atomic::AtomicBool::new(false);
		let spawn = |_: &str, _: &str, _: bool| -> Result<SidecarSession, Error> {
			second_spawned.store(true, std::sync::atomic::Ordering::Relaxed);
			spawn_sidecar_session_mode(&other, &worker_dir, true)
		};
		let slot = sidecar_slot_for(mode);
		let mut guard = slot.acquire();
		guard.session = Some(first);
		let timeout = if timed_out {
			Duration::from_secs(2)
		} else {
			Duration::from_secs(10)
		};
		let outcome = run_parallel_transformer_with_spawner(
			None,
			timeout,
			&worker_dir,
			&mut guard,
			&TransformerRequest {
				compile_file_names: Some(vec!["a".to_owned(), "b".to_owned()]),
				..TransformerRequest::default()
			},
			None,
			&mut SidecarCallStats::default(),
			&spawn,
		);
		kill_descendant(&mode_dir);
		if mode == "preflight-timeout" {
			assert!(
				matches!(outcome, ParallelOutcome::Declined)
					&& guard.session.is_some()
					&& second_spawned.load(std::sync::atomic::Ordering::Relaxed),
				"{mode}: expected auxiliary kill lost serial fallback"
			);
			let mut first = guard.session.take().expect("worker 0");
			let request = TransformerRequest {
				operation: "transform".to_owned(),
				..TransformerRequest::default()
			};
			if let Err(error) = round_trip(&mut first, &request, Duration::from_secs(10)) {
				panic!("{mode}: fresh serial request: {error}");
			}
			first.close_and_wait().unwrap_or_else(|error| panic!("{mode}: {error}"));
			assert_eq!(
				std::fs::read_to_string(path(&dir, "transform-dispatch"))
					.ok()
					.as_deref(),
				Some("transform\n"),
				"{mode}: expected exactly one serial dispatch"
			);
			assert!(
				!std::path::Path::new(&path(&other, "transform-dispatch")).exists(),
				"{mode}: auxiliary transformed after timeout"
			);
			continue;
		}
		let error = match outcome {
			ParallelOutcome::Handled(Err(error)) => error.to_string(),
			ParallelOutcome::Handled(Ok(_)) | ParallelOutcome::PreflightFailed | ParallelOutcome::Declined => {
				panic!("{mode}: cleanup was green")
			}
		};
		if mode.ends_with("inherited-stderr") {
			assert!(
				error.contains("WaitDelay expired before I/O complete"),
				"{mode}: incomplete drain not surfaced: {error}"
			);
		}
		if mode.ends_with("abnormal-exit") {
			assert!(
				error.contains("exit status 23"),
				"{mode}: independent abnormal exit not surfaced: {error}"
			);
		}
		assert!(
			guard.session.is_none(),
			"{mode}: failed cleanup retained primary session"
		);
		if negative || timed_out {
			for directory in [&dir, &other] {
				assert!(
					!std::path::Path::new(&path(directory, "transform-dispatch")).exists(),
					"{mode}: transform dispatched after preflight cleanup failure"
				);
			}
		}
	}
}
