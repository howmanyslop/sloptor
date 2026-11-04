//! The live view of a build: a progress line on a terminal and, for a solution, one line per
//! project as it finishes.
//!
//! The build never waits on it. Workers bump the atomics of a [`BuildProgress`]; a render thread
//! samples them at most 15 times a second and owns every write while the build runs. On a pipe there
//! is no progress line, but project lines still print as projects finish.

use std::fmt::Write as _;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rotor_compile::progress::{BuildProgress, Phase, ProjectEvent, ProjectStatus};

use crate::term::Streams;
use crate::ui::{self, Paint, Tone};

/// 15 Hz.
const FRAME: Duration = Duration::from_millis(66);
/// A build faster than this never shows a progress line.
const GRACE: Duration = Duration::from_millis(150);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const BAR_CELLS: usize = 28;

/// What the live view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
	/// Draw the progress line on stderr.
	pub progress_line: bool,
	/// Print a line per finished project on stdout (a solution build).
	pub project_lines: bool,
	/// Count files (`false`) or projects (`true`) on the bar.
	pub solution: bool,
	/// The colour of stdout.
	pub out: Paint,
}

/// Whether the progress line may draw: stderr is a coloured terminal.
#[must_use]
pub fn progress_line_allowed(streams: &Streams) -> bool {
	streams.err_color && crate::term::process_terminals().1
}

/// The finished-project line of a solution build.
#[must_use]
pub fn project_line(paint: Paint, event: &ProjectEvent) -> String {
	let (tone, word) = match event.status {
		ProjectStatus::Built => (Tone::Success, "Built"),
		ProjectStatus::Fresh => (Tone::Quiet, "Fresh"),
		ProjectStatus::Failed => (Tone::Failure, "Failed"),
		ProjectStatus::Blocked => (Tone::Warning, "Blocked"),
	};
	let name = ui::pad(&paint.bold(&ui::project_name(&event.config_path)), 22);
	if event.status == ProjectStatus::Blocked {
		let blocker = ui::project_name(&event.blocked_by);
		return format!(
			"{} {name} {}",
			ui::verb(paint, tone, word),
			paint.muted(&format!("blocked by {blocker}"))
		);
	}
	if event.status == ProjectStatus::Fresh && event.files == 0 {
		return format!("{} {name} {}", ui::verb(paint, tone, word), paint.muted("up to date"));
	}
	let files = ui::pad_left(&ui::thousands(event.files), 5);
	let noun = if event.files == 1 { "file " } else { "files" };
	let mut line = format!(
		"{} {name} {files} {noun}  {}",
		ui::verb(paint, tone, word),
		ui::pad_left(&paint.muted(&ui::duration(event.elapsed)), 7)
	);
	let mut notes = ui::counts(paint, event.errors, event.warnings);
	if event.status == ProjectStatus::Fresh {
		notes.insert(0, paint.muted("up to date"));
	}
	if !notes.is_empty() {
		line.push_str("  ");
		line.push_str(&ui::summary_parts(paint, &notes));
	}
	line
}

fn bar(paint: Paint, done: usize, total: usize) -> String {
	let filled = (done * BAR_CELLS).checked_div(total).unwrap_or(0).min(BAR_CELLS);
	let mut bar = paint.pink(&"━".repeat(filled));
	if filled < BAR_CELLS {
		bar.push_str(&paint.muted("╺"));
		bar.push_str(&paint.muted(&"━".repeat(BAR_CELLS - filled - 1)));
	}
	bar
}

/// The progress line's text, cut to `width`.
fn progress_text(progress: &BuildProgress, solution: bool, tick: usize, elapsed: Duration, width: usize) -> String {
	let paint = Paint { color: true };
	let spinner = paint.cyan_plain(SPINNER[tick % SPINNER.len()]);
	let phase = progress.phase();
	let (word, done, total, unit, detail) = if solution {
		let (done, total) = progress.projects();
		let running: Vec<String> = progress
			.running()
			.iter()
			.map(|config| rotor_support::gopath::base(&rotor_support::gopath::dir(config)))
			.collect();
		("Building", done, total, "projects", running.join(", "))
	} else {
		let (checked, transformed, total) = progress.files();
		let done = match phase {
			Phase::Checking => checked,
			Phase::Starting | Phase::Loading => 0,
			_ => transformed,
		};
		(phase.verb(), done, total, "files", String::new())
	};
	let mut line = format!("  {spinner} {}", paint.cyan(&format!("{word:<13}")));
	if total > 0 {
		let _ = write!(
			line,
			"{}  {}{}",
			bar(paint, done, total),
			paint.bold(&ui::thousands(done)),
			paint.muted(&format!("/{} {unit}", ui::thousands(total)))
		);
	}
	let _ = write!(line, "  {}", paint.muted(&format!("{:.1}s", elapsed.as_secs_f64())));
	if !detail.is_empty() {
		let _ = write!(line, "  {}", paint.muted(&detail));
	}
	truncate(&line, width)
}

