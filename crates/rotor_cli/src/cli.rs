//! The command tree: every command and flag of `cmd/rotor`, declared with clap's derive API.
//!
//! Flags are kebab-case; Go's camelCase spelling of each stays as a hidden alias. Booleans take
//! pflag's forms (`--flag`, `--flag=<bool>`, and `--no-flag`, the last rewritten by
//! [`crate::argv`] before clap parses), so every boolean is an `Option<bool>` that is `None` when
//! the flag was not passed. Help renders them as plain flags ([`display_command`]).

use std::ffi::OsString;

use clap::builder::styling::{Ansi256Color, AnsiColor, Color, Style, Styles};
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};

/// The value name every boolean flag carries; [`display_command`] and [`crate::argv`] find the
/// booleans by it.
pub const BOOL_VALUE_NAME: &str = "BOOL";

/// The brand pink of the `sloptor` wordmark.
pub const BRAND: Style = Style::new().bold().fg_color(Some(Color::Ansi256(Ansi256Color(205))));

/// Secondary text.
const MUTED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

/// cargo's help palette, with the brand pink for the usage line.
const STYLES: Styles = Styles::styled()
	.header(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightGreen))))
	.usage(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightGreen))))
	.literal(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightCyan))))
	.placeholder(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan))))
	.error(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightRed))))
	.valid(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightGreen))))
	.invalid(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightYellow))))
	.context(Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack))));

const ROOT_AFTER_HELP: &str = "Run 'sloptor <command> --help' for a command's options and examples.";

const ROOT_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor build                       compile the project in the current directory
  sloptor build -b                    build every project reference of ./tsconfig.json
  sloptor check src/client --json     typecheck one project, one JSON result on stdout
  sloptor clean --types --dry-run     list what clean would remove

Environment:
  NO_COLOR, CLICOLOR, CLICOLOR_FORCE  colour policy under --color auto (FORCE_COLOR also forces)
  RBXTSC_WRITE_CONCURRENCY            output-write worker override (max 256)
  ROTOR_WRITE_WORKERS                 rotor-specific output-write worker override
  UV_THREADPOOL_SIZE                  Node sidecar libuv pool size (not the output writers)

Exit codes:
  0  success
  1  any failure: compile errors, config errors, usage errors (rbxtsc parity)
     `diagnostics` exits 0 whenever it produced a census";

const BUILD_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor build                         compile ./tsconfig.json to Luau
  sloptor build src/server              compile the project found from src/server upward
  sloptor build -b tsconfig.solution.json --builders 8
  sloptor build --json > result.json    one machine-readable result object
  sloptor build --no-luau               emit .lua instead of .luau

Booleans accept --flag, --flag=<bool> and --no-flag. Go's camelCase spellings
(--optimizedLoops, --includePath, ...) are accepted as aliases.";

const CHECK_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor check                         typecheck ./tsconfig.json
  sloptor check packages/core --json    one machine-readable result object
  sloptor check --checkers 8";

const CLEAN_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor clean                         remove outDir and include/
  sloptor clean --types                 also remove rotor.d.ts and its legacy companions
  sloptor clean -n                      list what would be removed";

const COMPLETION_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor completion bash > /etc/bash_completion.d/sloptor
  sloptor completion zsh > \"${fpath[1]}/_sloptor\"
  sloptor completion fish > ~/.config/fish/completions/sloptor.fish
  sloptor completion elvish >> ~/.config/elvish/rc.elv
  sloptor completion powershell | Out-String | Invoke-Expression";

/// `--color`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
	/// Colour on a terminal, honouring `NO_COLOR`, `CLICOLOR` and `CLICOLOR_FORCE`.
	#[default]
	Auto,
	/// Always colour.
	Always,
	/// Never colour.
	Never,
}

/// The shells `completion` generates for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Shell {
	/// Bash
	Bash,
	/// Zsh
	Zsh,
	/// Fish
	Fish,
	/// Elvish
	Elvish,
	/// PowerShell
	Powershell,
}

