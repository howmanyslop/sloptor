//! `internal/logservice`: the upstream compiler's stdout channel.
//!
//! The port of `LogService.ts` and `benchmark.ts`: partial-line tracking, a verbose gate, the
//! yellow `Compiler Warning:` prefix, and the verbose benchmark line format (`name ( N ms )`).
//!
//! As upstream's static class, the state is process-wide. Tests capture the output with
//! [`capture`].

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

static VERBOSE: AtomicBool = AtomicBool::new(false);

struct Channel {
	/// A captured sink replacing stdout, for tests.
	sink: Option<Vec<u8>>,
	/// Whether the channel writes to stderr instead of stdout (Go's `Output = os.Stderr`).
	stderr: bool,
	/// Whether the last write left an unterminated line (`LogService.ts` L5-9).
	partial: bool,
}

static CHANNEL: Mutex<Channel> = Mutex::new(Channel {
	sink: None,
	stderr: false,
	partial: false,
});

/// `Verbose`: whether the verbose writes print.
#[must_use]
pub fn verbose() -> bool {
	VERBOSE.load(Ordering::Relaxed)
}

/// Sets [`verbose`], once before concurrent logging begins.
pub fn set_verbose(on: bool) {
	VERBOSE.store(on, Ordering::Relaxed);
}

fn write_locked(channel: &mut Channel, message: &str) {
	channel.partial = !message.ends_with('\n');
	match &mut channel.sink {
		Some(buffer) => buffer.extend_from_slice(message.as_bytes()),
		None if channel.stderr => {
			let _ = io::stderr().lock().write_all(message.as_bytes());
		}
		None => {
			// Go's fmt.Fprint drops a failed stdout write too.
			let _ = io::stdout().lock().write_all(message.as_bytes());
		}
	}
}

fn write_line_locked(channel: &mut Channel, messages: &[&str]) {
	if channel.partial {
		write_locked(channel, "\n");
	}
	for message in messages {
		write_locked(channel, &format!("{message}\n"));
	}
}

/// `Write` (`LogService.write`).
pub fn write(message: &str) {
	let mut channel = CHANNEL.lock().unwrap_or_else(PoisonError::into_inner);
	write_locked(&mut channel, message);
}

/// `WriteLine` (`LogService.writeLine`), including the partial-line `\n` injection.
pub fn write_line(messages: &[&str]) {
	let mut channel = CHANNEL.lock().unwrap_or_else(PoisonError::into_inner);
	write_line_locked(&mut channel, messages);
}

/// `WriteLineIfVerbose`
pub fn write_line_if_verbose(messages: &[&str]) {
	if verbose() {
		write_line(messages);
	}
}

/// `Warn`: the yellow `Compiler Warning:` prefix and the message. Warnings print whatever
/// [`verbose`] says and never fail a build.
pub fn warn(message: &str) {
	let mut channel = CHANNEL.lock().unwrap_or_else(PoisonError::into_inner);
	let prefix = if channel.sink.is_none() && use_color() {
		"\x1b[33mCompiler Warning:\x1b[39m"
	} else {
		"Compiler Warning:"
	};
	write_line_locked(&mut channel, &[&format!("{prefix} {message}")]);
	drop(channel);
}

/// `BenchmarkIfVerbose`: under verbose, runs `callback` and then writes `name ( N ms )`;
/// otherwise just runs it.
pub fn benchmark_if_verbose<T>(name: &str, callback: impl FnOnce() -> T) -> T {
	if !verbose() {
		return callback();
	}
	let start = Instant::now();
	let result = callback();
	write_line(&[&format!("{name} ( {} ms )", start.elapsed().as_millis())]);
	result
}

/// `WriteStageStartIfVerbose`
pub fn write_stage_start_if_verbose(project: &str, name: &str) {
	if verbose() {
		write_line(&[&format!("{}...", stage_label(project, name))]);
	}
}

/// `WriteStageDoneIfVerbose`
pub fn write_stage_done_if_verbose(project: &str, name: &str, duration: Duration) {
	if verbose() {
		write_line(&[&format!(
			"{} ( {} ms )",
			stage_label(project, name),
			duration.as_millis()
		)]);
	}
}

fn stage_label(project: &str, name: &str) -> String {
	if project.is_empty() {
		return name.to_owned();
	}
	format!("{project}: {name}")
}

/// `useColor`: `NO_COLOR` wins, otherwise color only on a terminal.
fn use_color() -> bool {
	if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
		return false;
	}
	io::stdout().is_terminal()
}

/// Sends the output to stderr (`true`) or stdout, as Go's `logservice.Output` assignment does;
/// returns the previous setting.
pub fn set_output_stderr(stderr: bool) -> bool {
	let mut channel = CHANNEL.lock().unwrap_or_else(PoisonError::into_inner);
	std::mem::replace(&mut channel.stderr, stderr)
}

/// Redirects the output to a buffer until [`take_capture`], resetting the partial-line tracker.
pub fn capture() {
	let mut channel = CHANNEL.lock().unwrap_or_else(PoisonError::into_inner);
	channel.sink = Some(Vec::new());
	channel.partial = false;
}

