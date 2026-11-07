//! The command tree: every command and flag of `sloptor`, declared with usage-rs's derive API.
//!
//! Flags are kebab-case. rbxtsc's camelCase spelling of each stays as a hidden alias, so a
//! roblox-ts build script keeps working.
//!
//! A boolean that layers over tsconfig's `rbxts` key (or `rotor.toml`) has three states: not
//! given, on, and off. It is an `Option<bool>` written `--flag`, `--flag=true` or `--flag=false`,
//! with a hidden `--no-flag` partner, the two overriding each other so the last one given wins.
//! Every other boolean is a plain `bool` with a native `--no-flag`.

use std::fmt;
use std::num::NonZeroUsize;
use std::str::FromStr;

const ROOT_AFTER_HELP: &str = "Run 'sloptor <command> --help' for a command's options and examples.";

const ROOT_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor build                       compile the project in the current directory
  sloptor build -b                    build every project reference of ./tsconfig.json
  sloptor check src/client --json     typecheck one project, one JSON result on stdout
  sloptor clean --types --dry-run     list what clean would remove

Environment:
  NO_COLOR, CLICOLOR, CLICOLOR_FORCE  colour policy under --color auto (FORCE_COLOR also forces)
  ROTOR_MAX_BUILDS                    concurrent builds per machine (0 turns admission off)
  RAYON_NUM_THREADS, GOMAXPROCS       checker worker threads (default: CPUs per concurrent build)
  RBXTSC_WRITE_CONCURRENCY            output-write worker override (max 256)
  ROTOR_WRITE_WORKERS                 rotor-specific output-write worker override
  UV_THREADPOOL_SIZE                  Node sidecar libuv pool size (not the output writers)

Exit codes:
  0    success, help and --version
  1    a failure: compile errors, type errors, config errors
       (`diagnostics` exits 0 whenever it produced a census)
  2    a usage error: an unknown command or flag, a bad or missing value
  130  interrupted (Ctrl+C in watch mode)";

const BUILD_AFTER_LONG_HELP: &str = "\
Examples:
  sloptor build                         compile ./tsconfig.json to Luau
  sloptor build src/server              compile the project found from src/server upward
  sloptor build -b tsconfig.solution.json --builders 8
  sloptor build --json > result.json    one machine-readable result object
  sloptor build --no-luau               emit .lua instead of .luau

Booleans accept --flag, --flag=true, --flag=false and --no-flag; the last one wins.
rbxtsc's camelCase spellings (--optimizedLoops, --includePath, ...) are accepted as aliases.";

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
  sloptor completion zsh --install      write the script where zsh looks for it
  sloptor completion fish > ~/.config/fish/completions/sloptor.fish
  sloptor completion powershell | Out-String | Invoke-Expression

The script asks sloptor for candidates on every Tab, so it never goes stale on an upgrade.";

/// `--color`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, usage_rs::ValueEnum)]
pub enum ColorChoice {
	/// Colour on a terminal, honouring `NO_COLOR`, `CLICOLOR` and `CLICOLOR_FORCE`.
	#[default]
	Auto,
	/// Always colour.
	Always,
	/// Never colour.
	Never,
}

/// `--type`
#[derive(Clone, Copy, Debug, PartialEq, Eq, usage_rs::ValueEnum)]
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

/// A count of at least one: `--builders` and `--checkers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Positive(NonZeroUsize);

impl Positive {
	/// The count, in the width the compile options carry.
	#[must_use]
	pub fn get(self) -> i64 {
		i64::try_from(self.0.get()).unwrap_or(i64::MAX)
	}
}

impl FromStr for Positive {
	type Err = NotACount;

	fn from_str(text: &str) -> Result<Self, Self::Err> {
		text.parse().map(Self).map_err(|_| NotACount("a positive integer"))
	}
}

/// A count that may be zero: `--max-errors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonNegative(usize);

impl NonNegative {
	/// The count.
	#[must_use]
	pub const fn get(self) -> usize {
		self.0
	}
}

impl FromStr for NonNegative {
	type Err = NotACount;

	fn from_str(text: &str) -> Result<Self, Self::Err> {
		text.parse().map(Self).map_err(|_| NotACount("a non-negative integer"))
	}
}

/// Why a count was refused: what it has to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotACount(&'static str);

impl fmt::Display for NotACount {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "must be {}", self.0)
	}
}

