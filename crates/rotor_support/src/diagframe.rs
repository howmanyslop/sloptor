//! Renders a compiler or lexer diagnostic as a source code frame.
//!
//! A frame is the offending line(s) with a gutter, a severity-colored caret
//! and underline, and reserved keywords highlighted. It is language-agnostic;
//! callers pass byte offsets into the source plus a [`Language`] selecting the
//! reserved-keyword set. Output is plain ASCII (byte-stable) unless
//! [`Options::color`] is set.

use std::fmt::Write;

use crate::gopath;
use crate::term::{ForceColorWriter, PlainWriter, Styler};

mod group;

pub use group::{Group, render_groups};

/// Selects the headline word and caret color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Severity {
	/// An error.
	#[default]
	Error,
	/// A warning.
	Warning,
}

/// Selects which reserved-keyword set is highlighted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
	/// The 21 Luau reserved words.
	#[default]
	Luau,
	/// TypeScript and JavaScript reserved and contextual keywords.
	TypeScript,
}

/// One diagnostic anchored to a byte span of the source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spot {
	/// Byte offset of the span start into the source.
	pub offset: isize,
	/// Span length in bytes; the caret count is `max(1, len)` clamped to the
	/// line.
	pub len: isize,
	/// How severe the diagnostic is.
	pub severity: Severity,
	/// An optional code shown after the severity: `TS2322`, `noAny`, or empty.
	pub code: String,
	/// The primary message (no suggestion or "More information:" tail).
	pub message: String,
	/// Suggestion and "More information:" lines, rendered as `help:`.
	pub help: Vec<String>,
}

/// Controls presentation. `color` and `link` are decided by the caller from
/// the destination writer ([`crate::term::color_enabled`]); with `color` false
/// the output is plain ASCII and byte-stable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
	/// Context lines above and below the primary line; 0 means the default
	/// (1).
	pub context: isize,
	/// Emit ANSI styling.
	pub color: bool,
	/// Emit an OSC 8 hyperlink on the locator (the caller passes
	/// `color && wanted`).
	pub link: bool,
}

/// The 21 Luau reserved words (mirrors `rotor_luau`'s canonical set; a drift
/// test guards against divergence).
const LUAU_KEYWORDS: &[&str] = &[
	"and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local", "nil", "not", "or",
	"repeat", "return", "then", "true", "until", "while",
];

/// The TypeScript/JavaScript reserved and contextual keyword set used for
/// highlighting (presentation only, not a parser).
const TS_KEYWORDS: &[&str] = &[
	"abstract",
	"any",
	"as",
	"asserts",
	"async",
	"await",
	"boolean",
	"break",
	"case",
	"catch",
	"class",
	"const",
	"continue",
	"debugger",
	"declare",
	"default",
	"delete",
	"do",
	"else",
	"enum",
	"export",
	"extends",
	"false",
	"finally",
	"for",
	"from",
	"function",
	"if",
	"implements",
	"import",
	"in",
	"infer",
	"instanceof",
	"interface",
	"keyof",
	"let",
	"namespace",
	"never",
	"new",
	"null",
	"number",
	"object",
	"of",
	"private",
	"protected",
	"public",
	"readonly",
	"return",
	"static",
	"string",
	"super",
	"switch",
	"this",
	"throw",
	"true",
	"try",
	"type",
	"typeof",
	"undefined",
	"unknown",
	"var",
	"void",
	"while",
	"yield",
];

const TAB_WIDTH: usize = 4;

/// The 1-based line and column (a byte column) for a byte offset, plus the
/// index of the line's first byte. The offset is clamped to `[0, src.len()]`.
/// The column is byte-based, so it can differ from the display column on
/// lines holding multi-byte UTF-8 characters.
fn line_col_at(src: &str, offset: isize) -> (isize, isize, usize) {
	let offset = offset
		.clamp(0, isize::try_from(src.len()).unwrap_or(isize::MAX))
		.unsigned_abs();
	let mut line = 1;
	let mut line_start = 0;
	for (i, &byte) in src.as_bytes()[..offset].iter().enumerate() {
		if byte == b'\n' {
			line += 1;
			line_start = i + 1;
		}
	}
	let col = isize::try_from(offset - line_start).unwrap_or(isize::MAX) + 1;
	(line, col, line_start)
}

