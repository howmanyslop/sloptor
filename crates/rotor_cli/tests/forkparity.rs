//! The fork-parity matrix and the runtime suites behind its upstream-corrected rows (the Rust port
//! of Go's `internal/forkparity` and of the Lune half of `internal/conformance`).
//!
//! - [`matrix`]: `sloptor` against the frozen goldens of the pinned roblox-ts fork
//!   (`testdata/forkparity`), the archive digest, and the divergence ledger.
//! - [`runtime`]: compiled output run in Lune through a Rojo place, against the pinned upstream
//!   compiler where a ledger row says the fork was wrong.
//!
//! The suites that build places or run sloptor over every fixture are ignored by default. Run them
//! with `mise run rust:forkparity`, which builds the release binary first and names it in
//! `ROTOR_BIN`. Without `ROTOR_BIN` they run the test profile's `sloptor`.

#![cfg(test)]

#[path = "forkparity/matrix.rs"]
mod matrix;
#[path = "forkparity/runtime.rs"]
mod runtime;

use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clap as _;
use clap_complete as _;
use ctrlc as _;
use icu_normalizer as _;
use jiff as _;
#[cfg(target_os = "macos")]
use libc as _;
use libmimalloc_sys2 as _;
use mimalloc_safe as _;
use notify as _;
use rotor_config as _;
use rotor_lsp as _;
use terminal_size as _;

/// The repository root.
fn repo_root() -> PathBuf {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	rotor_support::fsutil::real_path(&root).unwrap_or(root)
}

/// The `sloptor` the suites run: the file `ROTOR_BIN` names, else the test profile's binary.
fn sloptor() -> PathBuf {
	match std::env::var_os("ROTOR_BIN") {
		Some(path) if !path.is_empty() => {
			let path = PathBuf::from(path);
			assert!(path.is_file(), "ROTOR_BIN names no file: {}", path.display());
			path
		}
		_ => PathBuf::from(env!("CARGO_BIN_EXE_sloptor")),
	}
}

/// What one process produced.
#[derive(Debug, Default)]
struct Output {
	/// The exit code; -1 when the process was killed or died on a signal.
	code: i32,
	stdout: String,
	stderr: String,
}

impl Output {
	fn success(&self) -> bool {
		self.code == 0
	}

	/// Stdout then stderr, for failure messages.
	fn combined(&self) -> String {
		format!("{}{}", self.stdout, self.stderr)
	}
}

/// How long one compiler, Rojo or Lune process may run.
const PROCESS_TIMEOUT: Duration = Duration::from_mins(5);