/// `sloptor`
#[derive(Debug, usage_rs::Cli)]
#[usage(
	bin = "sloptor",
	about = "A native TypeScript-to-Luau compiler for Roblox",
	version = crate::VERSION,
	disable_version_flag,
	unknown_flags = "error",
	completion,
	arg_required_else_help,
	subcommand_value_name = "COMMAND",
	usage = "Usage: sloptor [FLAGS] [COMMAND]",
	// The sections usage lays out; `app` writes the banner above them and the command's trailing
	// text below, so neither is reflowed.
	help_template = "{{usage}}\n\n{{commands}}\n\n{{args}}\n\n{{grouped_flags}}\n\n{{ungrouped_flags}}",
	disable_help_flag,
	after_help = ROOT_AFTER_HELP,
	after_long_help = ROOT_AFTER_LONG_HELP
)]
pub struct Cli {
	/// The command.
	#[usage(subcommand)]
	pub command: Option<Command>,

	/// Print sloptor's version
	#[usage(short = 'v', short = 'V', long, global, action = Version)]
	pub version: bool,

	/// When to use colour
	#[usage(long, global, value_enum, value_name = "WHEN", default = "auto")]
	pub color: ColorChoice,

	/// Print help (see more with '--help')
	#[usage(short = 'h', long, global, action = Help)]
	pub help: bool,
}

/// The commands, in help order.
#[derive(Debug, usage_rs::Subcommands)]
pub enum Command {
	/// Compile the project to Luau (tsconfig outDir + include/)
	#[usage(display_order = 1, after_long_help = BUILD_AFTER_LONG_HELP)]
	Build(Box<BuildCommand>),
	/// Typecheck the project (native, full strictness)
	#[usage(display_order = 2, after_long_help = CHECK_AFTER_LONG_HELP)]
	Check(CheckCommand),
	/// Report every file's outcome instead of stopping at the first failure
	#[usage(display_order = 3)]
	Diagnostics(DiagnosticsCommand),
	/// Remove build outputs; --types also removes generated editor types
	#[usage(display_order = 4, after_long_help = CLEAN_AFTER_LONG_HELP)]
	Clean(CleanCommand),
	/// Run a language server that reports sloptor diagnostics to editors
	#[usage(display_order = 5)]
	Lsp(LspCommand),
	/// Print a JSON Schema to stdout (rotor.toml, or the tsconfig "rbxts" key)
	#[usage(display_order = 6)]
	Schema(SchemaCommand),
	/// Generate a shell completion script
	#[usage(display_order = 7, visible_alias = "completions", after_long_help = COMPLETION_AFTER_LONG_HELP)]
	Completion(usage_rs::completion::Completion),
	/// Print sloptor's version
	#[usage(display_order = 8)]
	Version,
	/// Serve build requests over stdio (for build integrations)
	#[usage(name = "api-server", hide)]
	ApiServer,
	/// Write the CLI reference as MDX pages for the docs site
	#[usage(name = "gen-docs", hide)]
	GenDocs(GenDocsCommand),
}

/// `sloptor build`
#[derive(Debug, usage_rs::Args)]
#[expect(clippy::struct_excessive_bools, reason = "one field per boolean flag")]
pub struct BuildCommand {
	/// Project path: a tsconfig file, a directory containing one, or any path to search upward from
	#[usage(arg, value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub path: Option<String>,

