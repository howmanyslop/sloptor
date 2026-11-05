//! `solution_test.go`, and the solution helpers the other solution tests share.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rotor_support::gopath;

use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::compile::{DiagnosticInfo, Error};
use crate::output::BuildResult;
use crate::project::{ProjectOptions, lock};
use crate::rbxts_options::RbxtsOptions;
use crate::solution::{
	DrainResult, SolutionCoordinator, SolutionProject, SolutionProjectDrainer, SolutionProjectStatus, SolutionRoot,
	build_solution_graph, build_solution_graph_for_roots, canonical_solution_config_path,
};
use crate::solution_drain::build_solution_with_options;
use crate::timings::{
	BuildTimings, PROJECT_TIMING_STATUS_BLOCKED, PROJECT_TIMING_STATUS_FAILED, PROJECT_TIMING_STATUS_SKIPPED,
};

/// `recordingSolutionDrainer`
#[derive(Default)]
pub struct RecordingDrainer {
	pub drained: Mutex<Vec<String>>,
	pub fail: Mutex<String>,
}

impl RecordingDrainer {
	pub fn failing(config_path: &str) -> Arc<Self> {
		Arc::new(Self {
			drained: Mutex::default(),
			fail: Mutex::new(config_path.to_owned()),
		})
	}

	pub fn drained(&self) -> Vec<String> {
		lock(&self.drained).clone()
	}
}

impl SolutionProjectDrainer for RecordingDrainer {
	fn drain(&self, project: SolutionProject) -> DrainResult {
		lock(&self.drained).push(gopath::base(&gopath::dir(&project.config_path)));
		if project.config_path == *lock(&self.fail) {
			return DrainResult {
				result: Some(BuildResult {
					diagnostics: vec![DiagnosticInfo {
						message: "failed project".to_owned(),
						..DiagnosticInfo::default()
					}],
					..BuildResult::default()
				}),
				messages: vec!["failed project".to_owned()],
				error: Some(Error::message("failed project")),
			};
		}
		DrainResult {
			result: Some(BuildResult::default()),
			..DrainResult::default()
		}
	}
}

/// `ownedOutputSolutionDrainer`
struct OwnedOutputDrainer;

impl SolutionProjectDrainer for OwnedOutputDrainer {
	fn drain(&self, _: SolutionProject) -> DrainResult {
		DrainResult {
			result: Some(BuildResult {
				outputs: [("out/main.luau".to_owned(), b"compiled".to_vec())].into(),
				owned_outputs: vec!["out/main.d.ts".to_owned(), "out/main.luau".to_owned()],
				..BuildResult::default()
			}),
			..DrainResult::default()
		}
	}
}

