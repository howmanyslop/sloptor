//! Replays the spike's checker queries through `rotor_tsrs`.
//!
//! The default test runs on the small conformance project in this repo. The ignored one reads
//! `ROTOR_BENCH_PROJECT` (a tsconfig path) and prints its numbers; run it with
//! `cargo nextest run --cargo-profile fastrel -p rotor_tsrs --run-ignored only --no-capture`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rotor_tsrs::replay::{Counts, project_files, replay_basic, replay_rotor_apis};
use rotor_tsrs::{Project, ProjectOptions, load_program, load_project, with_large_stack};

// An integration test is handed every dependency of its package, so the ones only the library
// uses must be acknowledged to keep `unused_crate_dependencies` quiet.
use regex as _;
use rotor_support as _;
use serde_json as _;
use tempfile as _;
use tsrs_ast as _;
use tsrs_checker as _;
use tsrs_compiler as _;
use tsrs_core as _;
use tsrs_diagnostics as _;
use tsrs_module as _;
use tsrs_parser as _;
use tsrs_printer as _;
use tsrs_scanner as _;
use tsrs_sourcemap as _;
use tsrs_tsoptions as _;
use tsrs_vfs as _;

fn conformance_tsconfig() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/conformance/project/tsconfig.json")
}

struct Replayed {
	files: usize,
	basic: Counts,
	rotor: Counts,
}

fn replay_project(tsconfig: &Path) -> Result<Replayed, String> {
	with_large_stack(|| {
		let Project { program } =
			load_project(tsconfig, &ProjectOptions::default()).map_err(|error| error.to_string())?;
		let files = project_files(program);
		let basic = replay_basic(program, &files).ok_or("program has no checker pool")?;
		let rotor = replay_rotor_apis(program, &files).ok_or("program has no checker pool")?;
		Ok(Replayed {
			files: files.len(),
			basic,
			rotor,
		})
	})
}

#[test]
fn loads_the_conformance_project() {
	let summary = load_program(&conformance_tsconfig()).expect("conformance tsconfig loads");
	assert!(summary.source_files > 0, "no source files in the program");
}

#[test]
fn replays_checker_queries_without_panicking() {
	let replayed = replay_project(&conformance_tsconfig()).expect("conformance project replays");
	assert!(replayed.files > 0, "no project files to walk");
	assert_eq!(replayed.basic.get("panicked_files"), 0);
	assert_eq!(replayed.rotor.get("panicked_files"), 0);
	assert!(replayed.basic.get("type_queries") > 0, "no type queries ran");
	assert!(replayed.basic.get("symbols_found") > 0, "no symbols resolved");
	assert!(replayed.rotor.get("resolved_signatures") > 0, "no signatures resolved");
}

#[test]
#[ignore = "needs ROTOR_BENCH_PROJECT to point at the benchmark project's tsconfig"]
fn replays_the_benchmark_project() {
	let tsconfig =
		PathBuf::from(std::env::var_os("ROTOR_BENCH_PROJECT").expect("set ROTOR_BENCH_PROJECT to a tsconfig path"));
	with_large_stack(|| {
		let started = Instant::now();
		let Project { program } =
			load_project(&tsconfig, &ProjectOptions::default()).expect("benchmark tsconfig loads");
		println!(
			"open (parse + bind): {} source files, {:.3}s",
			program.source_files().len(),
			started.elapsed().as_secs_f64()
		);

		let started = Instant::now();
		let files = project_files(program);
		let basic = replay_basic(program, &files).expect("program has a checker pool");
		println!(
			"basic walk (cold): {} project files, {:.3}s",
			files.len(),
			started.elapsed().as_secs_f64()
		);

		let started = Instant::now();
		let rotor = replay_rotor_apis(program, &files).expect("program has a checker pool");
		println!("rotor-API walk: {:.3}s", started.elapsed().as_secs_f64());

		for (name, counts) in [("basic", &basic), ("rotor", &rotor)] {
			let line: Vec<String> = counts.iter().map(|(key, value)| format!("{key}={value}")).collect();
			println!("{name}: {}", line.join(" "));
		}
		assert_ne!(files.len(), 0, "no project files to walk");
		assert_eq!(basic.get("panicked_files"), 0);
		assert_eq!(rotor.get("panicked_files"), 0);
	});
}