/// Ends a [`capture`] and returns what was written.
#[must_use]
pub fn take_capture() -> String {
	let mut channel = CHANNEL.lock().unwrap_or_else(PoisonError::into_inner);
	channel.partial = false;
	String::from_utf8_lossy(&channel.sink.take().unwrap_or_default()).into_owned()
}

#[cfg(test)]
mod tests {
	//! `logservice_test.go`. The channel is process-wide, so every test holds [`SERIAL`] while it
	//! captures.

	use std::sync::{Mutex, MutexGuard, PoisonError};
	use std::time::Duration;

	use super::{
		benchmark_if_verbose, capture, set_verbose, take_capture, warn, write, write_line, write_line_if_verbose,
		write_stage_done_if_verbose, write_stage_start_if_verbose,
	};

	static SERIAL: Mutex<()> = Mutex::new(());

	/// `capture`: serializes the test and redirects the channel.
	fn captured() -> MutexGuard<'static, ()> {
		let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
		capture();
		set_verbose(false);
		guard
	}

	/// Whether `line` is `name ( N ms )`.
	fn is_benchmark_line(line: &str, name: &str) -> bool {
		line.strip_prefix(name)
			.and_then(|rest| rest.strip_prefix(" ( "))
			.and_then(|rest| rest.strip_suffix(" ms )"))
			.is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
	}

	#[test]
	fn write_line_injects_newline_after_partial_write() {
		let _serial = captured();
		write("compiling");
		write_line(&["Hello there."]);
		assert_eq!(take_capture(), "compiling\nHello there.\n");
	}

	#[test]
	fn write_line_no_injection_after_complete_line() {
		let _serial = captured();
		write("done\n");
		write_line(&["next"]);
		assert_eq!(take_capture(), "done\nnext\n");
	}

	#[test]
	fn warn_format() {
		let _serial = captured();
		warn("Multiple *.project.json files found, using a.project.json");
		assert_eq!(
			take_capture(),
			"Compiler Warning: Multiple *.project.json files found, using a.project.json\n"
		);
	}

	#[test]
	fn write_line_if_verbose_gate() {
		let _serial = captured();
		write_line_if_verbose(&["hidden"]);
		set_verbose(true);
		write_line_if_verbose(&["compiling as model.."]);
		set_verbose(false);
		assert_eq!(take_capture(), "compiling as model..\n");
	}

	#[test]
	fn benchmark_if_verbose_format() {
		let _serial = captured();
		set_verbose(true);
		let ran = benchmark_if_verbose("copy include files", || true);
		set_verbose(false);
		assert!(ran, "callback did not run");
		let got = take_capture();
		assert!(
			got.strip_suffix('\n')
				.is_some_and(|line| is_benchmark_line(line, "copy include files")),
			"benchmark line {got:?} does not match upstream format"
		);
	}

	#[test]
	fn benchmark_if_verbose_silent_when_not_verbose() {
		let _serial = captured();
		let ran = benchmark_if_verbose("writing compiled files", || true);
		assert!(ran, "callback did not run");
		let got = take_capture();
		assert!(got.is_empty(), "non-verbose benchmark leaked output: {got:?}");
	}

	#[test]
	fn concurrent_benchmark_if_verbose_and_warn() {
		let _serial = captured();
		set_verbose(true);
		let names = ["compile alpha", "compile beta", "compile gamma"];
		let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
		let release = std::sync::Barrier::new(names.len() + 1);
		std::thread::scope(|scope| {
			for name in names {
				let started_tx = started_tx.clone();
				let release = &release;
				scope.spawn(move || {
					benchmark_if_verbose(name, || {
						let _ = started_tx.send(());
						release.wait();
					});
				});
			}
			for _ in names {
				let _ = started_rx.recv();
			}
			warn("parallel compile in progress");
			release.wait();
		});
		set_verbose(false);

		let got = take_capture();
		assert_eq!(
			got.matches("Compiler Warning: parallel compile in progress\n").count(),
			1,
			"warning line count; output: {got:?}"
		);
		for name in names {
			assert!(
				got.lines().any(|line| is_benchmark_line(line, name)),
				"missing benchmark line for {name:?}; output: {got:?}"
			);
		}
		assert!(
			!got.contains("Compiler Warning: parallel compile in progress ("),
			"warning glued to benchmark line: {got:?}"
		);
	}

	#[test]
	fn stage_verbose_emits_start_before_work() {
		let _serial = captured();
		set_verbose(true);
		write_stage_start_if_verbose("child", "sidecar worker");
		let start = take_capture();
		capture();
		write_stage_done_if_verbose("child", "sidecar worker", Duration::from_millis(5));
		set_verbose(false);
		assert_eq!(
			start, "child: sidecar worker...\n",
			"start output, want start line only"
		);
		let done = take_capture();
		assert!(
			done.ends_with("child: sidecar worker ( 5 ms )\n"),
			"completion output = {done:?}"
		);
	}
}