/// `--type`
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ProjectKind {
	/// A place.
	Game,
	/// A model.
	Model,
	/// A package.
	Package,
}

impl ProjectKind {
	/// The name the tsconfig `rbxts` key and `compile.ProjectType` use.
	#[must_use]
	pub const fn name(self) -> &'static str {
		match self {
			Self::Game => "game",
			Self::Model => "model",
			Self::Package => "package",
		}
	}
}

/// A boolean the way pflag reads one (`strconv.ParseBool`).
///
/// # Errors
///
/// Anything `strconv.ParseBool` rejects.
pub fn go_bool(text: &str) -> Result<bool, String> {
	match text {
		"1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
		"0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
		_ => Err("expected true or false (or 1, 0, t, f)".to_owned()),
	}
}

/// `positiveIntValue`: `--builders` and `--checkers`.
///
/// # Errors
///
/// Zero, a negative number or a non-integer.
pub fn positive_int(text: &str) -> Result<i64, String> {
	match text.parse::<i64>() {
		Ok(value) if value > 0 => Ok(value),
		_ => Err("must be a positive integer".to_owned()),
	}
}

/// `nonNegativeIntValue`: `--max-errors`.
///
/// # Errors
///
/// A negative number or a non-integer.
pub fn non_negative_int(text: &str) -> Result<usize, String> {
	text.parse::<usize>()
		.map_err(|_| "must be a non-negative integer".to_owned())
}

/// `sloptor`
#[derive(Debug, Parser)]
#[command(
	name = "sloptor",
	bin_name = "sloptor",
	about = "A native TypeScript-to-Luau compiler for Roblox",
	disable_version_flag = true,
	args_override_self = true,
	subcommand_value_name = "COMMAND",
	styles = STYLES,
	after_help = ROOT_AFTER_HELP,
	after_long_help = ROOT_AFTER_LONG_HELP
)]
pub struct Cli {
	/// The command.
	#[command(subcommand)]
	pub command: Option<Command>,

	/// Print sloptor's version.
	#[arg(
		short = 'v',
		short_alias = 'V',
		long,
		action = ArgAction::Set,
		num_args = 0..=1,
		require_equals = true,
		default_missing_value = "true",
		value_parser = go_bool,
		value_name = BOOL_VALUE_NAME
	)]
	pub version: Option<bool>,

	/// When to colour output: auto, always or never
	#[arg(
		long,
		global = true,
		value_enum,
		value_name = "WHEN",
		default_value_t = ColorChoice::Auto,
		hide_possible_values = true
	)]
	pub color: ColorChoice,
}

/// The commands, in help order.
#[derive(Debug, Subcommand)]
pub enum Command {
	/// Compile the project to Luau (tsconfig outDir + include/)
	#[command(after_long_help = BUILD_AFTER_LONG_HELP)]
	Build(Box<BuildCommand>),
	/// Typecheck the project (native, full strictness)
	#[command(after_long_help = CHECK_AFTER_LONG_HELP)]
	Check(CheckCommand),
	/// Report every file's outcome instead of stopping at the first failure
	Diagnostics(DiagnosticsCommand),
	/// Remove build outputs; --types also removes generated editor types
	#[command(after_long_help = CLEAN_AFTER_LONG_HELP)]
	Clean(CleanCommand),
	/// Run a language server that reports sloptor diagnostics to editors
	Lsp(LspCommand),
	/// Print a JSON Schema to stdout (rotor.toml, or the tsconfig "rbxts" key)
	Schema(SchemaCommand),
	/// Generate a shell completion script
	#[command(visible_alias = "completions", after_long_help = COMPLETION_AFTER_LONG_HELP)]
	Completion(CompletionCommand),
	/// Print sloptor's version
	#[command(args_override_self = true)]
	Version,
	/// Serve build requests over stdio (for build integrations)
	#[command(name = "api-server", hide = true, args_override_self = true)]
	ApiServer,
	/// Write the CLI reference as MDX pages for the docs site
	#[command(name = "gen-docs", hide = true, args_override_self = true)]
	GenDocs(GenDocsCommand),
}

