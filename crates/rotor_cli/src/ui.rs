//! rotor's human-facing output: the header, cargo-style verb lines, and the small formatters the
//! commands share. Colour is decided per stream by the caller ([`Paint`]); plain output is the
//! same text with no escapes.
//!
//! The pieces are independent so the watch loop (`watch.rs`) reuses them: [`header`], [`verb`],
//! [`summary_parts`] and the frames in [`crate::frames`].

use std::io::Write;
use std::time::Duration;

use rotor_support::gopath;

use crate::VERSION;

/// An SGR sequence: what goes between `ESC [` and `m`.
type Style = &'static str;

const BRAND: Style = "1;38;5;205";
const PINK: Style = "38;5;205";
const BOLD: Style = "1";
const MUTED: Style = "90";
const BOLD_MUTED: Style = "1;90";
const GREEN: Style = "1;92";
const RED: Style = "1;91";
const RED_PLAIN: Style = "91";
const YELLOW: Style = "1;93";
const YELLOW_PLAIN: Style = "93";
const CYAN: Style = "1;96";
const CYAN_PLAIN: Style = "96";
const BLUE: Style = "1;94";
const BLUE_PLAIN: Style = "94";

/// Styles text for one stream: ANSI when `color`, the bare text otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
	/// Whether the stream gets colour.
	pub color: bool,
}

macro_rules! paints {
	($($(#[$doc:meta])* $name:ident => $style:expr;)*) => {
		$(
			$(#[$doc])*
			#[must_use]
			pub fn $name(self, text: &str) -> String {
				self.paint($style, text)
			}
		)*
	};
}

impl Paint {
	fn paint(self, style: Style, text: &str) -> String {
		if !self.color || text.is_empty() {
			return text.to_owned();
		}
		format!("\x1b[{style}m{text}\x1b[0m")
	}

	paints! {
		/// The bold pink wordmark.
		brand => BRAND;
		/// Pink, for bars.
		pink => PINK;
		/// Bold.
		bold => BOLD;
		/// Secondary text.
		muted => MUTED;
		/// Bold green: work done.
		green => GREEN;
		/// Bold red: failure.
		red => RED;
		/// Red.
		red_plain => RED_PLAIN;
		/// Bold yellow: warning.
		yellow => YELLOW;
		/// Yellow.
		yellow_plain => YELLOW_PLAIN;
		/// Bold cyan: in progress or informational.
		cyan => CYAN;
		/// Cyan.
		cyan_plain => CYAN_PLAIN;
		/// Bold blue: frame chrome.
		blue => BLUE;
		/// Blue.
		blue_plain => BLUE_PLAIN;
	}

	/// The separator between summary parts.
	#[must_use]
	pub fn dot(self) -> String {
		format!("  {}  ", self.muted("·"))
	}
}

/// The colour of a verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
	/// Work done.
	Success,
	/// Nothing to do.
	Quiet,
	/// Failed.
	Failure,
	/// Informational.
	Info,
	/// Attention.
	Warning,
}

/// The width of cargo's verb column.
pub const VERB_WIDTH: usize = 12;

/// A verb right-aligned in cargo's 12-column gutter.
#[must_use]
pub fn verb(paint: Paint, tone: Tone, word: &str) -> String {
	let padded = format!("{word:>VERB_WIDTH$}");
	match tone {
		Tone::Success => paint.green(&padded),
		Tone::Quiet => paint.paint(BOLD_MUTED, &padded),
		Tone::Failure => paint.red(&padded),
		Tone::Info => paint.cyan(&padded),
		Tone::Warning => paint.yellow(&padded),
	}
}

/// Writes one verb line.
pub fn verb_line(out: &mut dyn Write, paint: Paint, tone: Tone, word: &str, rest: &str) {
	let _ = writeln!(out, "{} {rest}", verb(paint, tone, word));
}

/// Writes a continuation line aligned under a verb line's text.
pub fn continuation(out: &mut dyn Write, rest: &str) {
	let _ = writeln!(out, "{:width$} {rest}", "", width = VERB_WIDTH);
}

/// The product header: the wordmark, the version, and what the command is working on, then a blank
/// line.
pub fn header(out: &mut dyn Write, paint: Paint, parts: &[&str]) {
	let mut line = format!("  {} {}", paint.brand("sloptor"), paint.muted(&format!("v{VERSION}")));
	for (index, part) in parts.iter().filter(|part| !part.is_empty()).enumerate() {
		line.push_str(&paint.dot());
		line.push_str(&if index == 0 {
			paint.cyan_plain(part)
		} else {
			paint.muted(part)
		});
	}
	let _ = write!(out, "{line}\n\n");
}

/// A warning row: `warning: <message>` in the verb gutter.
pub fn warn(out: &mut dyn Write, paint: Paint, message: &str) {
	verb_line(out, paint, Tone::Warning, "Warning", message);
}

/// `1,975`
#[must_use]
pub fn thousands(count: usize) -> String {
	let digits = count.to_string();
	let mut out = String::with_capacity(digits.len() + digits.len() / 3);
	for (index, digit) in digits.chars().enumerate() {
		if index > 0 && (digits.len() - index).is_multiple_of(3) {
			out.push(',');
		}
		out.push(digit);
	}
	out
}

/// `1 file`, `2 files`, `1,975 files`
#[must_use]
pub fn plural(count: usize, noun: &str) -> String {
	if count == 1 {
		format!("1 {noun}")
	} else {
		format!("{} {noun}s", thousands(count))
	}
}

/// A wall time for humans: `182 ms` under a second, `2.41s` from one.
#[must_use]
pub fn duration(elapsed: Duration) -> String {
	let ms = elapsed.as_millis();
	if ms < 1000 {
		return format!("{ms} ms");
	}
	format!("{:.2}s", elapsed.as_secs_f64())
}

/// Throughput, when the build took long enough to measure.
#[must_use]
pub fn rate(files: usize, elapsed: Duration) -> Option<String> {
	let seconds = elapsed.as_secs_f64();
	// A rate over a tiny or instant build says nothing.
	if seconds < 0.25 || files < 50 {
		return None;
	}
	#[expect(
		clippy::cast_precision_loss,
		clippy::cast_possible_truncation,
		clippy::cast_sign_loss,
		reason = "a rounded, displayed rate"
	)]
	let per_second = (files as f64 / seconds).round() as usize;
	Some(format!("{} files/s", thousands(per_second)))
}

