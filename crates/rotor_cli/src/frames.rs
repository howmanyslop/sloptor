//! rustc-style diagnostic frames: the severity and code, the message, the source line with its
//! span underlined, tsc's related information and rotor's `Suggestion:` hints, then a count.
//!
//! ```text
//! error[TS2322]: Type 'number' is not assignable to type 'string'.
//!    ╭▸ src/client/ui/Hud.tsx:14:27
//!    │
//! 14 │     const label: string = props.count;
//!    │                           ━━━━━━━━━━━
//!    ╰╴
//!    = note: 'count' is declared here
//! ```

use std::collections::HashMap;
use std::fmt::Write as _;

use rotor_compile::compile::DiagnosticInfo;

use crate::ui::{self, Paint};

/// How severe a frame is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
	/// An error.
	Error,
	/// A warning.
	Warning,
}

/// A span of one source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
	/// The path shown, relative to the working directory.
	pub path: String,
	/// 1-based.
	pub line: usize,
	/// 1-based, in characters.
	pub column: usize,
	/// The line's text, tabs expanded.
	pub text: String,
	/// The underlined columns of `text`: 0-based start, width.
	pub span: (usize, usize),
}

impl Location {
	/// The location of `offset..offset + len` in `source`, or `None` outside it.
	#[must_use]
	pub fn new(path: String, source: &str, offset: usize, len: usize) -> Option<Self> {
		if offset > source.len() || !source.is_char_boundary(offset) {
			return None;
		}
		let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
		let line_end = source[offset..].find('\n').map_or(source.len(), |index| offset + index);
		let raw = source[line_start..line_end].trim_end_matches('\r');
		let line = source[..line_start].matches('\n').count() + 1;
		let before = &source[line_start..offset];
		let span_end = (offset + len).min(line_start + raw.len()).max(offset);
		let spanned = source.get(offset..span_end).unwrap_or_default();
		let column = before.chars().count() + 1;
		let start = expanded_width(before);
		let width = expanded_width(&format!("{before}{spanned}")) - start;
		Some(Self {
			path,
			line,
			column,
			text: raw.replace('\t', "    "),
			span: (start, width.max(1)),
		})
	}
}

fn expanded_width(text: &str) -> usize {
	text.chars()
		.map(|character| if character == '\t' { 4 } else { 1 })
		.sum()
}

/// A related-information entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Related {
	/// The message.
	pub message: String,
	/// Where it points, if anywhere.
	pub location: Option<Location>,
}

/// One diagnostic, ready to render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
	/// How severe it is.
	pub severity: Severity,
	/// `TS2322`, a transformer diagnostic's name, or empty.
	pub code: String,
	/// The message's first line.
	pub message: String,
	/// The message's other lines: tsc's elaboration chain.
	pub notes: Vec<String>,
	/// `Suggestion:` lines.
	pub help: Vec<String>,
	/// Where it points, if anywhere.
	pub location: Option<Location>,
	/// tsc's related information.
	pub related: Vec<Related>,
}

/// Source texts read once per render, by absolute path; overlays win over the disk.
pub struct Sources<'a> {
	texts: HashMap<String, Option<String>>,
	overlays: Option<&'a std::collections::BTreeMap<String, String>>,
}

impl<'a> Sources<'a> {
	/// Sources read from disk, with `overlays` (keyed by absolute path) in place of their files.
	#[must_use]
	pub fn new(overlays: Option<&'a std::collections::BTreeMap<String, String>>) -> Self {
		Self {
			texts: HashMap::new(),
			overlays,
		}
	}

	fn location(&mut self, file_name: &str, offset: usize, len: usize) -> Option<Location> {
		if file_name.is_empty() {
			return None;
		}
		let overlays = self.overlays;
		let text = self.texts.entry(file_name.to_owned()).or_insert_with(|| {
			overlays
				.and_then(|overlays| {
					overlays
						.get(file_name)
						.or_else(|| overlays.get(&*rotor_support::gopath::from_slash(file_name)))
				})
				.cloned()
				.or_else(|| {
					std::fs::read(file_name)
						.ok()
						.map(|data| String::from_utf8_lossy(&data).into_owned())
				})
				.map(|text| text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text))
		});
		Location::new(ui::rel_for_display(file_name), text.as_deref()?, offset, len)
	}

	/// The frame of a compile diagnostic.
	pub fn frame(&mut self, diagnostic: &DiagnosticInfo) -> Frame {
		let mut lines = diagnostic.message.lines();
		let message = lines.next().unwrap_or_default().to_owned();
		let mut notes = Vec::new();
		let mut help = Vec::new();
		for line in lines {
			if let Some(suggestion) = line.strip_prefix("Suggestion: ") {
				help.push(suggestion.to_owned());
			} else if let Some(more) = line.strip_prefix("More information: ") {
				notes.push(format!("more information: {more}"));
			} else if !line.trim().is_empty() {
				notes.push(line.trim().to_owned());
			}
		}
		let location = self.location(&diagnostic.file_name, diagnostic.offset, diagnostic.len);
		let related = diagnostic
			.related
			.iter()
			.map(|related| Related {
				message: related.message.clone(),
				location: self.location(&related.file_name, related.offset, related.len),
			})
			.collect();
		Frame {
			severity: if diagnostic.warning {
				Severity::Warning
			} else {
				Severity::Error
			},
			code: diagnostic.code.clone(),
			message,
			notes,
			help,
			location,
			related,
		}
	}
}