/// `sloptor build`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct BuildCommand {
	/// Project path: a tsconfig file, a directory containing one, or any path to search upward from
	#[arg(value_name = "PATH", value_hint = ValueHint::AnyPath, help_heading = "Input")]
	pub path: Option<String>,

	/// Project path, as the positional [PATH] [default: .]
	#[arg(
		short = 'p',
		long,
		value_name = "PATH",
		value_hint = ValueHint::AnyPath,
		num_args = 0..=1,
		default_missing_value = "",
		help_heading = "Input"
	)]
	pub project: Option<String>,

	/// Build project references, optionally from the given tsconfig
	#[arg(
		short = 'b',
		long,
		value_name = "PATH",
		value_hint = ValueHint::FilePath,
		num_args = 0..=1,
		default_missing_value = "",
		help_heading = "Input"
	)]
	pub build: Option<String>,

	/// Rojo project file [default: discovered]
	#[arg(
		long,
		value_name = "PATH",
		value_hint = ValueHint::FilePath,
		num_args = 0..=1,
		default_missing_value = "",
		help_heading = "Input"
	)]
	pub rojo: Option<String>,

	/// Override the project type
	#[arg(long = "type", value_name = "KIND", value_enum, help_heading = "Input")]
	pub project_type: Option<ProjectKind>,

	/// Folder to copy the runtime files to [default: <project>/include]
	#[arg(
		short = 'i',
		long = "include-path",
		alias = "includePath",
		value_name = "DIR",
		value_hint = ValueHint::DirPath,
		num_args = 0..=1,
		default_missing_value = "",
		help_heading = "Output"
	)]
	pub include_path: Option<String>,

	/// Do not copy the runtime include files
	#[arg(long = "no-include", alias = "noInclude", action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Output")]
	pub no_include: Option<bool>,

	/// Skip rewriting outputs whose contents are unchanged
	#[arg(long = "write-only-changed", alias = "writeOnlyChanged", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Output")]
	pub write_only_changed: Option<bool>,

	/// Emit .luau files; --no-luau emits .lua [default: true]
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Emit")]
	pub luau: Option<bool>,

	/// Experimental: emit full Luau type annotations
	#[arg(long = "luau-types", alias = "luauTypes", action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Emit")]
	pub luau_types: Option<bool>,

	/// Only emit declaration files for a solution build (requires --build)
	#[arg(long = "emit-declaration-only", alias = "emitDeclarationOnly", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Emit")]
	pub emit_declaration_only: Option<bool>,

	/// Allow @ts-ignore, @ts-expect-error and @ts-nocheck
	#[arg(long = "allow-comment-directives", alias = "allowCommentDirectives", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Emit")]
	pub allow_comment_directives: Option<bool>,

	/// Log where Lua truthiness changes how a condition evaluates
	#[arg(long = "log-truthy-changes", alias = "logTruthyChanges", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Emit")]
	pub log_truthy_changes: Option<bool>,

	/// Not supported by sloptor (parsed and ignored)
	#[arg(long = "write-transformed-files", alias = "writeTransformedFiles", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Emit")]
	pub write_transformed_files: Option<bool>,

	/// Numeric for-loop optimization [default: true]
	#[arg(long = "optimized-loops", alias = "optimizedLoops", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Optimization")]
	pub optimized_loops: Option<bool>,

	/// Replace eligible Array.push calls in loops with indexed writes
	#[arg(long = "optimized-array-appends", alias = "optimizedArrayAppends", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Optimization")]
	pub optimized_array_appends: Option<bool>,

	/// Rebuild whenever an input changes
	#[arg(short = 'w', long, action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Watch")]
	pub watch: Option<bool>,

	/// Poll for changes instead of using file-system events (requires --watch)
	#[arg(long = "use-polling", alias = "usePolling", action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Watch")]
	pub use_polling: Option<bool>,

	/// Ring the terminal bell when a rebuild flips between failing and passing
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Watch")]
	pub bell: Option<bool>,

	/// Clear the screen before each rebuild; --no-clear keeps scrollback [default: true]
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Watch")]
	pub clear: Option<bool>,

	/// Print one machine-readable result object instead of styled output (NDJSON with --watch)
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Diagnostics")]
	pub json: Option<bool>,

	/// Verbose logs and per-phase timings
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME, help_heading = "Diagnostics")]
	pub verbose: Option<bool>,

	/// Cap the code frames rendered on failure; 0 renders all [default: 50]
	#[arg(long = "max-errors", value_name = "N", num_args = 0..=1, default_missing_value = "0",
		value_parser = non_negative_int, allow_negative_numbers = true, help_heading = "Diagnostics")]
	pub max_errors: Option<usize>,

	/// Projects to build concurrently (only with --build) [default: 4]
	#[arg(long, value_name = "N", value_parser = positive_int, allow_negative_numbers = true, help_heading = "Runtime")]
	pub builders: Option<i64>,

	/// Checkers per project (build and check) [default: 4]
	#[arg(long, value_name = "N", value_parser = positive_int, allow_negative_numbers = true, help_heading = "Runtime")]
	pub checkers: Option<i64>,

	/// Serialize builders and checkers (overrides --builders and --checkers; tsgo-compatible)
	#[arg(long = "single-threaded", alias = "singleThreaded", action = ArgAction::Set, num_args = 0..=1,
		require_equals = true, default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME,
		help_heading = "Runtime")]
	pub single_threaded: Option<bool>,

	/// Write aggregate one-shot build timings as JSON (not with --watch)
	#[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub timings: Option<String>,

	/// Go pprof CPU profile; the Rust build reports a clear error (use a sampling profiler)
	#[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub cpuprofile: Option<String>,

	/// Go execution trace; not available in the Rust build
	#[arg(long = "trace-out", value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub trace_out: Option<String>,

	/// Go blocking profile; not available in the Rust build
	#[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub blockprofile: Option<String>,

	/// Go mutex contention profile; not available in the Rust build
	#[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub mutexprofile: Option<String>,

	/// Go heap profile; not available in the Rust build
	#[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub heapprofile: Option<String>,

	/// Print sloptor's version
	#[arg(short = 'v', long, action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub version: Option<bool>,
}

