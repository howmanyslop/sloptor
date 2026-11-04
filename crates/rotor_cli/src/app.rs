//! `execute`: the single exit-code owner. argv is normalized, parsed by clap, and dispatched;
//! every failure, from clap's usage errors to a failed build, exits 1 (rbxtsc's policy), and
//! success, help and `--version` exit 0.

use std::ffi::OsString;
use std::fmt::Display;

use clap::error::{ContextKind, ContextValue, ErrorKind};

use crate::cli::{self, Cli, Command, Shell};
use crate::term::Streams;
use crate::{VERSION, argv, build, check, clean, diagnostics, gen_docs};

/// A failure that ends the command with exit code 1.
#[derive(Debug)]
pub enum Failure {
	/// A usage error, rendered by clap.
	Usage(Box<clap::Error>),
	/// An operational failure: one `error:` line.
	Runtime(String),
	/// Already rendered by the command.
	Reported,
}

/// A usage error of `subcommand` (or the root, for `""`), rendered with its usage line.
#[must_use]
pub fn usage(subcommand: &str, message: impl Display) -> Failure {
	let mut display = cli::display_command();
	let error = match display.find_subcommand_mut(subcommand) {
		Some(found) => found.error(ErrorKind::ArgumentConflict, message),
		None => display.error(ErrorKind::ArgumentConflict, message),
	};
	Failure::Usage(Box::new(error))
}

/// The failure of a command, or a mode of one, that Stage 7b ports.
#[must_use]
pub fn not_ported(what: &str) -> Failure {
	Failure::Runtime(format!(
		"`sloptor {what}` is not ported to the Rust CLI yet (Stage 7b); use the Go build of sloptor for it"
	))
}

/// Runs the CLI over `args` (without the program name) and returns the exit code.
pub fn execute(args: Vec<OsString>, streams: &mut Streams) -> u8 {
	// A bare `sloptor` shows the root help on stderr and fails, keeping Go's exit code 1 for
	// "no command given".
	if args.is_empty() {
		let help = cli::display_command().render_help();
		let _ = streams
			.err
			.write_all(render_styled(&help, streams.err_color).as_bytes());
		return 1;
	}
	let root = cli::command();
	let args = argv::normalize_negations(
		&root,
		argv::normalize_compiler_invocation(argv::normalize_help_command(args)),
	);
	let subcommand = argv::resolve_subcommand(&root, &args);
	let long_help = wants_long_help(&args);
	let parsed = match cli::parse(args) {
		Ok(parsed) => parsed,
		Err(error) => return clap_error(error, subcommand.as_deref(), long_help, streams),
	};
	match dispatch(parsed, streams) {
		Ok(()) => 0,
		Err(failure) => {
			report(failure, streams);
			1
		}
	}
}

/// Whether `--help` (long help) rather than `-h` was asked for.
fn wants_long_help(args: &[OsString]) -> bool {
	args.iter()
		.filter_map(|arg| arg.to_str())
		.take_while(|arg| *arg != "--")
		.any(|arg| arg == "--help")
}

fn dispatch(parsed: Cli, streams: &mut Streams) -> Result<(), Failure> {
	if parsed.version == Some(true) {
		let _ = writeln!(streams.out, "{VERSION}");
		return Ok(());
	}
	let Some(command) = parsed.command else {
		let mut display = cli::display_command();
		return Err(Failure::Usage(Box::new(
			display.error(ErrorKind::MissingSubcommand, "a command is required"),
		)));
	};
	match command {
		Command::Build(command) => build::run(*command, streams),
		Command::Check(command) => check::run(command, streams),
		Command::Clean(command) => clean::run(command, streams),
		Command::Diagnostics(command) => diagnostics::run(command, streams),
		Command::Lsp(_) => Err(not_ported("lsp")),
		Command::ApiServer => Err(not_ported("api-server")),
		Command::GenDocs(command) => gen_docs::write(&command.dir).map_err(Failure::Runtime),
		Command::Schema(command) => {
			let schema = if command.rbxts == Some(true) {
				rotor_compile::rbxts_schema::RBXTS_TS_CONFIG_SCHEMA
			} else {
				rotor_config::config::SCHEMA
			};
			streams
				.out
				.write_all(schema.as_bytes())
				.map_err(|error| Failure::Runtime(format!("schema write failed: {error}")))
		}
		Command::Completion(command) => {
			let shell = match command.shell {
				Shell::Bash => clap_complete::Shell::Bash,
				Shell::Zsh => clap_complete::Shell::Zsh,
				Shell::Fish => clap_complete::Shell::Fish,
				Shell::Elvish => clap_complete::Shell::Elvish,
				Shell::Powershell => clap_complete::Shell::PowerShell,
			};
			clap_complete::generate(shell, &mut cli::display_command(), "sloptor", streams.out);
			Ok(())
		}
		Command::Version => {
			let _ = writeln!(streams.out, "{VERSION}");
			Ok(())
		}
	}
}

