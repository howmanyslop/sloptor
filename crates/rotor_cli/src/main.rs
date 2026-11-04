//! The `sloptor` binary, the Rust port of the rotor CLI (`cmd/rotor`).
//!
//! `CLI.md` maps every Go file to its module here, lists the kebab-case flag names and their Go
//! camelCase aliases, and records every intentional difference from Go.

mod api_server;
mod app;
mod argv;
mod build;
mod buildapi;
mod check;
mod clean;
mod cli;
mod diagnostics;
mod frames;
mod fswatch;
mod gen_docs;
mod json;
mod live;
mod lsp;
mod memory;
mod options;
mod report;
mod runtime;
mod solution_watch;
mod solution_watch_coverage;
mod term;
#[cfg(test)]
mod test_fixtures;
mod ui;
mod watch;
mod watch_json;

use std::process::ExitCode;

#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

/// sloptor's version (`version`).
const VERSION: &str = rotor_support::version::VERSION;

fn main() -> ExitCode {
	let args: Vec<_> = std::env::args_os().skip(1).collect();
	let choice = term::color_choice(&args);
	let (out_terminal, err_terminal) = term::process_terminals();
	let mut out = term::Stdout;
	let mut err = term::Stderr;
	let mut streams = term::Streams {
		out: &mut out,
		err: &mut err,
		out_color: term::color_enabled(choice, out_terminal),
		err_color: term::color_enabled(choice, err_terminal),
	};
	ExitCode::from(app::execute(args, &mut streams))
}
