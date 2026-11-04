//! `sloptor lsp` (`cmd/rotor/lsp.go`): a stdio language server publishing the transformer
//! diagnostics `sloptor diagnostics` reports, over the editor's unsaved buffers.
//!
//! Censuses run one at a time (as Go's `censusMu` serializes them) on one long-lived large-stack
//! thread, each in its own program scope, so a session never accumulates programs and the
//! allocators never see a thread per census.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};

use rotor_compile::census::{ProjectDiagnostics, compile_project_diagnostics};
use rotor_support::{gopath, logservice};

use crate::app::Failure;
use crate::cli::LspCommand;
use crate::options::find_ts_config_path;
use crate::term::Streams;

/// One census: the tsconfig, the overlays, and where the answer goes.
type Job = (
	String,
	BTreeMap<String, String>,
	Sender<(Option<ProjectDiagnostics>, Option<String>)>,
);

/// `errNotRobloxTS`
const NOT_ROBLOX_TS: &str = "not a roblox-ts project";

/// `sloptor lsp`
///
/// # Errors
///
/// A transport failure.
pub fn run(_command: LspCommand, _streams: &mut Streams) -> Result<(), Failure> {
	rotor_tsrs::project::set_long_lived(true);
	// stdout carries the protocol, so everything else goes to stderr.
	let previous = logservice::set_output_stderr(true);
	let (jobs, receiver) = mpsc::channel::<Job>();
	let worker = std::thread::Builder::new()
		.name("rotor-lsp-census".to_owned())
		.stack_size(512 << 20)
		.spawn(move || {
			while let Ok((config, overlays, reply)) = receiver.recv() {
				let answer = census(&config, overlays);
				crate::memory::trim();
				let _ = reply.send(answer);
			}
		})
		.map_err(|error| Failure::Runtime(format!("lsp: start the census thread: {error}")))?;
	let jobs = Mutex::new(jobs);
	let options = rotor_lsp::Options {
		find_ts_config: Arc::new(|file_name: &str| {
			let ts_config_path = find_ts_config_path(&gopath::dir(file_name))?;
			if !has_rbxts_types(&gopath::dir(&ts_config_path)) {
				return Err(NOT_ROBLOX_TS.to_owned());
			}
			Ok(ts_config_path)
		}),
		census: Arc::new(move |config: &str, overlays: &BTreeMap<String, String>| {
			let (reply, answer) = mpsc::channel();
			let sent =
				jobs.lock()
					.unwrap_or_else(PoisonError::into_inner)
					.send((config.to_owned(), overlays.clone(), reply));
			if sent.is_err() {
				return (None, Some("lsp: the census thread stopped".to_owned()));
			}
			answer
				.recv()
				.unwrap_or_else(|_| (None, Some("lsp: the census thread stopped".to_owned())))
		}),
		debounce: std::time::Duration::ZERO,
	};
	let served = rotor_lsp::serve(
		&rotor_lsp::Cancel::default(),
		std::io::stdin(),
		std::io::stdout(),
		options,
	);
	logservice::set_output_stderr(previous);
	// The census thread ends with the server's last sender; it is not waited for.
	drop(worker);
	served.map_err(Failure::Runtime)
}

/// One census, admitted like `sloptor build`, in its own program scope.
fn census(config: &str, overlays: BTreeMap<String, String>) -> (Option<ProjectDiagnostics>, Option<String>) {
	let _lease = crate::runtime::admit(&mut |message| {
		let _ = std::io::Write::write_all(&mut std::io::stderr(), format!("{message}\n").as_bytes());
	});
	// SAFETY: the census returns owned data (paths, messages and offsets), its error is turned
	// into text here, and every thread that compiled for it is done before it returns, so nothing
	// from the scope's programs, or allocated in its region (the config reads included), is used
	// after the scope.
	#[expect(unsafe_code, reason = "each census frees its programs")]
	let _programs = unsafe { rotor_tsrs::project::ProgramScope::enter() };
	let mut options = match crate::diagnostics::compile_options(config) {
		Ok(options) => options,
		Err(error) => return (None, Some(error)),
	};
	options.overlays = overlays;
	// An editor's open buffers include files this project does not hold; they are not a failure.
	options.ignore_unmatched_overlays = true;
	let (result, error) = compile_project_diagnostics(&gopath::dir(config), &options);
	(Some(result), error.map(|error| error.to_string()))
}

/// `hasRbxtsTypes`: `node_modules/@rbxts` exists in `dir` or an ancestor (a monorepo may hoist
/// it). A plain TypeScript project has none, and the server leaves its files alone.
#[must_use]
pub fn has_rbxts_types(dir: &str) -> bool {
	let mut dir = dir.to_owned();
	loop {
		if std::fs::metadata(gopath::join(&[&dir, "node_modules", "@rbxts"])).is_ok_and(|metadata| metadata.is_dir()) {
			return true;
		}
		let parent = gopath::dir(&dir);
		if parent == dir {
			return false;
		}
		dir = parent;
	}
}

#[cfg(test)]
mod tests {
	//! `lsp_test.go`

	use super::has_rbxts_types;

	/// A census of a plain TypeScript project would pop "rootDir must be defined" in the editor,
	/// and a project with hoisted `node_modules` must still count.
	#[test]
	fn has_rbxts_types_beside_above_or_absent() {
		for (at, want) in [(Some("pkg"), true), (Some("."), true), (None, false)] {
			let root = tempfile::tempdir().expect("temp dir");
			let project = root.path().join("pkg");
			std::fs::create_dir_all(&project).expect("project");
			if let Some(at) = at {
				std::fs::create_dir_all(root.path().join(at).join("node_modules").join("@rbxts")).expect("@rbxts");
			}
			assert_eq!(has_rbxts_types(&project.to_string_lossy()), want, "{at:?}");
		}
	}
}
