//! The CLI's one writer edge: the process streams, and whether each gets colour.
//!
//! Every command writes through [`Streams`]; nothing else in the crate touches the process's
//! stdout or stderr (`logservice`, the upstream-compatible compiler channel, owns its own).

use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};

use rotor_support::term::{ForceColorWriter, PlainWriter, Styler};

use crate::cli::ColorChoice;

/// The streams a command writes to, and whether each is coloured.
pub struct Streams<'a> {
	/// Standard output.
	pub out: &'a mut dyn Write,
	/// Standard error.
	pub err: &'a mut dyn Write,
	/// Whether stdout gets colour.
	pub out_color: bool,
	/// Whether stderr gets colour.
	pub err_color: bool,
}

impl Streams<'_> {
	/// The styler for stdout.
	#[must_use]
	pub fn out_styler(&self) -> Styler {
		styler(self.out_color)
	}

	/// The styler for stderr.
	#[must_use]
	pub fn err_styler(&self) -> Styler {
		styler(self.err_color)
	}
}

/// A styler that colours exactly when `color`.
#[must_use]
pub fn styler(color: bool) -> Styler {
	if color {
		Styler::for_writer(&ForceColorWriter)
	} else {
		Styler::for_writer(&PlainWriter)
	}
}

/// The process's stdout. Each write takes the lock for that write only: the build logs from other
/// threads through `logservice`, which takes the same lock.
pub struct Stdout;

impl Write for Stdout {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		io::stdout().write(buf)
	}

	fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
		io::stdout().write_all(buf)
	}

	fn flush(&mut self) -> io::Result<()> {
		io::stdout().flush()
	}
}

/// The process's stderr.
pub struct Stderr;

impl Write for Stderr {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		io::stderr().write(buf)
	}

	fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
		io::stderr().write_all(buf)
	}

	fn flush(&mut self) -> io::Result<()> {
		io::stderr().flush()
	}
}

/// Whether the process's stdout and stderr are terminals.
#[must_use]
pub fn process_terminals() -> (bool, bool) {
	(io::stdout().is_terminal(), io::stderr().is_terminal())
}

fn env_set(name: &str) -> Option<String> {
	std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Whether a stream gets colour under `choice`: `never` and `always` decide; `auto` is off under
/// `NO_COLOR`, on under `CLICOLOR_FORCE` or `FORCE_COLOR` (other than `0`), off under
/// `CLICOLOR=0`, and otherwise on exactly for a terminal.
#[must_use]
pub fn color_enabled(choice: ColorChoice, terminal: bool) -> bool {
	match choice {
		ColorChoice::Always => true,
		ColorChoice::Never => false,
		ColorChoice::Auto => {
			if env_set("NO_COLOR").is_some() {
				return false;
			}
			if ["CLICOLOR_FORCE", "FORCE_COLOR"]
				.iter()
				.any(|name| env_set(name).is_some_and(|value| value != "0"))
			{
				return true;
			}
			if env_set("CLICOLOR").as_deref() == Some("0") {
				return false;
			}
			terminal && env_set("TERM").as_deref() != Some("dumb")
		}
	}
}

/// The `--color` choice of `args`, read before clap parses so a usage error is coloured by it too.
/// The last occurrence wins; an invalid value is `auto` (clap reports it).
#[must_use]
pub fn color_choice(args: &[OsString]) -> ColorChoice {
	let mut choice = ColorChoice::Auto;
	let mut tokens = args.iter().filter_map(|arg| arg.to_str());
	while let Some(token) = tokens.next() {
		if token == "--" {
			break;
		}
		let value = if token == "--color" {
			tokens.next()
		} else {
			token.strip_prefix("--color=")
		};
		if let Some(value) = value {
			choice = match value {
				"always" => ColorChoice::Always,
				"never" => ColorChoice::Never,
				_ => ColorChoice::Auto,
			};
		}
	}
	choice
}