/// `sloptor check`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct CheckCommand {
	/// Project directory containing tsconfig.json [default: .]
	#[arg(value_name = "PATH", value_hint = ValueHint::DirPath)]
	pub path: Option<String>,

	/// Re-check whenever an input changes
	#[arg(short = 'w', long, action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub watch: Option<bool>,

	/// Print one machine-readable result object instead of styled output (NDJSON with --watch)
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub json: Option<bool>,

	/// Checkers per project [default: 4]
	#[arg(long, value_name = "N", value_parser = positive_int, allow_negative_numbers = true)]
	pub checkers: Option<i64>,
}

/// `sloptor diagnostics`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct DiagnosticsCommand {
	/// Project path [default: .]
	#[arg(value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub path: Option<String>,

	/// Project path: a tsconfig file, a directory containing one, or any path to search upward from
	#[arg(short = 'p', long, value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub project: Option<String>,

	/// Census a solution of project references, optionally from the given tsconfig
	#[arg(short = 'b', long, value_name = "PATH", value_hint = ValueHint::FilePath, num_args = 0..=1,
		default_missing_value = "")]
	pub build: Option<String>,

	/// Projects to census concurrently (only with --build) [default: 4]
	#[arg(long, value_name = "N", value_parser = positive_int, allow_negative_numbers = true)]
	pub builders: Option<i64>,

	/// Checkers per project [default: 4]
	#[arg(long, value_name = "N", value_parser = positive_int, allow_negative_numbers = true)]
	pub checkers: Option<i64>,

	/// Print one machine-readable result object instead of styled output
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub json: Option<bool>,
}