/// `writeSolutionConfig`
pub fn write_solution_config(dir: &str, name: &str, references: &[&str], coordinator: bool) {
	std::fs::create_dir_all(dir).expect("the directory");
	let config = if coordinator {
		format!(r#"{{"files":[],"references":{}}}"#, solution_references(references))
	} else if references.is_empty() {
		"{}".to_owned()
	} else {
		format!(r#"{{"references":{}}}"#, solution_references(references))
	};
	std::fs::write(gopath::join(&[dir, name]), config).expect("the config writes");
}

/// `writeSolutionFile`
pub fn write_solution_file(dir: &str, name: &str, text: &str) {
	let path = gopath::join(&[dir, name]);
	std::fs::create_dir_all(gopath::dir(&path)).expect("the directory");
	std::fs::write(path, text).expect("the file writes");
}

/// `solutionReferences`
fn solution_references(paths: &[&str]) -> String {
	let entries: Vec<String> = paths.iter().map(|path| format!(r#"{{"path":"{path}"}}"#)).collect();
	format!("[{}]", entries.join(","))
}

/// `solutionProjectNames`
pub fn solution_project_names(projects: &[SolutionProject]) -> Vec<String> {
	projects
		.iter()
		.map(|project| gopath::base(&gopath::dir(&project.config_path)))
		.collect()
}

/// `writeBuildableSolutionProject`
pub fn write_buildable_solution_project(dir: &str) {
	std::fs::create_dir_all(gopath::join(&[dir, "src"])).expect("the directory");
	let config = r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"include":["src"]}"#;
	for (path, content) in [
		("tsconfig.json", config),
		("package.json", r#"{"name":"@scope/solution-child"}"#),
		("src/globals.d.ts", NO_LIB_GLOBAL_STUBS),
		("src/main.ts", "export const value = 1;\n"),
	] {
		std::fs::write(gopath::join(&[dir, path]), content).expect("the file writes");
	}
}

pub fn join(dir: &str, parts: &[&str]) -> String {
	let mut all = vec![dir];
	all.extend_from_slice(parts);
	gopath::join(&all)
}

pub fn builders(count: i64) -> ProjectOptions {
	ProjectOptions {
		builders: Some(count),
		..ProjectOptions::default()
	}
}

pub fn coordinator_with(
	config: &str,
	options: &ProjectOptions,
	drainer: Arc<dyn SolutionProjectDrainer>,
) -> SolutionCoordinator {
	SolutionCoordinator::with_drainer(config, options, drainer)
		.unwrap_or_else(|error| panic!("NewSolutionCoordinatorWithDrainer: {error}"))
}

#[test]
fn solution_graph() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_solution_config(&join(&root, &["app"]), "tsconfig.json", &["../left", "../right"], false);
	write_solution_config(&join(&root, &["left"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["right"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["shared"]), "tsconfig.json", &[], false);
	let graph = build_solution_graph(&join(&root, &["tsconfig.json"]), &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	let got = solution_project_names(&graph.projects);
	assert_eq!(got, ["shared", "left", "right", "app"], "project order = {got:?}");
}

#[test]
fn solution_graph_for_roots_builds_canonical_union() {
	let temp = Temp::new();
	let root = temp.dir();
	let production_dir = join(&root, &["production"]);
	let tests_dir = join(&root, &["tests"]);
	let shared_dir = join(&root, &["shared"]);
	write_solution_config(&production_dir, "tsconfig.json", &["../shared"], false);
	write_solution_config(&tests_dir, "tsconfig.json", &["../shared"], false);
	write_solution_config(&shared_dir, "tsconfig.json", &[], false);
	let graph = build_solution_graph_for_roots(
		&[
			join(&production_dir, &["tsconfig.json"]),
			format!("{production_dir}/../production/tsconfig.json"),
			join(&tests_dir, &["tsconfig.json"]),
		],
		&ProjectOptions::default(),
	)
	.unwrap_or_else(|error| panic!("BuildSolutionGraphForRoots: {error}"));
	let got = solution_project_names(&graph.projects);
	assert_eq!(got, ["shared", "production", "tests"], "project order = {got:?}");
	for project in &graph.projects {
		assert!(
			gopath::is_abs(&project.config_path) && project.config_path == gopath::clean(&project.config_path),
			"config path {:?} is not canonical",
			project.config_path
		);
	}
	let (shared_config, _) = canonical_solution_config_path(&join(&shared_dir, &["tsconfig.json"])).expect("canonical");
	assert_eq!(
		graph.projects[1].references,
		std::slice::from_ref(&shared_config),
		"production references"
	);
	assert_eq!(graph.projects[2].references, [shared_config], "tests references");
}

#[cfg(unix)]
#[test]
fn solution_graph_for_roots_deduplicates_symlink_alias() {
	let temp = Temp::new();
	let root = temp.dir();
	let project_dir = join(&root, &["project"]);
	let alias_dir = join(&root, &["project-alias"]);
	write_solution_config(&project_dir, "tsconfig.json", &[], false);
	std::os::unix::fs::symlink(&project_dir, &alias_dir).expect("symlinks");
	let graph = build_solution_graph_for_roots(
		&[
			join(&project_dir, &["tsconfig.json"]),
			join(&alias_dir, &["tsconfig.json"]),
		],
		&ProjectOptions::default(),
	)
	.unwrap_or_else(|error| panic!("BuildSolutionGraphForRoots: {error}"));
	assert_eq!(graph.projects.len(), 1, "project count");
	assert_eq!(
		graph.projects[0].config_path,
		join(&project_dir, &["tsconfig.json"]),
		"canonical config"
	);
}

#[test]
fn solution_graph_for_roots_rejects_conflicting_effective_options() {
	let temp = Temp::new();
	let root = temp.dir();
	let first_dir = join(&root, &["first"]);
	let second_dir = join(&root, &["second"]);
	let shared_dir = join(&root, &["shared"]);
	write_solution_config(&first_dir, "tsconfig.json", &["../shared"], true);
	write_solution_config(&second_dir, "tsconfig.json", &["../shared"], true);
	write_solution_config(&shared_dir, "tsconfig.json", &[], false);
	let options = |project_type| ProjectOptions {
		project_type: Some(project_type),
		solution_argv: Some(RbxtsOptions::default()),
		..ProjectOptions::default()
	};
	let result = SolutionCoordinator::for_root_options(&[
		SolutionRoot {
			config_path: join(&first_dir, &["tsconfig.json"]),
			options: options(rotor_transformer::ProjectType::Game),
		},
		SolutionRoot {
			config_path: join(&second_dir, &["tsconfig.json"]),
			options: options(rotor_transformer::ProjectType::Model),
		},
	]);
	match result {
		Err(error) if error.to_string().contains("conflicting effective options") => {}
		Err(error) => {
			panic!("NewSolutionCoordinatorForRootOptions error = {error}, want conflicting effective options")
		}
		Ok(_) => panic!("NewSolutionCoordinatorForRootOptions succeeded, want conflicting effective options"),
	}
}

#[test]
fn solution_coordinator() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./left", "./right"], true);
	write_solution_config(&join(&root, &["left"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["right"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["shared"]), "tsconfig.json", &[], false);
	let drainer = Arc::new(RecordingDrainer::default());
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(1), drainer.clone());
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "Drain: {error:?}");
	assert_eq!(drainer.drained(), ["shared", "left", "right"], "drained projects");
	let shared_config = join(&root, &["shared", "tsconfig.json"]);
	let state = coordinator.project_state(&shared_config);
	assert!(
		state.as_ref().is_some_and(|state| state.up_to_date),
		"shared state found = {}, want up-to-date",
		state.is_some()
	);
}

#[test]
fn solution_coordinator_blocks_dependent_after_failure() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_solution_config(&join(&root, &["app"]), "tsconfig.json", &["../broken"], false);
	write_solution_config(&join(&root, &["broken"]), "tsconfig.json", &[], false);
	let broken_config = join(&root, &["broken", "tsconfig.json"]);
	let drainer = RecordingDrainer::failing(&broken_config);
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(1), drainer.clone());
	let (_, _, error) = coordinator.drain();
	assert!(error.is_some(), "Drain unexpectedly succeeded");
	assert_eq!(drainer.drained(), ["broken"], "drained projects");
	let state = coordinator.project_state(&join(&root, &["app", "tsconfig.json"]));
	assert!(
		state.as_ref().is_some_and(|state| state.blocked_by == broken_config),
		"app state blocked by {:?}, want blocked by {broken_config}",
		state.map(|state| state.blocked_by)
	);
	lock(&drainer.fail).clear();
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "Drain after dependency recovery: {error:?}");
	assert_eq!(
		drainer.drained(),
		["broken", "broken", "app"],
		"drained projects after recovery"
	);
}

#[test]
fn solution_coordinator_returns_one_owned_outcome_per_union_project() {
	let temp = Temp::new();
	let root = temp.dir();
	let production_dir = join(&root, &["production"]);
	let tests_dir = join(&root, &["tests"]);
	let shared_dir = join(&root, &["shared"]);
	write_solution_config(&production_dir, "tsconfig.json", &["../shared"], false);
	write_solution_config(&tests_dir, "tsconfig.json", &["../shared"], false);
	write_solution_config(&shared_dir, "tsconfig.json", &[], false);
	let (shared_config, _) = canonical_solution_config_path(&join(&shared_dir, &["tsconfig.json"])).expect("canonical");
	let drainer = RecordingDrainer::failing(&shared_config);
	let mut coordinator = SolutionCoordinator::for_roots_with_drainer(
		&[
			join(&production_dir, &["tsconfig.json"]),
			join(&tests_dir, &["tsconfig.json"]),
		],
		&ProjectOptions::default(),
		drainer.clone(),
	)
	.unwrap_or_else(|error| panic!("NewSolutionCoordinatorForRootsWithDrainer: {error}"));
	let output = coordinator.drain_with_project_results();
	assert!(output.error.is_some(), "DrainWithProjectResults unexpectedly succeeded");
	let projects = output.project_results;
	assert_eq!(projects.len(), 3, "project outcomes");
	let statuses: Vec<SolutionProjectStatus> = projects.iter().map(|project| project.status).collect();
	assert_eq!(
		statuses,
		[
			SolutionProjectStatus::Failed,
			SolutionProjectStatus::Blocked,
			SolutionProjectStatus::Blocked
		],
		"project statuses"
	);
	assert_eq!(projects[0].diagnostics.len(), 1, "failed project diagnostics");
	for project in &projects[1..] {
		assert!(
			project.diagnostics.is_empty(),
			"blocked project diagnostics = {:?}, want none",
			project.diagnostics
		);
		assert_eq!(
			project.blockers,
			std::slice::from_ref(&shared_config),
			"project blockers"
		);
	}
	assert_eq!(drainer.drained(), ["shared"], "drained projects");
}

#[test]
fn solution_coordinator_reports_no_change_for_up_to_date_project() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &[], false);
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&ProjectOptions::default(),
		Arc::new(RecordingDrainer::default()),
	);
	let first = coordinator.drain_with_project_results();
	assert!(
		first.error.is_none(),
		"first DrainWithProjectResults: {:?}",
		first.error
	);
	let second = coordinator.drain_with_project_results();
	assert!(
		second.error.is_none(),
		"second DrainWithProjectResults: {:?}",
		second.error
	);
	assert!(
		first.project_results[0].status == SolutionProjectStatus::Success
			&& second.project_results[0].status == SolutionProjectStatus::NoChange,
		"statuses = {:?} then {:?}, want success then no-change",
		first.project_results[0].status,
		second.project_results[0].status
	);
}