/// The text of the given 1-based line with any trailing CR stripped, so CRLF
/// sources render cleanly; empty when `line` is out of range.
fn line_text(src: &str, line: isize) -> &str {
	if line < 1 {
		return "";
	}
	let bytes = src.as_bytes();
	let mut current = 1;
	let mut start = 0;
	for (i, &byte) in bytes.iter().enumerate() {
		if current == line {
			break;
		}
		if byte == b'\n' {
			current += 1;
			start = i + 1;
		}
	}
	if current != line {
		return "";
	}
	let end = bytes[start..]
		.iter()
		.position(|&b| b == b'\n')
		.map_or(bytes.len(), |p| start + p);
	let text = &src[start..end];
	text.strip_suffix('\r').unwrap_or(text)
}

fn is_keyword(word: &str, lang: Language) -> bool {
	match lang {
		Language::TypeScript => TS_KEYWORDS.contains(&word),
		Language::Luau => LUAU_KEYWORDS.contains(&word),
	}
}

const fn is_ident_byte(c: u8) -> bool {
	c == b'_' || c.is_ascii_alphanumeric()
}

/// Colors reserved words in one line of source. A no-color styler returns the
/// line unchanged, so uncolored output is byte-stable. The scan is word
/// boundary based: keywords inside strings and comments may be colored, which
/// is accepted for the keywords-only tier.
fn highlight_keywords(line: &str, lang: Language, styler: Styler) -> String {
	if !styler.color() {
		return line.to_owned();
	}
	let bytes = line.as_bytes();
	let mut out = String::with_capacity(line.len());
	let mut plain_from = 0;
	let mut i = 0;
	while i < bytes.len() {
		let c = bytes[i];
		if c == b'_' || c.is_ascii_alphabetic() {
			let mut j = i + 1;
			while j < bytes.len() && is_ident_byte(bytes[j]) {
				j += 1;
			}
			let word = &line[i..j];
			if is_keyword(word, lang) {
				out.push_str(&line[plain_from..i]);
				out.push_str(&styler.accent(word));
				plain_from = j;
			}
			i = j;
		} else {
			i += 1;
		}
	}
	out.push_str(&line[plain_from..]);
	out
}

/// A styler with color forced on or off: the renderer's single source of
/// truth for the [`Options::color`] decision.
fn styler_for(color: bool) -> Styler {
	if color {
		Styler::for_writer(&ForceColorWriter)
	} else {
		Styler::for_writer(&PlainWriter)
	}
}

/// Whether `p` looks like a Windows drive path (`C:/` or `C:\`). On Windows
/// `IsAbs` already handles this; the check exists so a Windows-style path
/// processed on another host (from a config or a cross-build) is still
/// treated as absolute.
const fn is_windows_drive(p: &str) -> bool {
	let b = p.as_bytes();
	b.len() >= 3 && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\') && b[0].is_ascii_alphabetic()
}

/// Go's `url.URL.EscapedPath` for a path with no `RawPath`.
fn escape_path(path: &str) -> String {
	if path == "*" {
		return "*".to_owned();
	}
	let mut out = String::with_capacity(path.len());
	for &c in path.as_bytes() {
		let keep = c.is_ascii_alphanumeric()
			|| matches!(
				c,
				b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+' | b',' | b'/' | b':' | b';' | b'=' | b'@'
			);
		if keep {
			out.push(char::from(c));
		} else {
			let _ = write!(out, "%{c:02X}");
		}
	}
	out
}

/// A `file://` URI for `p`, resolving relative paths against the working
/// directory and percent-encoding the path. An already-qualified `file://`
/// URI is returned unchanged. Used for OSC 8 hyperlinks so terminals open a
/// real file rather than a bogus network share: building `"file://" + path`
/// from a relative path made VS Code read the first segment as a UNC host
/// and fail with "The network name cannot be found. (0x43)".
fn file_uri(p: &str) -> String {
	if p.is_empty() {
		return String::new();
	}
	if p.starts_with("file://") {
		return p.to_owned();
	}
	let abs = if !gopath::is_abs(p) && !is_windows_drive(p) {
		gopath::abs(p).unwrap_or_else(|_| p.to_owned())
	} else {
		p.to_owned()
	};
	let escaped = escape_path(&gopath::to_slash(&abs));
	if escaped.starts_with("//") {
		// UNC: //server/share/file -> file://server/share/file, not file:////...
		return format!("file:{escaped}");
	}
	if escaped.starts_with('/') {
		return format!("file://{escaped}");
	}
	format!("file:///{escaped}")
}

/// The framed (and, when `o.color`, ANSI-colored) block for one file's spots.
/// Pure: no I/O, so the caller writes the result where it wants.
#[must_use]
pub fn render(path: &str, source: &str, lang: Language, spots: &[Spot], o: Options) -> String {
	let o = Options {
		context: if o.context == 0 { 1 } else { o.context },
		..o
	};
	let styler = styler_for(o.color);
	let mut out = String::new();
	for spot in spots {
		render_spot(&mut out, path, source, lang, spot, o, styler);
	}
	out
}

