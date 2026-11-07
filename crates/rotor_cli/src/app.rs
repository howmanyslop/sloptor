//! `execute`: the single exit-code owner. argv is parsed by usage and dispatched. Success, help and
//! `--version` exit 0; an operational failure exits 1; a usage error exits 2, whether usage found
//! it while parsing or a command found it afterwards ([`usage`]). Watch mode's Ctrl+C exits 130
//! on its own.

use std::ffi::{OsStr, OsString};
use std::fmt::Display;

use usage_rs::{Error, diagnostic, help};

use crate::cli::{Cli, Command};
use crate::term::Streams;
use crate::ui::Paint;
use crate::{VERSION, argv, build, check, clean, diagnostics, gen_docs};

/// Help in cargo's colours, which sloptor's own output uses too.
const PALETTE: help::Palette = help::Palette::DEFAULT
	.heading("bright-green+bold")
	.option("bright-cyan+bold")
	.metavar("cyan")
	.command("bright-cyan+bold");

/// The exit code of an operational failure.
const FAILED: u8 = 1;

/// The exit code of a usage error.
const MISUSED: u8 = 2;

/// Why a command did not succeed.
#[derive(Debug)]
pub enum Failure {
	/// A usage error found after parsing: the command it belongs to (`""` for the root) and what
	/// is wrong with the command line.
	Usage {
		/// The subcommand whose usage line the error shows.
		command: &'static str,
		/// What is wrong.
		message: String,
	},
	/// An operational failure: one `error:` line.
	Runtime(String),
	/// Already rendered by the command.
	Reported,
}

/// A usage error of `command` (or the root, for `""`).
#[must_use]
pub fn usage(command: &'static str, message: impl Display) -> Failure {
	Failure::Usage {
		command,
		message: message.to_string(),
	}
}

/// A usage error found after parsing, laid out and coloured the way usage lays out its own: the
/// message, the command's usage line, and where to look next.
#[must_use]
pub fn render_usage(command: &str, message: &str, coloured: bool) -> String {
	let paint = |ink: &str, text: &str| {
		if coloured {
			format!("{ink}{text}\x1b[0m")
		} else {
			text.to_owned()
		}
	};
	let root = Cli::spec().root;
	let line = root
		.subcommands()
		.iter()
		.find(|meta| meta.cmd().name == command)
		.map_or_else(
			|| help::usage_line(&["sloptor"], root),
			|meta| help::usage_line(&["sloptor", command], meta),
		);
	format!(
		"{} {message}\n\n{} {}\n\nFor more information, try '{}'.\n",
		paint("\x1b[1m\x1b[31m", "error:"),
		paint("\x1b[1m\x1b[4m", "Usage:"),
		paint("\x1b[1m", &line),
		paint("\x1b[1m", "--help"),
	)
}

/// Runs the CLI over `args` (without the program name) and returns the exit code.
pub fn execute(args: Vec<OsString>, streams: &mut Streams) -> u8 {
	if let Some(answer) = Cli::completion_request(&args) {
		let _ = streams.out.write_all(answer.as_bytes());
		return 0;
	}
	let args = argv::normalize_compiler_invocation(args);
	let tokens: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
	if let Some(spec) = Cli::spec_request(&tokens) {
		let _ = streams.out.write_all(spec.as_bytes());
		return 0;
	}
	let parsed = match Cli::parse_from(&tokens) {
		Ok(parsed) => parsed,
		Err(error) => return answer(&tokens, &error, streams),
	};
	match dispatch(parsed, streams) {
		Ok(()) => 0,
		Err(failure) => report(failure, streams),
	}
}