#[test]
fn solution_coordinator_retains_complete_output_ownership_for_no_change_projects() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &[], false);
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&ProjectOptions::default(),
		Arc::new(OwnedOutputDrainer),
	);
	let want = ["out/main.d.ts", "out/main.luau"];
	for attempt in 1..=2 {
		let output = coordinator.drain_with_project_results();
		assert!(output.error.is_none(), "drain {attempt}: {:?}", output.error);
		let project = &output.project_results[0];
		assert!(
			project.outputs == want && project.output_count == want.len(),
			"drain {attempt} output ownership = {:?} ({}), want {want:?}",
			project.outputs,
			project.output_count
		);
	}
}

#[test]
fn solution_coordinator_reports_unique_project_scheduling() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./left", "./right"], true);
	write_solution_config(&join(&root, &["left"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["right"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["shared"]), "tsconfig.json", &[], false);
	let timings = Arc::new(BuildTimings::new());
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		},
		Arc::new(RecordingDrainer::default()),
	);
	let output = coordinator.drain_with_project_results();
	assert!(output.error.is_none(), "DrainWithProjectResults: {:?}", output.error);
	assert_eq!(timings.report().counts.scheduled_projects, 3, "scheduled projects");
	for project in &output.project_results {
		assert_eq!(
			project.timings.counts.scheduled_projects, 1,
			"{} scheduled count",
			project.config_path
		);
	}
}