fn snippet(out: &mut String, paint: Paint, location: &Location, indent: usize, underline: Underline) {
	let number = location.line.to_string();
	let gutter = " ".repeat(indent + number.len());
	let bar = paint.blue("│");
	let _ = writeln!(
		out,
		"{gutter} {} {}:{}:{}",
		paint.blue("╭▸"),
		paint.bold(&location.path),
		location.line,
		location.column
	);
	let _ = writeln!(out, "{gutter} {bar}");
	let line = format!("{}{} {bar} {}", " ".repeat(indent), paint.blue(&number), location.text);
	let _ = writeln!(out, "{}", line.trim_end());
	let (start, width) = location.span;
	let _ = writeln!(
		out,
		"{gutter} {bar} {}{}",
		" ".repeat(start),
		underline(paint, &"━".repeat(width))
	);
	let _ = writeln!(out, "{gutter} {}", paint.blue("╰╴"));
}

/// Colours an underline.
type Underline = fn(Paint, &str) -> String;

/// Renders `frame` into `out`.
pub fn render(out: &mut String, paint: Paint, frame: &Frame) {
	let (word, underline): (&str, Underline) = match frame.severity {
		Severity::Error => ("error", |paint, text| paint.red_plain(text)),
		Severity::Warning => ("warning", |paint, text| paint.yellow_plain(text)),
	};
	let head = if frame.code.is_empty() {
		word.to_owned()
	} else {
		format!("{word}[{}]", frame.code)
	};
	let head = match frame.severity {
		Severity::Error => paint.red(&head),
		Severity::Warning => paint.yellow(&head),
	};
	let _ = writeln!(out, "{head}{}", paint.bold(&format!(": {}", frame.message)));
	let width = frame
		.location
		.as_ref()
		.map_or(0, |location| location.line.to_string().len());
	let gutter = " ".repeat(width);
	if let Some(location) = &frame.location {
		snippet(out, paint, location, 0, underline);
	}
	let equals = paint.blue("=");
	if frame.location.is_none() && frame.code.is_empty() {
		// A bare failure message: its other lines are its body, not tsc notes.
		for line in &frame.notes {
			let _ = writeln!(out, "  {line}");
		}
		for help in &frame.help {
			let _ = writeln!(out, "  {} {help}", paint.bold("help:"));
		}
		return;
	}
	for note in &frame.notes {
		let _ = writeln!(out, "{gutter} {equals} {}: {note}", paint.bold("note"));
	}
	for related in &frame.related {
		let _ = writeln!(out, "{gutter} {equals} {}: {}", paint.bold("note"), related.message);
		if let Some(location) = &related.location {
			snippet(out, paint, location, width + 3, Paint::blue_plain);
		}
	}
	for help in &frame.help {
		let _ = writeln!(out, "{gutter} {equals} {}: {help}", paint.bold("help"));
	}
}

/// Renders up to `max` frames (0 renders all), a blank line after each, then a line naming the
/// ones left out.
pub fn render_all(out: &mut String, paint: Paint, frames: &[Frame], max: usize) {
	let shown = if max == 0 { frames.len() } else { max.min(frames.len()) };
	for frame in &frames[..shown] {
		render(out, paint, frame);
		out.push('\n');
	}
	if shown < frames.len() {
		let _ = write!(
			out,
			"  {}\n\n",
			paint.muted(&format!(
				"… {} not shown (--max-errors 0 shows every one)",
				ui::plural(frames.len() - shown, "more diagnostic")
			))
		);
	}
}

/// `(errors, warnings)` among `frames`.
#[must_use]
pub fn tally(frames: &[Frame]) -> (usize, usize) {
	let warnings = frames
		.iter()
		.filter(|frame| frame.severity == Severity::Warning)
		.count();
	(frames.len() - warnings, warnings)
}

/// The distinct files `frames` point into.
#[must_use]
pub fn files(frames: &[Frame]) -> usize {
	let mut paths: Vec<&str> = frames
		.iter()
		.filter_map(|frame| frame.location.as_ref().map(|location| location.path.as_str()))
		.collect();
	paths.sort_unstable();
	paths.dedup();
	paths.len()
}

#[cfg(test)]
mod tests {
	use super::{Frame, Location, Related, Severity, render};
	use crate::ui::Paint;

	#[test]
	fn a_frame_underlines_its_span_and_lists_notes() {
		let source = "export function f() {\n\tconst label: string = props.count;\n}\n";
		let offset = source.find("props").unwrap_or_default();
		let frame = Frame {
			severity: Severity::Error,
			code: "TS2322".to_owned(),
			message: "Type 'number' is not assignable to type 'string'.".to_owned(),
			notes: Vec::new(),
			help: vec!["Use `unknown` instead.".to_owned()],
			location: Location::new("src/a.ts".to_owned(), source, offset, "props.count".len()),
			related: vec![Related {
				message: "'count' is declared here.".to_owned(),
				location: None,
			}],
		};
		let mut out = String::new();
		render(&mut out, Paint { color: false }, &frame);
		assert_eq!(
			out,
			"error[TS2322]: Type 'number' is not assignable to type 'string'.\n  ╭▸ src/a.ts:2:24\n  │\n2 │     \
			 const label: string = props.count;\n  │                           ━━━━━━━━━━━\n  ╰╴\n  = note: 'count' is \
			 declared here.\n  = help: Use `unknown` instead.\n"
		);
	}

	#[test]
	fn a_location_outside_the_source_is_none() {
		assert_eq!(Location::new(String::new(), "abc", 10, 1), None);
		let location = Location::new(String::new(), "a\r\nbc", 3, 0).unwrap_or_else(|| unreachable!());
		assert_eq!((location.line, location.column, location.span), (2, 1, (0, 1)));
	}
}