	/// Project path, as the positional [PATH] (default: .)
	#[usage(
		short = 'p',
		long,
		value_name = "PATH",
		value_hint = ValueHint::AnyPath,
		default_missing = "",
		help_heading = "Input"
	)]
	pub project: Option<String>,

	/// Build project references, optionally from the given tsconfig
	#[usage(
		short = 'b',
		long,
		value_name = "PATH",
		value_hint = ValueHint::FilePath,
		default_missing = "",
		help_heading = "Input"
	)]
	pub build: Option<String>,

	/// Rojo project file (default: discovered)
	#[usage(
		long,
		value_name = "PATH",
		value_hint = ValueHint::FilePath,
		default_missing = "",
		help_heading = "Input"
	)]
	pub rojo: Option<String>,

	/// Override the project type
	#[usage(long = "type", value_name = "KIND", value_enum, help_heading = "Input")]
	pub project_type: Option<ProjectKind>,

	/// Folder to copy the runtime files to (default: <project>/include)
	#[usage(
		short = 'i',
		long = "include-path",
		alias = "includePath",
		value_name = "DIR",
		value_hint = ValueHint::DirPath,
		default_missing = "",
		help_heading = "Output"
	)]
	pub include_path: Option<String>,

	/// Do not copy the runtime include files
	#[usage(
		long = "no-include",
		alias = "noInclude",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		help_heading = "Output"
	)]
	pub no_include: Option<bool>,

	/// Skip rewriting outputs whose contents are unchanged
	#[usage(
		long = "write-only-changed",
		alias = "writeOnlyChanged",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-write-only-changed"),
		help_heading = "Output"
	)]
	pub write_only_changed: Option<bool>,
	/// Rewrite every output
	#[usage(long = "no-write-only-changed", hide, overrides("--write-only-changed"))]
	pub no_write_only_changed: bool,

	/// Emit .luau files; --no-luau emits .lua (default: true)
	#[usage(
		long,
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-luau"),
		help_heading = "Emit"
	)]
	pub luau: Option<bool>,
	/// Emit .lua files
	#[usage(long = "no-luau", hide, overrides("--luau"))]
	pub no_luau: bool,

	/// Experimental: emit full Luau type annotations
	#[usage(
		long = "luau-types",
		alias = "luauTypes",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-luau-types"),
		help_heading = "Emit"
	)]
	pub luau_types: Option<bool>,
	/// Emit no Luau type annotations
	#[usage(long = "no-luau-types", hide, overrides("--luau-types"))]
	pub no_luau_types: bool,

	/// Only emit declaration files for a solution build (requires --build)
	#[usage(
		long = "emit-declaration-only",
		alias = "emitDeclarationOnly",
		bool_value,
		help_heading = "Emit"
	)]
	pub emit_declaration_only: bool,

	/// Allow @ts-ignore, @ts-expect-error and @ts-nocheck
	#[usage(
		long = "allow-comment-directives",
		alias = "allowCommentDirectives",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-allow-comment-directives"),
		help_heading = "Emit"
	)]
	pub allow_comment_directives: Option<bool>,
	/// Refuse comment directives
	#[usage(long = "no-allow-comment-directives", hide, overrides("--allow-comment-directives"))]
	pub no_allow_comment_directives: bool,

	/// Log where Lua truthiness changes how a condition evaluates
	#[usage(
		long = "log-truthy-changes",
		alias = "logTruthyChanges",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-log-truthy-changes"),
		help_heading = "Emit"
	)]
	pub log_truthy_changes: Option<bool>,
	/// Do not log truthiness changes
	#[usage(long = "no-log-truthy-changes", hide, overrides("--log-truthy-changes"))]
	pub no_log_truthy_changes: bool,

	/// Not supported by sloptor (parsed and ignored)
	#[usage(
		long = "write-transformed-files",
		alias = "writeTransformedFiles",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-write-transformed-files"),
		help_heading = "Emit"
	)]
	pub write_transformed_files: Option<bool>,
	/// Do not write transformed files
	#[usage(long = "no-write-transformed-files", hide, overrides("--write-transformed-files"))]
	pub no_write_transformed_files: bool,

	/// Numeric for-loop optimization; --no-optimized-loops turns it off (default: true)
	#[usage(
		long = "optimized-loops",
		alias = "optimizedLoops",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-optimized-loops"),
		help_heading = "Optimization"
	)]
	pub optimized_loops: Option<bool>,
	/// Turn the numeric for-loop optimization off
	#[usage(long = "no-optimized-loops", hide, overrides("--optimized-loops"))]
	pub no_optimized_loops: bool,

	/// Replace eligible Array.push calls in loops with indexed writes
	#[usage(
		long = "optimized-array-appends",
		alias = "optimizedArrayAppends",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-optimized-array-appends"),
		help_heading = "Optimization"
	)]
	pub optimized_array_appends: Option<bool>,
	/// Keep Array.push calls as written
	#[usage(long = "no-optimized-array-appends", hide, overrides("--optimized-array-appends"))]
	pub no_optimized_array_appends: bool,

	/// Rebuild whenever an input changes
	#[usage(
		short = 'w',
		long,
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-watch"),
		help_heading = "Watch"
	)]
	pub watch: Option<bool>,
	/// Build once
	#[usage(long = "no-watch", hide, overrides("--watch"))]
	pub no_watch: bool,

	/// Poll for changes instead of using file-system events (requires --watch)
	#[usage(
		long = "use-polling",
		alias = "usePolling",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-use-polling"),
		help_heading = "Watch"
	)]
	pub use_polling: Option<bool>,
	/// Use file-system events
	#[usage(long = "no-use-polling", hide, overrides("--use-polling"))]
	pub no_use_polling: bool,

	/// Ring the terminal bell when a rebuild flips between failing and passing
	#[usage(long, bool_value, negate = "--no-bell", help_heading = "Watch")]
	pub bell: bool,

	/// Clear the screen before each rebuild
	#[usage(long, bool_value, negate = "--no-clear", default = "true", help_heading = "Watch")]
	pub clear: bool,

	/// Print one machine-readable result object instead of styled output (NDJSON with --watch)
	#[usage(long, bool_value, negate = "--no-json", help_heading = "Diagnostics")]
	pub json: bool,

	/// Verbose logs and per-phase timings
	#[usage(
		long,
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-verbose"),
		help_heading = "Diagnostics"
	)]
	pub verbose: Option<bool>,
	/// Quiet logs
	#[usage(long = "no-verbose", hide, overrides("--verbose"))]
	pub no_verbose: bool,

	/// Cap the code frames rendered on failure; 0 renders all (default: 50)
	#[usage(
		long = "max-errors",
		value_name = "N",
		default_missing = "0",
		allow_negative_numbers,
		help_heading = "Diagnostics"
	)]
	pub max_errors: Option<NonNegative>,

	/// Projects to build concurrently (only with --build) (default: 4)
	#[usage(long, value_name = "N", allow_negative_numbers, help_heading = "Runtime")]
	pub builders: Option<Positive>,

	/// Checkers per project (build and check) (default: 4)
	#[usage(long, value_name = "N", allow_negative_numbers, help_heading = "Runtime")]
	pub checkers: Option<Positive>,

	/// Serialize builders and checkers (overrides --builders and --checkers; tsgo-compatible)
	#[usage(
		long = "single-threaded",
		alias = "singleThreaded",
		require_equals,
		default_missing = "true",
		value_name = "BOOL",
		overrides("--no-single-threaded"),
		help_heading = "Runtime"
	)]
	pub single_threaded: Option<bool>,
	/// Let builders and checkers run concurrently
	#[usage(long = "no-single-threaded", hide, overrides("--single-threaded"))]
	pub no_single_threaded: bool,

	/// Write aggregate one-shot build timings as JSON (not with --watch)
	#[usage(long, value_name = "PATH", value_hint = ValueHint::FilePath, help_heading = "Advanced")]
	pub timings: Option<String>,
}