#[test]
fn solution_coordinator_blocks_dependent_through_skipped_coordinator() {
	let temp = Temp::new();
	let root = temp.dir();
	let app_dir = join(&root, &["app"]);
	let bridge_dir = join(&root, &["bridge"]);
	let broken_dir = join(&root, &["broken"]);
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_solution_config(&app_dir, "tsconfig.json", &["../bridge"], false);
	write_solution_file(&bridge_dir, "tsconfig.base.json", r#"{"files":[],"include":[]}"#);
	write_solution_file(
		&bridge_dir,
		"tsconfig.json",
		r#"{"extends":"./tsconfig.base.json","references":[{"path":"../broken"}]}"#,
	);
	write_solution_config(&broken_dir, "tsconfig.json", &[], false);
	let broken_config = join(&broken_dir, &["tsconfig.json"]);
	let drainer = RecordingDrainer::failing(&broken_config);
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(1), drainer.clone());
	let (_, _, error) = coordinator.drain();
	assert!(error.is_some(), "Drain unexpectedly succeeded");
	assert_eq!(drainer.drained(), ["broken"], "drained projects");
	let state = coordinator.project_state(&join(&app_dir, &["tsconfig.json"]));
	assert!(
		state.as_ref().is_some_and(|state| state.blocked_by == broken_config),
		"app state, want blocked by {broken_config}"
	);
}