/// Runs `command` to completion, or kills it after [`PROCESS_TIMEOUT`].
fn run(command: &mut Command) -> Output {
	let description = format!("{command:?}");
	let mut child = command
		.env("ROTOR_MAX_BUILDS", "0")
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap_or_else(|error| panic!("spawn {description}: {error}"));
	let read = |pipe: Option<Box<dyn Read + Send>>| {
		std::thread::spawn(move || {
			let mut bytes = Vec::new();
			if let Some(mut pipe) = pipe {
				let _ = pipe.read_to_end(&mut bytes);
			}
			String::from_utf8_lossy(&bytes).into_owned()
		})
	};
	let stdout = read(child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
	let stderr = read(child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
	let started = Instant::now();
	let status = loop {
		if let Some(status) = child.try_wait().expect("poll the child") {
			break Some(status);
		}
		if started.elapsed() > PROCESS_TIMEOUT {
			let _ = child.kill();
			let _ = child.wait();
			break None;
		}
		std::thread::sleep(Duration::from_millis(10));
	};
	let mut output = Output {
		code: status.and_then(|status| status.code()).unwrap_or(-1),
		stdout: stdout.join().unwrap_or_default(),
		stderr: stderr.join().unwrap_or_default(),
	};
	if status.is_none() {
		let _ = write!(output.stderr, "\n{description} timed out after {PROCESS_TIMEOUT:?}\n");
	}
	output
}

/// Runs `sloptor` in `dir`.
fn run_sloptor(dir: &Path, args: &[&str]) -> Output {
	run(Command::new(sloptor()).args(args).current_dir(dir).env("NO_COLOR", "1"))
}

fn write_file(path: &Path, contents: impl AsRef<[u8]>) {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
	}
	fs::write(path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

fn copy_file(src: &Path, dst: &Path) {
	if let Some(parent) = dst.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
	}
	fs::copy(src, dst).unwrap_or_else(|error| panic!("copy {} to {}: {error}", src.display(), dst.display()));
}

/// Copies the tree at `src` to `dst`, following symlinks, keeping the entries `keep` accepts (by
/// path relative to `src`).
fn copy_tree_filtered(src: &Path, dst: &Path, keep: &dyn Fn(&str) -> bool) {
	fn walk(root: &Path, dir: &Path, dst: &Path, keep: &dyn Fn(&str) -> bool) {
		fs::create_dir_all(dst).unwrap_or_else(|error| panic!("create {}: {error}", dst.display()));
		let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
		for entry in entries {
			let path = entry.expect("a directory entry").path();
			let relative = slash_relative(root, &path);
			if !keep(&relative) {
				continue;
			}
			let target = dst.join(path.file_name().expect("a file name"));
			if path.is_dir() {
				walk(root, &path, &target, keep);
			} else {
				copy_file(&path, &target);
			}
		}
	}
	walk(src, src, dst, keep);
}

fn copy_tree(src: &Path, dst: &Path) {
	copy_tree_filtered(src, dst, &|_| true);
}

/// `path` relative to `root`, slash-separated.
fn slash_relative(root: &Path, path: &Path) -> String {
	path.strip_prefix(root)
		.unwrap_or(path)
		.to_string_lossy()
		.replace('\\', "/")
}

/// Links the directory `link` to `target`.
fn symlink_dir(target: &Path, link: &Path) {
	if let Some(parent) = link.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
	}
	#[cfg(unix)]
	let linked = std::os::unix::fs::symlink(target, link);
	#[cfg(windows)]
	let linked = std::os::windows::fs::symlink_dir(target, link);
	linked.unwrap_or_else(|error| panic!("link {} to {}: {error}", link.display(), target.display()));
}

/// Every file under `root` by slash path relative to it, skipping directories named
/// `node_modules`. A missing `root` is an empty tree.
fn read_tree(root: &Path, keep: &dyn Fn(&str) -> bool) -> std::collections::BTreeMap<String, Vec<u8>> {
	fn walk(
		root: &Path,
		dir: &Path,
		keep: &dyn Fn(&str) -> bool,
		out: &mut std::collections::BTreeMap<String, Vec<u8>>,
	) {
		let Ok(entries) = fs::read_dir(dir) else {
			return;
		};
		for entry in entries {
			let entry = entry.expect("a directory entry");
			let path = entry.path();
			if entry.file_type().expect("a file type").is_dir() {
				if entry.file_name() != "node_modules" {
					walk(root, &path, keep, out);
				}
				continue;
			}
			let relative = slash_relative(root, &path);
			if keep(&relative) {
				let contents = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
				out.insert(relative, contents);
			}
		}
	}
	let mut out = std::collections::BTreeMap::new();
	walk(root, root, keep, &mut out);
	out
}

/// Whether the slash path `path` has the extension `extension` (compared exactly, as Go does).
fn has_extension(path: &str, extension: &str) -> bool {
	Path::new(path).extension().is_some_and(|found| found == extension)
}

/// The first line where `got` and `want` differ.
fn first_difference(got: &str, want: &str) -> String {
	let (got_lines, want_lines): (Vec<_>, Vec<_>) = (got.split('\n').collect(), want.split('\n').collect());
	for (index, (got_line, want_line)) in got_lines.iter().zip(&want_lines).enumerate() {
		if got_line != want_line {
			return format!("line {} got {got_line:?} want {want_line:?}", index + 1);
		}
	}
	format!("line count got {} want {}", got_lines.len(), want_lines.len())
}