fn render_spot(out: &mut String, path: &str, source: &str, lang: Language, sp: &Spot, o: Options, s: Styler) {
	let total = count_lines(source);
	let (line, col, _) = line_col_at(source, sp.offset);
	if source.is_empty() || line > total {
		// No usable source line, so fall back to one line.
		let _ = writeln!(out, "{path}:{line}:{col}: {}", sp.message);
		return;
	}

	let (severity_word, paint): (&str, fn(&Styler, &str) -> String) = match sp.severity {
		Severity::Error => ("error", Styler::error),
		Severity::Warning => ("warning", Styler::warn),
	};

	let head = if sp.code.is_empty() {
		format!("{severity_word}: {}", sp.message)
	} else if sp.code.starts_with("TS") {
		format!("{severity_word} {}: {}", sp.code, sp.message)
	} else {
		format!("{severity_word}[{}]: {}", sp.code, sp.message)
	};
	let _ = writeln!(out, "{}", paint(&s, &head));

	let mut loc = format!("{path}:{line}:{col}");
	if o.link {
		let uri = file_uri(path);
		if !uri.is_empty() {
			loc = s.hyperlink(&format!("{uri}#{line}:{col}"), &loc);
		}
	}
	let _ = writeln!(out, "  {} {loc}", s.muted("-->"));

	let last = (line + o.context).min(total);
	let gw = last.to_string().len();
	let gutter_pad = " ".repeat(gw);
	let bar = s.muted("|");

	let _ = writeln!(out, "{gutter_pad} {bar}");

	for ln in (line - o.context)..=last {
		if ln < 1 {
			continue;
		}
		let raw = line_text(source, ln);
		let expanded = expand_tabs(raw);
		let num = s.muted(&format!("{ln:>gw$}"));
		let _ = writeln!(out, "{num} {bar} {}", highlight_keywords(&expanded, lang, s));
		if ln == line {
			let caret_col = expanded_col(raw, col);
			let mut n = sp.len.max(1);
			// Clamp the caret run to the visible line; if the span starts at or
			// past end-of-line (an offset at EOF), show a single caret.
			let max_n = isize::try_from(expanded.len()).unwrap_or(isize::MAX) - (caret_col - 1);
			if max_n < 1 {
				n = 1;
			} else if n > max_n {
				n = max_n;
			}
			// The message is echoed inline at the caret (rustc-style) in
			// addition to the headline above.
			let carets = "^".repeat(n.unsigned_abs());
			let indent = " ".repeat((caret_col - 1).unsigned_abs());
			let caret_line = format!(
				"{gutter_pad} {bar} {indent}{} {}",
				paint(&s, &carets),
				paint(&s, &sp.message)
			);
			let _ = writeln!(out, "{}", caret_line.trim_end_matches(' '));
		}
	}
	for h in &sp.help {
		let _ = writeln!(out, "{gutter_pad} {bar} {}", s.info(&format!("help: {h}")));
	}
}

/// The number of display lines in `src`; a single trailing newline does not
/// add an empty line.
fn count_lines(src: &str) -> isize {
	let newlines = src.bytes().filter(|&b| b == b'\n').count();
	let mut n = isize::try_from(newlines).unwrap_or(isize::MAX) + 1;
	if src.ends_with('\n') {
		n -= 1;
	}
	n.max(1)
}

/// Replaces tabs with spaces up to the next `TAB_WIDTH` stop.
fn expand_tabs(s: &str) -> String {
	if !s.contains('\t') {
		return s.to_owned();
	}
	let mut out = String::with_capacity(s.len() + TAB_WIDTH);
	let mut col = 0;
	for c in s.chars() {
		if c == '\t' {
			let spaces = TAB_WIDTH - (col % TAB_WIDTH);
			out.push_str(&" ".repeat(spaces));
			col += spaces;
		} else {
			out.push(c);
			col += 1;
		}
	}
	out
}

/// Maps a 1-based byte column on the raw line to a 1-based visual column on
/// the tab-expanded line.
fn expanded_col(raw: &str, byte_col: isize) -> isize {
	let mut visual = 0_usize;
	let limit = usize::try_from(byte_col - 1).unwrap_or(0);
	for &byte in raw.as_bytes().iter().take(limit) {
		if byte == b'\t' {
			visual += TAB_WIDTH - (visual % TAB_WIDTH);
		} else {
			visual += 1;
		}
	}
	isize::try_from(visual).unwrap_or(isize::MAX) + 1
}

#[cfg(test)]
mod tests;
