//! `cmd/rotor/ui.go`: rotor's human-facing chrome (the header, result blocks, code frames and
//! event rows). It is separate from `logservice`, which owns the upstream-compatible stdout
//! channel.
//!
//! Stage 7a keeps Go's information content here; the redesigned output lands in Phase 2.

use std::io::Write;
use std::time::Duration;

use rotor_compile::compile::DiagnosticInfo;
use rotor_support::diagframe::{self, Group, Language, Spot};
use rotor_support::gopath;
use rotor_support::term::{Styler, visible_len};

use crate::VERSION;

/// `plural`
#[must_use]
pub fn plural(count: usize, noun: &str) -> String {
	if count == 1 {
		format!("1 {noun}")
	} else {
		format!("{count} {noun}s")
	}
}

/// `banner`: the product header at the top of a command.
pub fn banner(out: &mut dyn Write, styler: Styler, sub: &str) {
	let mut head = format!(
		"{} {}",
		styler.accent(&styler.bold("sloptor")),
		styler.muted(&format!("v{VERSION}"))
	);
	if !sub.is_empty() {
		head = format!("{head}  {}  {}", styler.muted(styler.glyphs().dot), styler.info(sub));
	}
	let _ = write!(out, "\n  {head}\n\n");
}

/// `warn`: one warning row in rotor chrome.
pub fn warn(out: &mut dyn Write, styler: Styler, message: &str) {
	let _ = writeln!(out, "  {}  {message}", styler.warn_bold(styler.glyphs().warn));
}

/// `noteLine`: a muted arrow row for a secondary fact.
pub fn note_line(out: &mut dyn Write, styler: Styler, message: &str) {
	let _ = writeln!(
		out,
		"    {} {}",
		styler.muted(styler.glyphs().arrow),
		styler.muted(message)
	);
}

/// `buildSuccess`: the result block of a successful build.
pub fn build_success(
	out: &mut dyn Write,
	styler: Styler,
	total: usize,
	written: usize,
	unchanged: usize,
	elapsed: Duration,
) {
	let glyphs = styler.glyphs();
	let ms = elapsed.as_millis();
	let headline = format!(
		"{}  {} {}",
		styler.success_bold(glyphs.check),
		styler.bold(&format!("compiled {}", plural(total, "file"))),
		styler.muted(&format!("in {ms} ms"))
	);
	let mut parts = vec![format!(
		"{}{}",
		styler.bold(&written.to_string()),
		styler.muted(" written")
	)];
	if unchanged > 0 {
		parts.push(styler.muted(&format!("{unchanged} unchanged")));
	}
	if ms > 0 {
		#[expect(clippy::cast_precision_loss, reason = "Go's float64 throughput")]
		#[expect(clippy::cast_possible_truncation, reason = "Go's int conversion")]
		let rate = (total as f64 / (ms as f64 / 1000.0)) as i64;
		parts.push(styler.muted(&format!("{rate} files/s")));
	}
	let _ = writeln!(out, "  {headline}");
	let _ = write!(out, "    {}\n\n", parts.join(&format!(" {} ", glyphs.dot)));
}

/// `buildFailure`: the failure headline, then a code frame per located diagnostic.
pub fn build_failure(
	out: &mut dyn Write,
	styler: Styler,
	headline: &str,
	diagnostics: &[DiagnosticInfo],
	max_frames: usize,
) {
	let _ = writeln!(
		out,
		"  {}  {}",
		styler.error_bold(styler.glyphs().cross),
		styler.bold(headline)
	);
	let _ = writeln!(out);
	if diagnostics.is_empty() {
		return;
	}
	let _ = write!(out, "{}", render_diag_frames(styler.color(), diagnostics, max_frames));
	let _ = writeln!(out);
}

/// `renderDiagFrames`: located diagnostics framed per file, the rest as indented lines.
#[must_use]
pub fn render_diag_frames(color: bool, diagnostics: &[DiagnosticInfo], max_frames: usize) -> String {
	let mut groups: Vec<Group> = Vec::new();
	let mut loose = Vec::new();
	for diagnostic in diagnostics {
		if diagnostic.file_name.is_empty() || diagnostic.len == 0 {
			loose.push(diagnostic.message.clone());
			continue;
		}
		let (primary, help) = split_help(&diagnostic.message);
		let spot = Spot {
			offset: isize::try_from(diagnostic.offset).unwrap_or(isize::MAX),
			len: isize::try_from(diagnostic.len).unwrap_or(isize::MAX),
			severity: if diagnostic.warning {
				diagframe::Severity::Warning
			} else {
				diagframe::Severity::Error
			},
			code: diagnostic.code.clone(),
			message: primary,
			help,
		};
		let path = rel_for_display(&diagnostic.file_name);
		if let Some(group) = groups.iter_mut().find(|group| group.path == path) {
			group.spots.push(spot);
		} else {
			groups.push(Group {
				path,
				source: std::fs::read(&diagnostic.file_name)
					.map(|data| String::from_utf8_lossy(&data).into_owned())
					.unwrap_or_default(),
				lang: Language::TypeScript,
				spots: vec![spot],
			});
		}
	}
	let mut out = diagframe::render_groups(
		&groups,
		diagframe::Options {
			context: 0,
			color,
			link: color,
		},
		max_frames,
	);
	for line in loose {
		out.push_str("    ");
		out.push_str(&line);
		out.push('\n');
	}
	out
}