/// `sloptor clean`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct CleanCommand {
	/// Project path: a tsconfig file, a directory containing one, or any path to search upward from
	#[arg(value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub path: Option<String>,

	/// Also remove the generated editor types (rotor.d.ts and its legacy companions)
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub types: Option<bool>,

	/// List what would be removed without deleting anything
	#[arg(short = 'n', long = "dry-run", action = ArgAction::Set, num_args = 0..=1, require_equals = true,
		default_missing_value = "true", value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub dry_run: Option<bool>,
}

/// `sloptor lsp`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct LspCommand {
	/// Use stdio for transport (the only transport; accepted for editor compatibility)
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub stdio: Option<bool>,
}

/// `sloptor schema`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct SchemaCommand {
	/// Print the tsconfig.json "rbxts" extension schema instead of the rotor.toml schema
	#[arg(long, action = ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true",
		value_parser = go_bool, value_name = BOOL_VALUE_NAME)]
	pub rbxts: Option<bool>,
}

/// `sloptor completion`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct CompletionCommand {
	/// The shell to generate the script for
	#[arg(value_enum, value_name = "SHELL")]
	pub shell: Shell,
}

/// `sloptor gen-docs`
#[derive(Debug, Args)]
#[command(args_override_self = true)]
pub struct GenDocsCommand {
	/// The directory to write the pages to
	#[arg(value_name = "DIR", value_hint = ValueHint::DirPath)]
	pub dir: String,
}

/// The parse command: what clap parses with.
#[must_use]
pub fn command() -> clap::Command {
	Cli::command()
}

/// The display command: [`command`] with every boolean shown as a plain flag, for help, usage
/// lines and completion scripts. Parsing never uses it.
#[must_use]
pub fn display_command() -> clap::Command {
	let header = format!(
		"{BRAND}sloptor{BRAND:#} {MUTED}v{version}{MUTED:#}  {MUTED}·{MUTED:#}  A native TypeScript-to-Luau compiler for Roblox\n",
		version = crate::VERSION
	);
	let mut command = plain_booleans(command())
		.before_help(header)
		.help_template("{before-help}{usage-heading} {usage}\n\n{all-args}{after-help}");
	command.build();
	general_last(command)
}

/// The heading of the flags every command shares (`--color`, `--version`, `--help`), which render
/// after the command's own.
const GENERAL: &str = "General";

fn general_last(command: clap::Command) -> clap::Command {
	let names: Vec<String> = command
		.get_subcommands()
		.map(|subcommand| subcommand.get_name().to_owned())
		.collect();
	let has_headings = command.get_arguments().any(|arg| arg.get_help_heading().is_some());
	let mut command = command.mut_args(|arg| {
		if has_headings && arg.get_help_heading().is_none() && !arg.is_positional() {
			arg.help_heading(GENERAL)
		} else {
			arg
		}
	});
	for name in names {
		command = command.mut_subcommand(name, general_last);
	}
	command
}

fn plain_booleans(command: clap::Command) -> clap::Command {
	let names: Vec<String> = command
		.get_subcommands()
		.map(|subcommand| subcommand.get_name().to_owned())
		.collect();
	let mut command = command.mut_args(plain_boolean);
	for name in names {
		command = command.mut_subcommand(name, plain_booleans);
	}
	command
}

fn plain_boolean(arg: clap::Arg) -> clap::Arg {
	if !is_bool(&arg) {
		return arg;
	}
	arg.action(ArgAction::SetTrue)
		.num_args(0)
		.require_equals(false)
		.default_missing_value(None)
		.value_parser(clap::value_parser!(bool))
}

/// Whether `arg` is one of the pflag-style booleans.
#[must_use]
pub fn is_bool(arg: &clap::Arg) -> bool {
	arg.get_value_names()
		.is_some_and(|names| names.iter().any(|name| name == BOOL_VALUE_NAME))
}

/// Parses `args` (without the program name).
///
/// # Errors
///
/// clap's error: a usage error, or the help or version request it reports as one.
pub fn parse(args: Vec<OsString>) -> Result<Cli, clap::Error> {
	let mut tokens = Vec::with_capacity(args.len() + 1);
	tokens.push(OsString::from("sloptor"));
	tokens.extend(args);
	Cli::try_parse_from(tokens)
}
