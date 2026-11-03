//! `census_solution_test.go`: a solution census reports every project and writes nothing.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::census::{
	CLEAN, NOANY, TYPEBAD, add_transformer_plugin, census_by_file, census_file_names, tree_snapshot,
	write_census_project,
};
use super::solution::{join, write_solution_config};
use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::census::{FileOutcome, ProjectDiagnostics};
use crate::census_solution::{SolutionDiagnostics, SolutionOverlayMatches, compile_solution_diagnostics};
use crate::compile::Error;
use crate::project::{ProjectOptions, new_project_program_with_options};

/// `writeCensusSolutionProject`: one referenced project of a census solution, a scoped package
/// name, composite + declaration, and one file per entry of `files` under src/.
fn write_census_solution_project(dir: &str, pkg_name: &str, references: &[&str], files: &[(&str, &str)]) {
	std::fs::create_dir_all(join(dir, &["src"])).expect("the directory");
	let mut config = r#"{"compilerOptions": {"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"include":["src"]"#.to_owned();
	if !references.is_empty() {
		let entries: Vec<String> = references
			.iter()
			.map(|path| format!(r#"{{"path":"{path}"}}"#))
			.collect();
		config.push_str(r#","references":["#);
		config.push_str(&entries.join(","));
		config.push(']');
	}
	config.push('}');
	std::fs::write(join(dir, &["tsconfig.json"]), config).expect("write");
	std::fs::write(join(dir, &["package.json"]), format!(r#"{{"name":"{pkg_name}"}}"#)).expect("write");
	std::fs::write(join(dir, &["src", "globals.d.ts"]), NO_LIB_GLOBAL_STUBS).expect("write");
	for (name, text) in files {
		std::fs::write(join(dir, &["src", name]), text).expect("write");
	}
}

/// `solutionCensusByDir`
fn census_by_dir(solution: &SolutionDiagnostics) -> HashMap<String, ProjectDiagnostics> {
	solution
		.projects
		.iter()
		.map(|project| (rotor_support::gopath::base(&project.project_dir), project.clone()))
		.collect()
}

fn outcome(project: Option<&ProjectDiagnostics>, name: &str) -> Option<FileOutcome> {
	project.and_then(|project| census_by_file(project).get(name).map(|file| file.outcome))
}

fn solution_census(config: &str, opts: &ProjectOptions) -> (SolutionDiagnostics, Option<Error>) {
	large_stack(|| compile_solution_diagnostics(config, opts))
}

/// Two sibling projects, alpha and beta, with one file each.
fn alpha_beta(alpha: &str, beta: &str) -> (Temp, String) {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./alpha", "./beta"], true);
	write_census_solution_project(&join(&root, &["alpha"]), "@scope/alpha", &[], &[("alpha.ts", alpha)]);
	write_census_solution_project(&join(&root, &["beta"]), "@scope/beta", &[], &[("beta.ts", beta)]);
	(temp, root)
}

#[test]
fn compile_solution_diagnostics_attributes_every_project() {
	// Given a solution of two referenced projects, each with its own files
	let (_temp, root) = alpha_beta(NOANY, CLEAN);
	// When the solution census runs
	let (solution, error) = solution_census(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	assert!(error.is_none(), "CompileSolutionDiagnostics: {error:?}");
	// Then every project is reported separately, carrying its own files and attribution
	assert_eq!(solution.projects.len(), 2, "projects");
	let by_dir = census_by_dir(&solution);
	for name in ["alpha", "beta"] {
		let project = by_dir
			.get(name)
			.unwrap_or_else(|| panic!("{name} missing from the census"));
		assert!(
			rotor_support::gopath::to_slash(&project.config_path).ends_with(&format!("{name}/tsconfig.json")),
			"{name} ConfigPath = {:?}, want it to name that project's tsconfig",
			project.config_path
		);
		assert_eq!(
			census_file_names(&census_by_file(project)),
			[format!("{name}.ts")],
			"{name} files"
		);
	}
	assert_eq!(
		outcome(by_dir.get("alpha"), "alpha.ts"),
		Some(FileOutcome::TransformerDiagnostic),
		"alpha.ts outcome"
	);
	assert_eq!(
		outcome(by_dir.get("beta"), "beta.ts"),
		Some(FileOutcome::Ok),
		"beta.ts outcome"
	);
}

#[test]
fn compile_solution_diagnostics_failing_project_does_not_stop_the_rest() {
	// Given a solution whose first project cannot be set up at all — a non-package project
	// with no Rojo file — and a second that depends on it
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_census_solution_project(&join(&root, &["broken"]), "broken", &[], &[("broken.ts", CLEAN)]);
	write_census_solution_project(
		&join(&root, &["app"]),
		"@scope/app",
		&["../broken"],
		&[("app.ts", NOANY)],
	);
	// When the solution census runs
	let (solution, error) = solution_census(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	// Then the failure is reported, and the dependent project is censused anyway
	assert!(
		error.is_some(),
		"a project that could not be set up did not fail the run"
	);
	let by_dir = census_by_dir(&solution);
	let broken = by_dir.get("broken").expect("broken missing from the census");
	assert!(
		!broken.diagnostics.is_empty(),
		"the failing project carries no diagnostic explaining why"
	);
	assert!(by_dir.contains_key("app"), "app missing from the census");
	assert_eq!(
		outcome(by_dir.get("app"), "app.ts"),
		Some(FileOutcome::TransformerDiagnostic),
		"app.ts outcome — the dependent project was not censused"
	);
}

#[test]
fn compile_solution_diagnostics_overlay_lands_in_the_owning_project() {
	// Given two sibling projects that are both clean on disk
	let (_temp, root) = alpha_beta(CLEAN, CLEAN);
	// When one project's file is overlaid with something that fails
	let opts = ProjectOptions {
		overlays: [(join(&root, &["beta", "src", "beta.ts"]), NOANY.to_owned())].into(),
		..ProjectOptions::default()
	};
	let (solution, error) = solution_census(&join(&root, &["tsconfig.json"]), &opts);
	assert!(error.is_none(), "CompileSolutionDiagnostics: {error:?}");
	// Then the overlay applies to that project only, and is counted there.
	let by_dir = census_by_dir(&solution);
	assert_eq!(solution.overlay_matches, 1, "solution OverlayMatches");
	assert_eq!(
		outcome(by_dir.get("beta"), "beta.ts"),
		Some(FileOutcome::TransformerDiagnostic),
		"beta.ts outcome — the overlay did not apply"
	);
	assert_eq!(
		outcome(by_dir.get("alpha"), "alpha.ts"),
		Some(FileOutcome::Ok),
		"alpha.ts outcome — the overlay leaked into another project"
	);
	assert_eq!(
		by_dir.get("beta").map(|beta| beta.overlay_matches),
		Some(1),
		"beta OverlayMatches"
	);
	assert_eq!(
		by_dir.get("alpha").map(|alpha| alpha.overlay_matches),
		Some(0),
		"alpha OverlayMatches"
	);
}

#[test]
fn compile_solution_diagnostics_overlay_matching_nothing_anywhere_is_an_error() {
	// Given a solution and an overlay for a file no project holds
	let (_temp, root) = alpha_beta(CLEAN, CLEAN);
	// When the solution census runs
	let opts = ProjectOptions {
		overlays: [(join(&root, &["alpha", "src", "typo.ts"]), NOANY.to_owned())].into(),
		..ProjectOptions::default()
	};
	let (_, error) = solution_census(&join(&root, &["tsconfig.json"]), &opts);
	// Then it refuses
	let error = error
		.expect("an overlay matching no file in any project was accepted")
		.to_string();
	assert!(
		error.contains("typo.ts"),
		"error = {error}, want it to name the unmatched overlay"
	);
}

/// `solutionOverlayProgram`: one project of a solution census, built the way
/// `CompileSolutionDiagnostics` does, stopping short of the sidecar.
fn solution_overlay_program(
	dir: &str,
	overlays: &BTreeMap<String, String>,
) -> (Arc<SolutionOverlayMatches>, Option<String>) {
	let tracker = Arc::new(SolutionOverlayMatches::default());
	let opts = ProjectOptions {
		overlays: overlays.clone(),
		solution_overlays: Some(Arc::clone(&tracker)),
		..ProjectOptions::default()
	};
	let error = large_stack(|| {
		new_project_program_with_options(dir, "", &opts)
			.err()
			.map(|failure| failure.error.to_string())
	});
	(tracker, error)
}

#[test]
fn solution_overlays_accept_a_plugin_project_they_reach() {
	// Given a project that declares a transformer plugin, and an overlay for a file it holds
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	add_transformer_plugin(&temp.dir());
	// When this project of the solution is prepared
	let overlaid = temp.join("src/clean.ts").to_string_lossy().into_owned();
	let (tracker, error) = solution_overlay_program(&temp.dir(), &[(overlaid.clone(), NOANY.to_owned())].into());
	// Then it is accepted, and the overlay is recorded against the union
	assert!(
		error.is_none(),
		"an overlay landing in a transformer-plugin project was refused: {error:?}"
	);
	let unmatched = tracker.unmatched(&[(overlaid, String::new())].into());
	assert!(
		unmatched.is_empty(),
		"unmatched = {unmatched:?}, want the overlay recorded as matched"
	);
}

#[test]
fn solution_overlays_defer_the_unmatched_check_to_the_union() {
	// Given a project that holds none of the overlaid files
	let temp = write_census_project(&[("clean.ts", CLEAN)]);
	let elsewhere_temp = Temp::new();
	let elsewhere = elsewhere_temp.join("src/other.ts").to_string_lossy().into_owned();
	// When this project of the solution is prepared
	let (tracker, error) = solution_overlay_program(&temp.dir(), &[(elsewhere.clone(), CLEAN.to_owned())].into());
	// Then it does not fail here.
	assert!(
		error.is_none(),
		"a project that holds none of the overlaid files was refused: {error:?}"
	);
	let unmatched = tracker.unmatched(&[(elsewhere, String::new())].into());
	assert_eq!(
		unmatched.len(),
		1,
		"unmatched = {unmatched:?}, want the overlay recorded as still unmatched"
	);
}

#[test]
fn compile_solution_diagnostics_writes_nothing_to_disk() {
	// Given a solution and a snapshot of its tree
	let (_temp, root) = alpha_beta(CLEAN, TYPEBAD);
	let before = tree_snapshot(&root);
	// When the solution census runs
	let (_, error) = solution_census(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	assert!(error.is_none(), "CompileSolutionDiagnostics: {error:?}");
	// Then no project wrote an outDir, an include folder, a rotor.d.ts or a .tsbuildinfo
	assert_eq!(tree_snapshot(&root), before, "tree changed");
}
