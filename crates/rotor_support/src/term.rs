//! rotor's terminal presentation layer: ANSI color and style helpers, a small
//! set of semantic styles (success, error, warn, ...), and glyphs for the
//! CLI's human-facing output.
//!
//! Color is gated exactly once per writer on the usual heuristics: `NO_COLOR`
//! disables, `FORCE_COLOR` forces, and otherwise color is on only when the
//! writer is an interactive terminal. Piped or redirected output stays plain
//! ASCII and byte-stable.
//!
//! This module governs only rotor's own chrome (headers, summaries, progress).
//! The upstream-compatible stdout channel (compiler warnings, `--verbose`
//! benchmark lines) lives elsewhere and is deliberately left untouched so
//! differential tests keep comparing exact bytes.

use std::fs::File;
use std::io::{self, Stderr, StderrLock, Stdout, StdoutLock};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";

const FG_RED: &str = "\x1b[31m";
const FG_GREEN: &str = "\x1b[32m";
const FG_YELLOW: &str = "\x1b[33m";
const FG_BLUE: &str = "\x1b[34m";
const FG_MAGENTA: &str = "\x1b[35m";
const FG_CYAN: &str = "\x1b[36m";
const FG_GRAY: &str = "\x1b[90m";

const FG_BRIGHT_GREEN: &str = "\x1b[92m";
const FG_BRIGHT_CYAN: &str = "\x1b[96m";

/// Glyphs used across the CLI. When color is disabled (for example a pipe)
/// the ASCII fallbacks are used so logs stay readable in plain files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyphs {
	/// A success mark.
	pub check: &'static str,
	/// A failure mark.
	pub cross: &'static str,
	/// A warning mark.
	pub warn: &'static str,
	/// An arrow.
	pub arrow: &'static str,
	/// A list bullet.
	pub bullet: &'static str,
	/// A separator dot.
	pub dot: &'static str,
	/// A spark, for fast paths.
	pub spark: &'static str,
	/// A watch mark, for pending work.
	pub watch: &'static str,
}

const UNICODE_GLYPHS: Glyphs = Glyphs {
	check: "✓",
	cross: "✗",
	warn: "▲",
	arrow: "→",
	bullet: "•",
	dot: "·",
	spark: "⚡",
	watch: "◷",
};

const ASCII_GLYPHS: Glyphs = Glyphs {
	check: "ok",
	cross: "x",
	warn: "!",
	arrow: "->",
	bullet: "*",
	dot: "-",
	spark: "*",
	watch: "~",
};

/// Renders styled text for a specific writer. Construct one with
/// [`Styler::for_writer`] and reuse it; every style method is a no-op
/// passthrough when color is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Styler {
	color: bool,
}