#[test]
fn solution_coordinator_skips_up_to_date_projects() {
	let temp = Temp::new();
	let root = temp.dir();
	let child = join(&root, &["child"]);
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	write_buildable_solution_project(&child);
	large_stack(|| {
		let mut coordinator = SolutionCoordinator::new(&join(&root, &["tsconfig.json"]), &ProjectOptions::default())
			.unwrap_or_else(|error| panic!("NewSolutionCoordinator: {error}"));
		let (first, _, error) = coordinator.drain();
		assert!(error.is_none(), "first Drain: {error:?}");
		assert!(!first.emitted_files.is_empty(), "first Drain emitted no files");
		let (second, _, error) = coordinator.drain();
		assert!(error.is_none(), "second Drain: {error:?}");
		assert!(
			second.emitted_files.is_empty(),
			"second Drain emitted files = {:?}, want none",
			second.emitted_files
		);
	});
}

#[test]
fn reference_only_solution_coordinator_allows_empty_include() {
	let temp = Temp::new();
	let root = temp.dir();
	let child = join(&root, &["child"]);
	let root_config = join(&root, &["tsconfig.json"]);
	std::fs::write(
		&root_config,
		r#"{"files":[],"include":[],"references":[{"path":"./child"}]}"#,
	)
	.expect("write");
	write_buildable_solution_project(&child);
	large_stack(|| {
		if let Err(failure) = build_solution_with_options(&root_config, &ProjectOptions::default()) {
			panic!("BuildSolutionWithOptions: {failure} ({:?})", failure.failure.messages());
		}
	});
	assert!(Path::new(&join(&child, &["out", "main.luau"])).exists(), "child output");
	assert!(
		!Path::new(&join(&root, &["out"])).exists(),
		"coordinator output exists, want not exists"
	);
}