/// `relForDisplay`: relative to the working directory when possible, slash-separated.
#[must_use]
pub fn rel_for_display(path: &str) -> String {
	std::env::current_dir()
		.ok()
		.and_then(|cwd| gopath::rel(&cwd.to_string_lossy(), path).ok())
		.map_or_else(|| path.to_owned(), |rel| gopath::to_slash(&rel).into_owned())
}

/// A project's name for humans: its directory relative to the working directory, or the
/// directory's own name when it is the working directory. A tsconfig other than `tsconfig.json`
/// keeps its file name.
#[must_use]
pub fn project_name(config_path: &str) -> String {
	let dir = gopath::dir(config_path);
	let mut name = rel_for_display(&dir);
	if name == "." || name.is_empty() {
		name = gopath::base(&dir);
	}
	name
}

/// The config file a project was built from, when it is not the default name.
#[must_use]
pub fn config_note(config_path: &str) -> String {
	gopath::base(config_path)
}

/// `2 errors`, `1 warning`, each in its colour, for a result line; `None` when there are none.
#[must_use]
pub fn counts(paint: Paint, errors: usize, warnings: usize) -> Vec<String> {
	let mut parts = Vec::new();
	if errors > 0 {
		parts.push(paint.red_plain(&plural(errors, "error")));
	}
	if warnings > 0 {
		parts.push(paint.yellow_plain(&plural(warnings, "warning")));
	}
	parts
}

/// The parts of a result line, joined by the dot.
#[must_use]
pub fn summary_parts(paint: Paint, parts: &[String]) -> String {
	parts
		.iter()
		.filter(|part| !part.is_empty())
		.cloned()
		.collect::<Vec<_>>()
		.join(&paint.dot())
}

/// The visible width of `text`: characters outside ANSI escapes.
#[must_use]
pub fn visible_width(text: &str) -> usize {
	rotor_support::term::visible_len(text)
}

/// `text` padded with spaces to `width` visible characters.
#[must_use]
pub fn pad(text: &str, width: usize) -> String {
	let mut padded = text.to_owned();
	padded.push_str(&" ".repeat(width.saturating_sub(visible_width(text))));
	padded
}

/// `text` right-aligned to `width` visible characters.
#[must_use]
pub fn pad_left(text: &str, width: usize) -> String {
	format!("{}{text}", " ".repeat(width.saturating_sub(visible_width(text))))
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use super::{Paint, Tone, duration, plural, thousands, verb};

	#[test]
	fn numbers_and_durations_read_like_cargo() {
		assert_eq!(thousands(0), "0");
		assert_eq!(thousands(999), "999");
		assert_eq!(thousands(1975), "1,975");
		assert_eq!(thousands(1_234_567), "1,234,567");
		assert_eq!(plural(1, "file"), "1 file");
		assert_eq!(plural(1975, "file"), "1,975 files");
		assert_eq!(duration(Duration::from_millis(182)), "182 ms");
		assert_eq!(duration(Duration::from_millis(2414)), "2.41s");
	}

	#[test]
	fn plain_verbs_have_no_escapes() {
		let plain = Paint { color: false };
		assert_eq!(verb(plain, Tone::Success, "Finished"), "    Finished");
		assert!(verb(Paint { color: true }, Tone::Failure, "Failed").contains("\x1b["));
	}
}