/// A help page: the banner, usage's sections in the compact layout, and the command's trailing
/// text (`after_long_help` for `--help`) written as authored, its headings coloured like usage's.
fn help_page(cmd: &usage_rs::Command<'_>, long: bool, coloured: bool) -> String {
	let root = Cli::spec().root;
	let meta = std::iter::once(root)
		.chain(root.subcommands().iter().copied())
		.find(|meta| std::ptr::eq(meta.cmd(), cmd))
		.unwrap_or(root);
	let paint = Paint { color: coloured };
	let about = meta.about().unwrap_or_default();
	let name = if std::ptr::eq(meta, root) {
		format!("{} {}", paint.brand("sloptor"), paint.muted(&format!("v{VERSION}")))
	} else {
		paint.brand(&format!("sloptor {}", cmd.name))
	};
	let style = if coloured {
		help::Style::COLOURED.palette(PALETTE)
	} else {
		help::Style::PLAIN
	};
	let mut page = format!("{name}  {}  {about}\n\n", paint.muted("·"));
	page.push_str(&Cli::render_help_styled(cmd, false, style).unwrap_or_default());
	let after = if long {
		meta.after_long_help().or_else(|| meta.after_help())
	} else {
		meta.after_help()
	};
	if let Some(after) = after {
		page.push('\n');
		for line in after.lines() {
			let heading = line.ends_with(':') && !line.starts_with(' ');
			page.push_str(&if heading { paint.green(line) } else { line.to_owned() });
			page.push('\n');
		}
	}
	page
}

/// What parsing stopped on: a help or version request, or a usage error.
fn answer(tokens: &[&OsStr], error: &Error<'_, '_>, streams: &mut Streams) -> u8 {
	match error {
		Error::Help { cmd, long } => {
			let page = help_page(cmd, *long, streams.out_color);
			let _ = streams.out.write_all(page.as_bytes());
			0
		}
		Error::HelpAll { cmd } => {
			let page = help_page(cmd, true, streams.out_color);
			let _ = streams.out.write_all(page.as_bytes());
			0
		}
		Error::Version { .. } => {
			let _ = writeln!(streams.out, "{VERSION}");
			0
		}
		// A bare `sloptor`: the help, on stderr, as the failure it is.
		Error::MissingArgsHelp { cmd } => {
			let page = help_page(cmd, false, streams.err_color);
			let _ = streams.err.write_all(page.as_bytes());
			MISUSED
		}
		_ => {
			let style = if streams.err_color {
				diagnostic::Style::COLOURED
			} else {
				diagnostic::Style::PLAIN
			};
			let text = diagnostic::render(Cli::spec(), tokens, error, style);
			let _ = streams.err.write_all(text.as_bytes());
			MISUSED
		}
	}
}

fn dispatch(parsed: Cli, streams: &mut Streams) -> Result<(), Failure> {
	let Some(command) = parsed.command else {
		return Err(usage("", "a command is required"));
	};
	match command {
		Command::Build(command) => build::run(*command, streams),
		Command::Check(command) => check::run(command, streams),
		Command::Clean(command) => clean::run(command, streams),
		Command::Diagnostics(command) => diagnostics::run(command, streams),
		Command::Lsp(command) => crate::lsp::run(command, streams),
		Command::ApiServer => crate::api_server::run(),
		Command::GenDocs(command) => gen_docs::write(&command.dir).map_err(Failure::Runtime),
		Command::Schema(command) => {
			let schema = if command.rbxts {
				rotor_compile::rbxts_schema::RBXTS_TS_CONFIG_SCHEMA
			} else {
				rotor_config::config::SCHEMA
			};
			streams
				.out
				.write_all(schema.as_bytes())
				.map_err(|error| Failure::Runtime(format!("schema write failed: {error}")))
		}
		Command::Completion(command) => command
			.run("sloptor", &mut streams.out)
			.map_err(|error| Failure::Runtime(error.to_string())),
		Command::Version => {
			let _ = writeln!(streams.out, "{VERSION}");
			Ok(())
		}
	}
}

/// Renders a failure on stderr and returns its exit code.
fn report(failure: Failure, streams: &mut Streams) -> u8 {
	match failure {
		Failure::Usage { command, message } => {
			let text = render_usage(command, &message, streams.err_color);
			let _ = streams.err.write_all(text.as_bytes());
			MISUSED
		}
		Failure::Runtime(message) => {
			let err = streams.err_paint();
			let _ = writeln!(streams.err, "{} {message}", err.red("error:"));
			FAILED
		}
		Failure::Reported => FAILED,
	}
}