#[test]
fn solution_graph_skips_coordinator_with_inherited_empty_files() {
	let temp = Temp::new();
	let root = temp.dir();
	let package_dir = join(&root, &["package"]);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"files":[],"references":[{"path":"./package"},{"path":"./package/tsconfig.lib.json"}]}"#,
	);
	write_solution_file(
		&package_dir,
		"tsconfig.base.json",
		"{
		\"files\": [],
		\"include\": [], // inherited reference-only coordinator
	}",
	);
	write_solution_file(
		&package_dir,
		"tsconfig.json",
		r#"{"extends":"./tsconfig.base.json","references":[{"path":"./tsconfig.lib.json"},{"path":"./tsconfig.spec.json"}]}"#,
	);
	write_solution_file(&package_dir, "tsconfig.lib.json", "{}");
	write_solution_file(
		&package_dir,
		"tsconfig.spec.json",
		r#"{"extends":"./tsconfig.base.json","references":[{"path":"./tsconfig.lib.json"}],"include":["test"]}"#,
	);
	let graph = build_solution_graph(&join(&root, &["tsconfig.json"]), &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	let want = [
		join(&package_dir, &["tsconfig.lib.json"]),
		join(&package_dir, &["tsconfig.spec.json"]),
	];
	let got: Vec<&str> = graph
		.projects
		.iter()
		.map(|project| project.config_path.as_str())
		.collect();
	assert_eq!(got, want, "projects, want emitting projects {want:?}");
}

#[test]
fn solution_graph_resolves_coordinator_extends_with_package_ts_config() {
	let temp = Temp::new();
	let root = temp.dir();
	let base_package = join(&root, &["node_modules", "@scope", "base"]);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"extends":"@scope/base","references":[{"path":"./child"}]}"#,
	);
	write_solution_file(&base_package, "package.json", r#"{"tsconfig":"config.json"}"#);
	write_solution_file(&base_package, "config.json", r#"{"files":[],"include":[]}"#);
	write_solution_config(&join(&root, &["child"]), "tsconfig.json", &[], false);
	let graph = build_solution_graph(
		&join(&root, &["tsconfig.json"]),
		&ProjectOptions {
			solution_argv: Some(RbxtsOptions::default()),
			..ProjectOptions::default()
		},
	)
	.unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	assert_eq!(solution_project_names(&graph.projects), ["child"], "project order");
}

#[test]
fn solution_graph_resolves_extensionless_coordinator_extends() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_file(&root, "base", r#"{"files":[],"include":[]}"#);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"extends":"./base","references":[{"path":"./child"}]}"#,
	);
	write_solution_config(&join(&root, &["child"]), "tsconfig.json", &[], false);
	let graph = build_solution_graph(&join(&root, &["tsconfig.json"]), &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	assert_eq!(solution_project_names(&graph.projects), ["child"], "project order");
}

#[test]
fn solution_graph_resolves_dotted_coordinator_extends() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_file(&root, "tsconfig.base.json", r#"{"files":[],"include":[]}"#);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"extends":"./tsconfig.base","references":[{"path":"./child"}]}"#,
	);
	write_solution_config(&join(&root, &["child"]), "tsconfig.json", &[], false);
	let graph = build_solution_graph(&join(&root, &["tsconfig.json"]), &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	assert_eq!(solution_project_names(&graph.projects), ["child"], "project order");
}

#[test]
fn solution_build_order() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_solution_config(&join(&root, &["app"]), "tsconfig.json", &["../dependency"], false);
	write_solution_config(&join(&root, &["dependency"]), "tsconfig.json", &[], false);
	let drainer = Arc::new(RecordingDrainer::default());
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(1), drainer.clone());
	let (_, _, error) = coordinator.drain();
	assert!(error.is_none(), "Drain: {error:?}");
	assert_eq!(drainer.drained(), ["dependency", "app"], "drained projects");
}

#[test]
fn solution_cycle() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./first"], true);
	write_solution_config(&join(&root, &["first"]), "tsconfig.json", &["../second"], false);
	write_solution_config(&join(&root, &["second"]), "tsconfig.json", &["../first"], false);
	let Err(error) = build_solution_graph(&join(&root, &["tsconfig.json"]), &ProjectOptions::default()) else {
		panic!("BuildSolutionGraph unexpectedly succeeded");
	};
	let message = error.to_string();
	assert!(
		message.contains("first") && message.contains("second"),
		"cycle error = {message:?}, want cycle path"
	);
}