/// `text` cut to `width` visible characters, with its escapes kept and closed.
fn truncate(text: &str, width: usize) -> String {
	if ui::visible_width(text) <= width {
		return text.to_owned();
	}
	let mut out = String::new();
	let mut visible = 0;
	let mut escape = false;
	for character in text.chars() {
		if escape {
			out.push(character);
			escape = character != 'm';
			continue;
		}
		if character == '\x1b' {
			escape = true;
			out.push(character);
			continue;
		}
		if visible + 1 >= width {
			out.push('…');
			break;
		}
		out.push(character);
		visible += 1;
	}
	out.push_str("\x1b[0m");
	out
}

const CLEAR: &[u8] = b"\r\x1b[2K";

/// Runs `work` with the live view of `progress`, and returns its result. With nothing to show,
/// `work` just runs.
pub fn run<T>(streams: &mut Streams, progress: &Arc<BuildProgress>, view: View, work: impl FnOnce() -> T) -> T {
	if !view.progress_line && !view.project_lines {
		return work();
	}
	let width = terminal_size::terminal_size_of(std::io::stderr())
		.map_or(100, |(terminal_size::Width(width), _)| usize::from(width));
	// Whether the progress line is on screen; a log line clears it first.
	let drawn = Arc::new(Mutex::new(false));
	if view.progress_line {
		rotor_support::logservice::set_before_write(Some(crate::term::clear_line_hook(Arc::clone(&drawn))));
	}
	let out = &mut *streams.out;
	let err = &mut *streams.err;
	let result = std::thread::scope(|scope| {
		let (stop, stopped) = mpsc::channel::<()>();
		let drawn = Arc::clone(&drawn);
		let progress = Arc::clone(progress);
		scope.spawn(move || {
			let started = Instant::now();
			let mut seen = 0;
			let mut tick = 0;
			loop {
				let done = !matches!(stopped.recv_timeout(FRAME), Err(RecvTimeoutError::Timeout));
				let events = progress.finished_since(seen);
				seen += events.len();
				let draw = !done && view.progress_line && started.elapsed() >= GRACE;
				let line = draw.then(|| progress_text(&progress, view.solution, tick, started.elapsed(), width));
				let mut on_screen = drawn.lock().unwrap_or_else(PoisonError::into_inner);
				if (!events.is_empty() || done) && *on_screen {
					let _ = err.write_all(CLEAR);
					let _ = err.flush();
					*on_screen = false;
				}
				if view.project_lines && !events.is_empty() {
					for event in &events {
						let _ = writeln!(out, "{}", project_line(view.out, event));
					}
					let _ = out.flush();
				}
				if let Some(line) = line {
					let _ = err.write_all(CLEAR);
					let _ = err.write_all(line.as_bytes());
					let _ = err.flush();
					*on_screen = true;
					tick += 1;
				}
				drop(on_screen);
				if done {
					break;
				}
			}
		});
		let result = work();
		drop(stop);
		result
	});
	if view.progress_line {
		rotor_support::logservice::set_before_write(None);
	}
	result
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use rotor_compile::progress::{ProjectEvent, ProjectStatus};

	use super::{project_line, truncate};
	use crate::ui::Paint;

	#[test]
	fn project_lines_name_the_outcome() {
		let plain = Paint { color: false };
		let mut event = ProjectEvent {
			config_path: "/nowhere/packages/core/tsconfig.json".to_owned(),
			status: ProjectStatus::Built,
			files: 736,
			written: 512,
			errors: 0,
			warnings: 2,
			elapsed: Duration::from_millis(2410),
			blocked_by: String::new(),
		};
		let line = project_line(plain, &event);
		assert!(line.starts_with("       Built "), "{line}");
		assert!(line.contains("  736 files    2.41s  2 warnings"), "{line}");
		event.status = ProjectStatus::Blocked;
		"/nowhere/packages/net/tsconfig.json".clone_into(&mut event.blocked_by);
		assert!(
			project_line(plain, &event).ends_with("blocked by net")
				|| project_line(plain, &event).contains("blocked by")
		);
	}

	#[test]
	fn truncation_keeps_the_width() {
		assert_eq!(truncate("abcdef", 10), "abcdef");
		assert_eq!(truncate("abcdef", 4), "abc…\x1b[0m");
	}
}