macro_rules! styles {
	($($(#[$doc:meta])* $name:ident => $codes:expr;)*) => {
		$(
			$(#[$doc])*
			#[must_use]
			pub fn $name(&self, text: &str) -> String {
				self.wrap($codes, text)
			}
		)*
	};
}

impl Styler {
	/// A styler whose color enablement is decided once for `writer`.
	#[must_use]
	pub fn for_writer(writer: &impl ColorSink) -> Self {
		Self {
			color: color_enabled(writer),
		}
	}

	/// Whether this styler emits ANSI codes.
	#[must_use]
	pub const fn color(&self) -> bool {
		self.color
	}

	/// The glyph set for this styler: Unicode when color is on, ASCII
	/// fallbacks otherwise.
	#[must_use]
	pub const fn glyphs(&self) -> Glyphs {
		if self.color { UNICODE_GLYPHS } else { ASCII_GLYPHS }
	}

	fn wrap(self, codes: &str, text: &str) -> String {
		if !self.color || text.is_empty() {
			return text.to_owned();
		}
		format!("{codes}{text}{RESET}")
	}

	styles! {
		/// Bright green: something succeeded.
		success => FG_BRIGHT_GREEN;
		/// Red: something failed.
		error => FG_RED;
		/// Yellow: something deserves attention.
		warn => FG_YELLOW;
		/// Bright cyan: neutral information.
		info => FG_BRIGHT_CYAN;
		/// Magenta: emphasis.
		accent => FG_MAGENTA;
		/// Gray: secondary text.
		muted => FG_GRAY;
		/// Bold.
		bold => BOLD;
		/// Dim.
		dim => DIM;
		/// Italic.
		italic => ITALIC;
		/// Underlined.
		underline => UNDERLINE;
		/// Plain red.
		red => FG_RED;
		/// Plain green.
		green => FG_GREEN;
		/// Plain yellow.
		yellow => FG_YELLOW;
		/// Plain blue.
		blue => FG_BLUE;
		/// Plain cyan.
		cyan => FG_CYAN;
		/// Plain gray.
		gray => FG_GRAY;
	}

	/// Bold bright green in one wrap, such as a headline, without doubling
	/// the reset sequence.
	#[must_use]
	pub fn success_bold(&self, text: &str) -> String {
		self.wrap(&format!("{BOLD}{FG_BRIGHT_GREEN}"), text)
	}

	/// Bold red in one wrap.
	#[must_use]
	pub fn error_bold(&self, text: &str) -> String {
		self.wrap(&format!("{BOLD}{FG_RED}"), text)
	}

	/// Bold yellow in one wrap.
	#[must_use]
	pub fn warn_bold(&self, text: &str) -> String {
		self.wrap(&format!("{BOLD}{FG_YELLOW}"), text)
	}

	/// Wraps `text` in an OSC 8 terminal hyperlink when color is enabled, so
	/// file paths in supporting terminals are clickable. Plain text
	/// otherwise.
	#[must_use]
	pub fn hyperlink(&self, uri: &str, text: &str) -> String {
		if !self.color {
			return text.to_owned();
		}
		format!("\x1b]8;;{uri}\x1b\\{text}\x1b]8;;\x1b\\")
	}

	/// Renders `values` as a one-line sparkline scaled to the largest value
	/// (a Unicode bar ramp when color is on, an ASCII density ramp otherwise).
	#[must_use]
	#[expect(
		clippy::cast_possible_truncation,
		clippy::cast_sign_loss,
		clippy::cast_precision_loss,
		reason = "the ramp index is a small non-negative float rounded down, exactly as in Go"
	)]
	pub fn spark(&self, values: &[f64]) -> String {
		if values.is_empty() {
			return String::new();
		}
		let ramp: Vec<char> = if self.color {
			"▁▂▃▄▅▆▇█"
		} else {
			".:-=+*#%"
		}
		.chars()
		.collect();
		let max = values.iter().copied().fold(0.0, |max, v| if v > max { v } else { max });
		values
			.iter()
			.map(|&v| {
				let index = if max > 0.0 && v > 0.0 {
					((v / max).mul_add((ramp.len() - 1) as f64, 0.5) as usize).min(ramp.len() - 1)
				} else {
					0
				};
				ramp[index]
			})
			.collect()
	}

	/// A horizontal rule of `width` using box-drawing characters when color is
	/// on and ASCII dashes otherwise.
	#[must_use]
	pub fn rule(&self, width: usize) -> String {
		let ch = if self.color { "─" } else { "-" };
		self.muted(&ch.repeat(width))
	}
}

/// A writer that discards everything and forces color on, for building a
/// [`Styler`] without consulting the environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct ForceColorWriter;

impl io::Write for ForceColorWriter {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		Ok(buf.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

/// A writer that discards everything and forces color off, for building a
/// [`Styler`] without consulting the environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainWriter;

impl io::Write for PlainWriter {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		Ok(buf.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

/// A destination [`color_enabled`] can decide color for.
///
/// Only files, the standard streams and the two marker writers can ever be
/// colored; every other writer reports plain output.
pub trait ColorSink {
	/// `Some(true)` or `Some(false)` when the writer settles the question
	/// without consulting the environment.
	fn forced(&self) -> Option<bool> {
		None
	}

	/// Whether the writer is a character device, which is what an
	/// interactive terminal is.
	fn is_char_device(&self) -> bool {
		false
	}

	/// Windows only: switches the console behind the writer into
	/// virtual-terminal mode so ANSI sequences are interpreted rather than
	/// printed.
	fn enable_virtual_terminal(&self) {}
}

impl ColorSink for ForceColorWriter {
	fn forced(&self) -> Option<bool> {
		Some(true)
	}
}

impl ColorSink for PlainWriter {
	fn forced(&self) -> Option<bool> {
		Some(false)
	}
}

impl ColorSink for Vec<u8> {}

impl<T: ColorSink + ?Sized> ColorSink for &T {
	fn forced(&self) -> Option<bool> {
		(**self).forced()
	}

	fn is_char_device(&self) -> bool {
		(**self).is_char_device()
	}

	fn enable_virtual_terminal(&self) {
		(**self).enable_virtual_terminal();
	}
}

impl<T: ColorSink + ?Sized> ColorSink for &mut T {
	fn forced(&self) -> Option<bool> {
		(**self).forced()
	}

	fn is_char_device(&self) -> bool {
		(**self).is_char_device()
	}

	fn enable_virtual_terminal(&self) {
		(**self).enable_virtual_terminal();
	}
}

#[cfg(unix)]
fn fd_is_char_device(fd: std::os::fd::BorrowedFd<'_>) -> bool {
	use std::os::unix::fs::FileTypeExt;
	fd.try_clone_to_owned()
		.and_then(|owned| File::from(owned).metadata())
		.is_ok_and(|info| info.file_type().is_char_device())
}

#[cfg(windows)]
mod console {
	use std::collections::HashSet;
	use std::os::windows::io::{AsRawHandle, BorrowedHandle};
	use std::sync::{LazyLock, Mutex};

	use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_CHAR, GetFileType};
	use windows_sys::Win32::System::Console::{ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, SetConsoleMode};

	/// Handles already configured, so each is configured at most once.
	static CONFIGURED: LazyLock<Mutex<HashSet<usize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

	#[expect(unsafe_code, reason = "GetFileType takes only a handle that the borrow keeps open")]
	pub fn is_char_device(handle: BorrowedHandle<'_>) -> bool {
		// SAFETY: the handle is open for the duration of the borrow.
		unsafe { GetFileType(handle.as_raw_handle()) == FILE_TYPE_CHAR }
	}

	#[expect(
		unsafe_code,
		reason = "the console mode calls take only a handle that the borrow keeps open"
	)]
	pub fn enable_virtual_terminal(handle: BorrowedHandle<'_>) {
		let raw = handle.as_raw_handle();
		let first = CONFIGURED
			.lock()
			.is_ok_and(|mut configured| configured.insert(raw as usize));
		if !first {
			return;
		}
		let mut mode = 0;
		// SAFETY: the handle is open for the duration of the borrow and `mode`
		// is a valid out pointer. Both calls fail harmlessly for non-consoles.
		unsafe {
			if GetConsoleMode(raw, &raw mut mode) != 0 {
				SetConsoleMode(raw, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
			}
		}
	}
}

macro_rules! impl_file_sink {
	($($ty:ty),* $(,)?) => {
		$(
			impl ColorSink for $ty {
				#[cfg(unix)]
				fn is_char_device(&self) -> bool {
					use std::os::fd::AsFd;
					fd_is_char_device(self.as_fd())
				}

				#[cfg(windows)]
				fn is_char_device(&self) -> bool {
					use std::os::windows::io::AsHandle;
					console::is_char_device(self.as_handle())
				}

				#[cfg(windows)]
				fn enable_virtual_terminal(&self) {
					use std::os::windows::io::AsHandle;
					console::enable_virtual_terminal(self.as_handle());
				}
			}
		)*
	};
}

impl_file_sink!(File, Stdout, Stderr, StdoutLock<'_>, StderrLock<'_>);

/// Whether ANSI color should be written to `writer`.
///
/// `NO_COLOR` (any value) disables; `FORCE_COLOR` (any value) forces;
/// otherwise color is on only when `writer` is a character device (an
/// interactive terminal). This mirrors the kleur/standard heuristic the rest
/// of the toolchain follows.
#[must_use]
pub fn color_enabled(writer: &impl ColorSink) -> bool {
	if let Some(forced) = writer.forced() {
		return forced;
	}
	if std::env::var_os("NO_COLOR").is_some() {
		return false;
	}
	if std::env::var_os("FORCE_COLOR").is_some() {
		writer.enable_virtual_terminal();
		return true;
	}
	if !writer.is_char_device() {
		return false;
	}
	writer.enable_virtual_terminal();
	true
}

/// The width of `s` as displayed: the number of characters outside ANSI SGR
/// sequences (everything from an escape up to and including the next `m`).
#[must_use]
pub fn visible_len(s: &str) -> usize {
	let mut n = 0;
	let mut in_esc = false;
	for c in s.chars() {
		if in_esc {
			if c == 'm' {
				in_esc = false;
			}
		} else if c == '\x1b' {
			in_esc = true;
		} else {
			n += 1;
		}
	}
	n
}

#[cfg(test)]
mod tests;
