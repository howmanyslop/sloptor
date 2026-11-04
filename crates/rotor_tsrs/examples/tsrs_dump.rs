//! Writes the Rust side of the differential for one project (see `rotor_tsrs::dump`).
//!
//! Usage: `cargo run --profile fastrel -p rotor_tsrs --example tsrs_dump -- <tsconfig> <out>`

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use rotor_tsrs::dump::dump_project;
use rotor_tsrs::{ProjectOptions, load_project, with_large_stack};

// An example is handed every dependency of its package, so the ones only the library uses must be
// acknowledged to keep `unused_crate_dependencies` quiet.
use regex as _;
use rotor_support as _;
use serde_json as _;
use tempfile as _;
use tsrs_ast as _;
use tsrs_binder as _;
use tsrs_checker as _;
use tsrs_compiler as _;
use tsrs_contentmapper as _;
use tsrs_core as _;
use tsrs_diagnostics as _;
use tsrs_module as _;
use tsrs_parser as _;
use tsrs_printer as _;
use tsrs_scanner as _;
use tsrs_sourcemap as _;
use tsrs_tsoptions as _;
use tsrs_vfs as _;

fn main() -> ExitCode {
	let mut arguments = std::env::args_os().skip(1);
	let (Some(tsconfig), Some(out)) = (arguments.next(), arguments.next()) else {
		let _ = writeln!(std::io::stderr(), "usage: tsrs_dump <tsconfig> <out>");
		return ExitCode::from(2);
	};
	let tsconfig = PathBuf::from(tsconfig);
	let options = ProjectOptions {
		checkers: Some(1),
		..ProjectOptions::default()
	};
	let dumped = with_large_stack(|| load_project(&tsconfig, &options).map(dump_project));
	let lines = match dumped {
		Ok(lines) => lines,
		Err(error) => {
			let _ = writeln!(std::io::stderr(), "{error}");
			return ExitCode::FAILURE;
		}
	};
	let mut text = lines.join("\n");
	text.push('\n');
	if let Err(error) = std::fs::write(&out, text) {
		let _ = writeln!(std::io::stderr(), "{error}");
		return ExitCode::FAILURE;
	}
	ExitCode::SUCCESS
}