/// Renders a failure on stderr.
fn report(failure: Failure, streams: &mut Streams) {
	match failure {
		Failure::Usage(error) => {
			let text = render_styled(&error.render(), streams.err_color);
			let _ = streams.err.write_all(text.as_bytes());
		}
		Failure::Runtime(message) => {
			let err = streams.err_paint();
			let _ = writeln!(streams.err, "{} {message}", err.red("error:"));
		}
		Failure::Reported => {}
	}
}

fn render_styled(text: &clap::builder::StyledStr, color: bool) -> String {
	if color {
		text.ansi().to_string()
	} else {
		text.to_string()
	}
}

/// Help, version requests and usage errors from clap.
fn clap_error(mut error: clap::Error, subcommand: Option<&str>, long_help: bool, streams: &mut Streams) -> u8 {
	let display = cli::display_command();
	let mut target = subcommand
		.and_then(|name| display.find_subcommand(name).cloned())
		.unwrap_or(display);
	match error.kind() {
		ErrorKind::DisplayHelp => {
			// `--help` keeps `-h`'s compact layout and adds the examples, environment and exit
			// codes; clap's long layout puts every description on its own line.
			let help = match target.get_after_long_help().filter(|_| long_help).cloned() {
				Some(after) => target.after_help(after).render_help(),
				None => target.render_help(),
			};
			let _ = streams
				.out
				.write_all(render_styled(&help, streams.out_color).as_bytes());
			0
		}
		ErrorKind::DisplayVersion => {
			let _ = writeln!(streams.out, "{VERSION}");
			0
		}
		_ => {
			// The parse command shows booleans as `--flag[=<BOOL>]`; the usage line comes from
			// the display command, and a suggestion names the kebab flag rather than an alias.
			error.insert(ContextKind::Usage, ContextValue::StyledStr(target.render_usage()));
			if let Some(ContextValue::String(invalid)) = error.get(ContextKind::InvalidArg).cloned()
				&& let Some(plain) = invalid.strip_suffix("[=<BOOL>]")
			{
				error.insert(ContextKind::InvalidArg, ContextValue::String(plain.to_owned()));
			}
			if let Some(ContextValue::String(suggested)) = error.get(ContextKind::SuggestedArg).cloned()
				&& let Some(primary) = primary_long(&target, &suggested)
			{
				error.insert(ContextKind::SuggestedArg, ContextValue::String(primary));
			}
			let text = render_styled(&error.render(), streams.err_color);
			let _ = streams.err.write_all(text.as_bytes());
			1
		}
	}
}

/// The primary `--long` of the flag `spelling` (`--optimizedLoops`) names, by any of its names.
fn primary_long(command: &clap::Command, spelling: &str) -> Option<String> {
	let name = spelling.strip_prefix("--")?;
	command
		.get_arguments()
		.find(|arg| {
			arg.get_long() == Some(name) || arg.get_all_aliases().into_iter().flatten().any(|alias| alias == name)
		})
		.and_then(clap::Arg::get_long)
		.map(|long| format!("--{long}"))
}
