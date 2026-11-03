//! Builds one project through [`rotor_compile::build_project_with_options`], for timing the
//! single-project pipeline against the Go build: `cargo run --release --example build -- <dir>`.

use std::io::Write as _;
use std::process::ExitCode;
use std::time::Instant;

use rotor_compile::{ProjectOptions, build_project_with_options};

use icu_normalizer as _;
use rotor_config as _;
use rotor_luau as _;
use rotor_rojo as _;
use rotor_support as _;
use rotor_transformer as _;
use serde_json as _;
use sha2 as _;
use tempfile as _;

fn main() -> ExitCode {
	let dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_owned());
	let started = Instant::now();
	let result = rotor_tsrs::with_large_stack(|| build_project_with_options(&dir, &ProjectOptions::default()));
	let elapsed = started.elapsed();
	let mut stderr = std::io::stderr().lock();
	match result {
		Ok(build) => {
			let _ = writeln!(
				stderr,
				"built {} outputs in {} ms",
				build.outputs.len(),
				elapsed.as_millis()
			);
			ExitCode::SUCCESS
		}
		Err(failure) => {
			let _ = writeln!(stderr, "{}: {:?}", failure.failure, failure.failure.messages());
			ExitCode::FAILURE
		}
	}
}