/// `splitHelp`: the primary lines and the trailing `Suggestion:` / `More information:` lines.
#[must_use]
pub fn split_help(message: &str) -> (String, Vec<String>) {
	let mut parts = message.split('\n');
	let mut primary = parts.next().unwrap_or_default().to_owned();
	let mut help = Vec::new();
	for part in parts {
		if part.starts_with("Suggestion: ") || part.starts_with("More information: ") {
			help.push(part.to_owned());
		} else {
			primary.push('\n');
			primary.push_str(part);
		}
	}
	(primary, help)
}

/// `relForDisplay`: relative to the working directory when possible.
#[must_use]
pub fn rel_for_display(path: &str) -> String {
	std::env::current_dir()
		.ok()
		.and_then(|cwd| gopath::rel(&cwd.to_string_lossy(), path).ok())
		.map_or_else(|| path.to_owned(), |rel| gopath::to_slash(&rel).into_owned())
}

/// `checkSummary`: the result line of `sloptor check`.
pub fn check_summary(out: &mut dyn Write, styler: Styler, files: usize, errors: usize, elapsed: Duration) {
	let glyphs = styler.glyphs();
	let ms = elapsed.as_millis();
	let checked = styler.bold(&format!("checked {}", plural(files, "file")));
	if errors == 0 {
		let _ = write!(
			out,
			"\n  {}  {checked} {}\n\n",
			styler.success_bold(glyphs.check),
			styler.muted(&format!("in {ms} ms · no errors"))
		);
		return;
	}
	let _ = write!(
		out,
		"\n  {}  {checked} {}{}\n\n",
		styler.error_bold(glyphs.cross),
		styler.muted(&format!("in {ms} ms · ")),
		styler.error(&plural(errors, "error"))
	);
}

/// `eventStatus`: the status word of an event row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventStatus {
	/// Work done: a target was removed.
	Removed,
	/// The batch finished.
	Finished,
	/// Nothing to do.
	Unchanged,
	/// A dry run's would-be work.
	Planned,
	/// Failed.
	Failed,
}

impl EventStatus {
	const fn word(self) -> &'static str {
		match self {
			Self::Removed => "Removed",
			Self::Finished => "Finished",
			Self::Unchanged => "Unchanged",
			Self::Planned => "Planned",
			Self::Failed => "Failed",
		}
	}

	fn styled(self, styler: Styler) -> String {
		match self {
			Self::Unchanged | Self::Planned => styler.muted(self.word()),
			Self::Failed => styler.error_bold(self.word()),
			Self::Removed | Self::Finished => styler.success_bold(self.word()),
		}
	}
}

/// `uiEvent`: one row of a completed batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
	/// The status word.
	pub status: EventStatus,
	/// A slash-separated path or a logical name; may be empty.
	pub target: String,
	/// A dim detail; may be empty.
	pub detail: String,
	/// A measured duration; zero is not shown.
	pub elapsed: Duration,
}

/// `events`: one aligned row per event, with the status and target columns padded to the widest
/// visible cell and empty trailing columns left out.
pub fn events(out: &mut dyn Write, styler: Styler, events: &[Event]) {
	let status_width = events.iter().map(|event| event.status.word().len()).max().unwrap_or(0);
	let target_width = events.iter().map(|event| visible_len(&event.target)).max().unwrap_or(0);
	for event in events {
		let mut cells = vec![pad(&event.status.styled(styler), status_width)];
		if !event.target.is_empty() {
			cells.push(pad(&rel_for_display(&event.target), target_width));
		}
		if !event.detail.is_empty() {
			cells.push(styler.muted(&event.detail));
		}
		if !event.elapsed.is_zero() {
			cells.push(styler.muted(&format!("in {} ms", event.elapsed.as_millis())));
		}
		let _ = writeln!(out, "  {}", cells.join("  "));
	}
}

fn pad(text: &str, width: usize) -> String {
	let mut padded = text.to_owned();
	padded.push_str(&" ".repeat(width.saturating_sub(visible_len(text))));
	padded
}