#[test]
fn emit_declaration_only() {
	let temp = Temp::new();
	let root = temp.dir();
	let child = join(&root, &["child"]);
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	write_buildable_solution_project(&child);
	large_stack(|| {
		let options = ProjectOptions {
			emit_declaration_only: true,
			..ProjectOptions::default()
		};
		if let Err(failure) = build_solution_with_options(&join(&root, &["tsconfig.json"]), &options) {
			panic!("BuildSolutionWithOptions: {failure} ({:?})", failure.failure.messages());
		}
	});
	assert!(
		Path::new(&join(&child, &["out", "main.d.ts"])).exists(),
		"declaration output"
	);
	assert!(
		!Path::new(&join(&child, &["out", "main.luau"])).exists(),
		"Luau output exists, want not exists"
	);
}

#[test]
fn solution_invalidate_direct_project_keeps_incremental_selection() {
	let temp = Temp::new();
	let root = temp.dir();
	let child = join(&root, &["child"]);
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	write_buildable_solution_project(&child);
	let child_config = join(&child, &["tsconfig.json"]);
	std::fs::write(join(&child, &["src", "extra.ts"]), "export const extra = 1;\n").expect("write");
	let timings = Arc::new(BuildTimings::new());
	large_stack(|| {
		let options = ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			builders: Some(1),
			..ProjectOptions::default()
		};
		let mut coordinator = SolutionCoordinator::new(&join(&root, &["tsconfig.json"]), &options)
			.unwrap_or_else(|error| panic!("NewSolutionCoordinator: {error}"));
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "first Drain: {error:?} ({messages:?})");
		let counts = timings.report().counts;
		assert_eq!(
			counts.selected_sources, counts.total_sources,
			"first build selected {} of {} sources, want full build",
			counts.selected_sources, counts.total_sources
		);
		std::fs::write(join(&child, &["src", "main.ts"]), "export const value = 2;\n").expect("write");
		coordinator.invalidate(std::slice::from_ref(&child_config));
		let (_, messages, error) = coordinator.drain();
		assert!(error.is_none(), "second Drain: {error:?} ({messages:?})");
		let counts = timings.report().counts;
		assert!(
			counts.selected_sources != 0 && counts.selected_sources < counts.total_sources,
			"directly invalidated child selected {} of {} sources, want fewer than all",
			counts.selected_sources,
			counts.total_sources
		);
	});
}

#[test]
fn solution_timings_records_statuses_and_graph_order() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./left", "./right"], true);
	write_solution_config(&join(&root, &["left"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["right"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["shared"]), "tsconfig.json", &[], false);
	let drainer = RecordingDrainer::failing(&join(&root, &["shared", "tsconfig.json"]));
	let timings = Arc::new(BuildTimings::new());
	let options = ProjectOptions {
		builders: Some(1),
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &options, drainer);
	assert!(coordinator.drain().2.is_some(), "Drain succeeded, want shared failure");
	let projects = timings.report().projects;
	assert_eq!(projects.len(), 3, "projects");
	let names: Vec<String> = projects
		.iter()
		.map(|project| gopath::base(&gopath::dir(&project.config_path)))
		.collect();
	assert_eq!(names, ["shared", "left", "right"], "project order");
	assert_eq!(projects[0].status, PROJECT_TIMING_STATUS_FAILED, "shared status");
	assert!(
		projects[1].status == PROJECT_TIMING_STATUS_BLOCKED && projects[2].status == PROJECT_TIMING_STATUS_BLOCKED,
		"dependent statuses = {:?} {:?}, want blocked",
		projects[1].status,
		projects[2].status
	);

	let timings = Arc::new(BuildTimings::new());
	let options = ProjectOptions {
		builders: Some(1),
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&options,
		Arc::new(RecordingDrainer::default()),
	);
	assert!(coordinator.drain().2.is_none(), "second Drain");
	assert!(coordinator.drain().2.is_none(), "repeat Drain");
	for project in timings.report().projects {
		assert_eq!(
			project.status, PROJECT_TIMING_STATUS_SKIPPED,
			"{} status, want skipped on up-to-date drain",
			project.config_path
		);
	}
}