/// `sloptor check`
#[derive(Debug, usage_rs::Args)]
pub struct CheckCommand {
	/// Project directory containing tsconfig.json (default: .)
	#[usage(arg, value_name = "PATH", value_hint = ValueHint::DirPath)]
	pub path: Option<String>,

	/// Re-check whenever an input changes
	#[usage(short = 'w', long, bool_value, negate = "--no-watch")]
	pub watch: bool,

	/// Print one machine-readable result object instead of styled output (NDJSON with --watch)
	#[usage(long, bool_value, negate = "--no-json")]
	pub json: bool,

	/// Checkers per project (default: 4)
	#[usage(long, value_name = "N", allow_negative_numbers)]
	pub checkers: Option<Positive>,
}

/// `sloptor diagnostics`
#[derive(Debug, usage_rs::Args)]
pub struct DiagnosticsCommand {
	/// Project path (default: .)
	#[usage(arg, value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub path: Option<String>,

	/// Project path: a tsconfig file, a directory containing one, or any path to search upward from
	#[usage(short = 'p', long, value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub project: Option<String>,

	/// Census a solution of project references, optionally from the given tsconfig
	#[usage(short = 'b', long, value_name = "PATH", value_hint = ValueHint::FilePath, default_missing = "")]
	pub build: Option<String>,

	/// Projects to census concurrently (only with --build) (default: 4)
	#[usage(long, value_name = "N", allow_negative_numbers)]
	pub builders: Option<Positive>,

	/// Checkers per project (default: 4)
	#[usage(long, value_name = "N", allow_negative_numbers)]
	pub checkers: Option<Positive>,

	/// Print one machine-readable result object instead of styled output
	#[usage(long, bool_value, negate = "--no-json")]
	pub json: bool,
}

/// `sloptor clean`
#[derive(Debug, usage_rs::Args)]
pub struct CleanCommand {
	/// Project path: a tsconfig file, a directory containing one, or any path to search upward from
	#[usage(arg, value_name = "PATH", value_hint = ValueHint::AnyPath)]
	pub path: Option<String>,

	/// Also remove the generated editor types (rotor.d.ts and its legacy companions)
	#[usage(long, bool_value, negate = "--no-types")]
	pub types: bool,

	/// List what would be removed without deleting anything
	#[usage(short = 'n', long = "dry-run", bool_value, negate = "--no-dry-run")]
	pub dry_run: bool,
}

/// `sloptor lsp`
#[derive(Debug, usage_rs::Args)]
pub struct LspCommand {
	/// Use stdio for transport (the only transport; accepted for editor compatibility)
	#[usage(long, bool_value)]
	pub stdio: bool,
}

/// `sloptor schema`
#[derive(Debug, usage_rs::Args)]
pub struct SchemaCommand {
	/// Print the tsconfig.json "rbxts" extension schema instead of the rotor.toml schema
	#[usage(long, bool_value, negate = "--no-rbxts")]
	pub rbxts: bool,
}

/// `sloptor gen-docs`
#[derive(Debug, usage_rs::Args)]
pub struct GenDocsCommand {
	/// The directory to write the pages to
	#[usage(arg, value_name = "DIR", value_hint = ValueHint::DirPath)]
	pub dir: String,
}

#[cfg(test)]
mod tests {
	use super::Cli;

	#[test]
	fn the_spec_round_trips_through_usage() {
		let kdl = Cli::to_kdl();
		if let Err(error) = kdl.parse::<usage_parser::Spec>() {
			panic!("the emitted spec does not parse: {error}\n{kdl}");
		}
	}
}
