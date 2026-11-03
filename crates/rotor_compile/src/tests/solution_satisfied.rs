//! `solution_satisfied_test.go`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::json;

use super::project_reference_import::{ReferenceFixture, build_solution_triple, new_reference_fixture};
use super::solution::{RecordingDrainer, coordinator_with, join, write_solution_config};
use super::solution_drain::must_read_file;
use super::solution_metadata::write_metadata_solution_project;
use super::vector_math::build_audit_project;
use super::{Temp, copy_dir, large_stack};
use crate::crossproject::project_restored_declarations;
use crate::project::ProjectOptions;
use crate::solution::{
	SolutionCoordinator, SolutionProject, SolutionProjectResult, SolutionProjectSelection, SolutionProjectStatus,
	build_solution_graph,
};
use crate::solution_cache::SolutionCompileCache;
use crate::solution_drain::{DrainOutput, SolutionBuildDrainer, SolutionSelectionError};
use crate::timings::BuildTimings;

/// `solutionProjectResultsByConfig`
fn results_by_config(projects: &[SolutionProjectResult]) -> HashMap<String, SolutionProjectResult> {
	projects
		.iter()
		.map(|project| (project.config_path.clone(), project.clone()))
		.collect()
}

fn select(coordinator: &mut SolutionCoordinator, selected: &[String], satisfied: &[String]) -> DrainOutput {
	let selection = SolutionProjectSelection {
		selected: selected.to_vec(),
		satisfied: satisfied.to_vec(),
	};
	large_stack(|| coordinator.drain_with_project_results_for_selection(Some(&selection)))
}

/// `assertDrainedProjectNames`
fn assert_drained_project_names(drainer: &RecordingDrainer, want: &[&str]) {
	let mut names = drainer.drained();
	names.sort();
	let mut want = want.to_vec();
	want.sort_unstable();
	assert_eq!(names, want, "drained projects");
}

/// The app → shared solution the selection tests share.
fn app_shared_solution(root: &str) -> (String, String) {
	write_solution_config(root, "tsconfig.json", &["./app"], true);
	write_solution_config(&join(root, &["app"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(root, &["shared"]), "tsconfig.json", &[], false);
	(
		join(root, &["app", "tsconfig.json"]),
		join(root, &["shared", "tsconfig.json"]),
	)
}

#[test]
fn satisfied_declaration_validation_reuses_solution_parse_cache() {
	let temp = Temp::new();
	let declaration = join(&temp.dir(), &["restored.d.ts"]);
	std::fs::write(&declaration, "export declare const restored: number;\n").expect("the declaration writes");
	let project = SolutionProject {
		config_path: join(&temp.dir(), &["tsconfig.json"]),
		..SolutionProject::default()
	};
	let mut drainer = SolutionBuildDrainer::default();
	drainer
		.restored_declarations
		.insert(project.config_path.clone(), vec![declaration]);
	let cache = SolutionCompileCache::new();
	for attempt in 1..=2 {
		if let Some(error) = drainer.validate_satisfied_project(&project, &cache) {
			panic!("validation {attempt}: {}", error.message);
		}
	}
	assert!(
		cache.misses() == 1 && cache.hits() == 1,
		"parse cache hits={} misses={}, want one of each",
		cache.hits(),
		cache.misses()
	);
}

#[test]
fn solution_coordinator_builds_selected_project_from_satisfied_dependency() {
	let temp = Temp::new();
	let root = temp.dir();
	let (_, shared_config) = app_shared_solution(&root);
	let drainer = Arc::new(RecordingDrainer::default());
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&ProjectOptions::default(),
		drainer.clone(),
	);
	let output = select(
		&mut coordinator,
		&[join(&root, &["app", "..", "app", "tsconfig.json"])],
		&[shared_config],
	);
	assert!(
		output.error.is_none(),
		"DrainWithProjectResultsForSelection: {:?}",
		output.error
	);
	assert_eq!(drainer.drained(), ["app"], "drained projects");
	let projects = &output.project_results;
	assert_eq!(
		[projects[0].status, projects[1].status],
		[SolutionProjectStatus::Satisfied, SolutionProjectStatus::Success],
		"project statuses"
	);
	let counts = &projects[0].timings.counts;
	assert!(
		counts.scheduled_projects == 0 && counts.transformed_projects == 0 && counts.emitted_projects == 0,
		"satisfied project performed work: {counts:?}"
	);
}

#[test]
fn solution_coordinator_rebuilds_for_changed_satisfied_declaration() {
	const FINAL_DECLARATION: &str = "export declare function values(): LuaTuple<[number, string]>;\n";
	let fixture = |declaration: &str| -> ReferenceFixture {
		let mut fixture = new_reference_fixture();
		let audit = build_audit_project();
		copy_dir(&audit.join("node_modules"), Path::new(&fixture.path("node_modules")));
		fixture.write(
			"node_modules/@rbxts/globals/index.d.ts",
			"/// <reference types=\"compiler-types\" />\n/// <reference types=\"types\" />\n",
		);
		fixture.project("shared", &[], &json!({}));
		fixture.project("game", &["shared"], &json!({"declaration": true}));
		fixture.write("shared/src/tuple/index.d.ts", declaration);
		fixture.write("shared/src/tuple/init.luau", "return {}\n");
		fixture.write(
			"game/src/index.ts",
			"import { values } from \"../../shared/src/tuple\";\nconst [count, text] = values();\nprint(count, text);\n",
		);
		fixture
	};
	let project_fixture = fixture("export declare function values(): [number, string];\n");
	project_fixture.build();
	project_fixture.write("shared/src/tuple/index.d.ts", FINAL_DECLARATION);
	let shared_config = project_fixture.path("shared/tsconfig.json");
	let game_config = project_fixture.path("game/tsconfig.json");
	let (_, messages, error) = build_solution_triple(&shared_config, &project_fixture.options());
	assert!(error.is_none(), "dependency build: {error:?} ({messages:?})");
	let mut coordinator = large_stack(|| SolutionCoordinator::new(&game_config, &project_fixture.options()))
		.unwrap_or_else(|error| panic!("{error}"));
	let output = select(&mut coordinator, &[game_config], std::slice::from_ref(&shared_config));
	assert!(
		output.error.is_none(),
		"selected build: {:?} ({:?})",
		output.error,
		output.messages
	);
	let results = results_by_config(&output.project_results);
	let shared = &results[&shared_config];
	assert!(
		shared.status == SolutionProjectStatus::Satisfied && shared.timings.counts.transformed_projects == 0,
		"dependency result = {shared:?}, want satisfied without transformations"
	);
	let oracle = fixture(FINAL_DECLARATION);
	oracle.build();
	let got = String::from_utf8(must_read_file(&project_fixture.path("game/out/init.luau"))).expect("utf8");
	let want = String::from_utf8(must_read_file(&oracle.path("game/out/init.luau"))).expect("utf8");
	assert!(
		got == want && want.contains("local count, text = values()"),
		"satisfied dependency build differs from cold build:\nwarm:\n{got}\ncold:\n{want}"
	);
}

#[test]
fn solution_coordinator_telemetry_separates_selected_and_satisfied_projects() {
	let temp = Temp::new();
	let root = temp.dir();
	let (app_config, shared_config) = app_shared_solution(&root);
	let timings = Arc::new(BuildTimings::new());
	let options = ProjectOptions {
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&options,
		Arc::new(RecordingDrainer::default()),
	);
	let output = select(&mut coordinator, &[app_config], &[shared_config]);
	assert!(
		output.error.is_none(),
		"DrainWithProjectResultsForSelection: {:?}",
		output.error
	);
	let counts = timings.report().counts;
	assert_eq!(
		[
			counts.selected_projects,
			counts.satisfied_projects,
			counts.scheduled_projects
		],
		[1, 1, 1],
		"selected/satisfied/scheduled telemetry"
	);
	let got = &output.project_results[0].timings.counts;
	assert!(
		got.selected_projects == 0
			&& got.satisfied_projects == 1
			&& got.scheduled_projects == 0
			&& got.transformed_projects == 0
			&& got.emitted_projects == 0,
		"satisfied project telemetry = {got:?}, want one satisfied and zero work"
	);
	let got = &output.project_results[1].timings.counts;
	assert!(
		got.selected_projects == 1 && got.satisfied_projects == 0 && got.scheduled_projects == 1,
		"selected project telemetry = {got:?}, want one selected and scheduled"
	);
}

#[test]
fn solution_coordinator_keeps_selected_dependency_barriers() {
	let temp = Temp::new();
	let root = temp.dir();
	let (app_config, shared_config) = app_shared_solution(&root);
	let drainer = RecordingDrainer::failing(&shared_config);
	let mut coordinator = coordinator_with(
		&join(&root, &["tsconfig.json"]),
		&ProjectOptions::default(),
		drainer.clone(),
	);
	let output = select(&mut coordinator, &[shared_config.clone(), app_config], &[]);
	assert!(
		output.error.is_some(),
		"DrainWithProjectResultsForSelection unexpectedly succeeded"
	);
	let projects = &output.project_results;
	assert_eq!(
		projects.len(),
		2,
		"DrainWithProjectResultsForSelection returned projects"
	);
	assert_eq!(drainer.drained(), ["shared"], "drained projects");
	assert_eq!(
		[projects[0].status, projects[1].status],
		[SolutionProjectStatus::Failed, SolutionProjectStatus::Blocked],
		"project statuses"
	);
	assert_eq!(projects[1].blockers, [shared_config], "app blockers");
}

#[test]
fn satisfied_validation_failure_blocks_only_transitive_dependants() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./dependent", "./success", "./failure"], true);
	write_solution_config(&join(&root, &["restored"]), "tsconfig.json", &[], false);
	write_solution_config(&join(&root, &["dependent"]), "tsconfig.json", &["../restored"], false);
	write_solution_config(&join(&root, &["success"]), "tsconfig.json", &[], false);
	write_solution_config(&join(&root, &["failure"]), "tsconfig.json", &[], false);
	let restored_config = join(&root, &["restored", "tsconfig.json"]);
	let dependent_config = join(&root, &["dependent", "tsconfig.json"]);
	let success_config = join(&root, &["success", "tsconfig.json"]);
	let failure_config = join(&root, &["failure", "tsconfig.json"]);
	let recorder = RecordingDrainer::failing(&failure_config);
	let timings = Arc::new(BuildTimings::new());
	let options = ProjectOptions {
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &options, recorder.clone());
	// `failingSatisfiedValidationDrainer`
	let invalid = restored_config.clone();
	coordinator.satisfied_validator = Some(Box::new(move |project| {
		(project.config_path == invalid).then(|| SolutionSelectionError {
			config_path: project.config_path.clone(),
			code: "SOLUTION_SATISFIED_OUTPUT_INVALID",
			message: "restored output is invalid".to_owned(),
		})
	}));
	let output = select(
		&mut coordinator,
		&[dependent_config.clone(), success_config.clone(), failure_config.clone()],
		std::slice::from_ref(&restored_config),
	);
	assert!(
		output.error.is_some(),
		"DrainWithProjectResultsForSelection unexpectedly succeeded"
	);
	let results = results_by_config(&output.project_results);
	assert_eq!(
		results.len(),
		4,
		"project results = {:?}, want one terminal result per owned config",
		output.project_results
	);
	let got = &results[&restored_config];
	assert!(
		got.status == SolutionProjectStatus::Failed
			&& got.diagnostics.len() == 1
			&& got.diagnostics[0].code == "SOLUTION_SATISFIED_OUTPUT_INVALID",
		"restored result = {got:?}, want owned validation failure"
	);
	let got = &results[&dependent_config];
	assert!(
		got.status == SolutionProjectStatus::Blocked
			&& got.blockers == [restored_config.clone()]
			&& got.diagnostics.is_empty(),
		"dependent result = {got:?}, want blocked only by restored project"
	);
	let got = &results[&success_config];
	assert_eq!(
		got.status,
		SolutionProjectStatus::Success,
		"unrelated success result = {got:?}, want success"
	);
	let got = &results[&failure_config];
	assert!(
		got.status == SolutionProjectStatus::Failed && got.diagnostics.len() == 1,
		"unrelated failure result = {got:?}, want normal compiler failure"
	);
	assert_drained_project_names(&recorder, &["failure", "success"]);
	let counts = timings.report().counts;
	assert_eq!(
		[
			counts.selected_projects,
			counts.satisfied_projects,
			counts.scheduled_projects
		],
		[3, 1, 3],
		"selection telemetry"
	);
}

#[test]
fn solution_coordinator_rejects_overlapping_output_directories_without_mutation() {
	let temp = Temp::new();
	let root = temp.dir();
	let app_dir = join(&root, &["app"]);
	let shared_dir = join(&root, &["shared"]);
	let selected_out = join(&app_dir, &["out"]);
	let satisfied_out = join(&selected_out, &["restored"]);
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_metadata_solution_project(&shared_dir, &satisfied_out);
	write_metadata_solution_project(&app_dir, &selected_out);
	std::fs::write(join(&app_dir, &["package.json"]), r#"{"name":"@scope/app"}"#).expect("write");
	let app_config_path = join(&app_dir, &["tsconfig.json"]);
	let app_config = std::fs::read_to_string(&app_config_path).expect("read").replacen(
		'{',
		r#"{"references":[{"path":"../shared"}],"#,
		1,
	);
	std::fs::write(&app_config_path, app_config).expect("write");

	let root_config = join(&root, &["tsconfig.json"]);
	let shared_config = join(&shared_dir, &["tsconfig.json"]);
	let graph = build_solution_graph(&root_config, &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("BuildSolutionGraph: {error}"));
	let mut protected = Vec::new();
	for project in &graph.projects {
		if project.config_path == shared_config {
			protected = project_restored_declarations(project).unwrap_or_else(|error| panic!("{error}"));
		}
	}
	assert!(!protected.is_empty(), "shared project has no restored declarations");
	protected.push(join(&satisfied_out, &["nested", "orphan.luau"]));
	let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
	for (index, path) in protected.iter().enumerate() {
		std::fs::create_dir_all(rotor_support::gopath::dir(path)).expect("mkdir");
		let content: &[u8] = if index == protected.len() - 1 {
			b"-- restored orphan\n"
		} else {
			b"export declare const restored: number;\n"
		};
		std::fs::write(path, content).expect("write");
		std::fs::File::options()
			.write(true)
			.open(path)
			.and_then(|file| file.set_modified(stamp))
			.expect("chtimes");
	}
	let state = |path: &str| {
		let content = std::fs::read(path).unwrap_or_else(|error| panic!("protected output {path}: {error}"));
		let modified = std::fs::metadata(path)
			.and_then(|info| info.modified())
			.unwrap_or_else(|error| panic!("stat protected output {path}: {error}"));
		(content, modified)
	};
	let before: HashMap<String, _> = protected.iter().map(|path| (path.clone(), state(path))).collect();
	let error = large_stack(|| SolutionCoordinator::new(&root_config, &ProjectOptions::default()))
		.err()
		.map(|error| error.to_string());
	assert!(
		error
			.as_deref()
			.is_some_and(|error| error.contains("overlapping output directories")),
		"configuration error = {error:?}, want overlapping output directories"
	);
	for path in &protected {
		assert!(state(path) == before[path], "protected output {path} changed");
	}
}

/// A coordinator over `root`'s solution with a recording drainer.
fn recording(root: &str) -> (SolutionCoordinator, Arc<RecordingDrainer>) {
	let drainer = Arc::new(RecordingDrainer::default());
	let coordinator = coordinator_with(
		&join(root, &["tsconfig.json"]),
		&ProjectOptions::default(),
		drainer.clone(),
	);
	(coordinator, drainer)
}

#[test]
fn invalid_ownership_claims_keep_every_selected_project_terminal() {
	// overlap
	{
		let temp = Temp::new();
		let root = temp.dir();
		write_solution_config(&root, "tsconfig.json", &["./first", "./second"], true);
		write_solution_config(&join(&root, &["first"]), "tsconfig.json", &[], false);
		write_solution_config(&join(&root, &["second"]), "tsconfig.json", &[], false);
		let first_config = join(&root, &["first", "tsconfig.json"]);
		let second_config = join(&root, &["second", "tsconfig.json"]);
		let (mut coordinator, drainer) = recording(&root);
		let output = select(
			&mut coordinator,
			&[first_config.clone(), second_config.clone()],
			&[join(&root, &["first", ".", "tsconfig.json"])],
		);
		assert!(output.error.is_some(), "overlapping ownership unexpectedly succeeded");
		let results = results_by_config(&output.project_results);
		assert!(
			results.len() == 2
				&& results[&first_config].status == SolutionProjectStatus::Failed
				&& results[&second_config].status == SolutionProjectStatus::Success,
			"project results = {:?}, want failed overlap and successful unrelated selected project",
			output.project_results
		);
		assert_drained_project_names(&drainer, &["second"]);
	}
	// unknown
	{
		let temp = Temp::new();
		let root = temp.dir();
		write_solution_config(&root, "tsconfig.json", &["./child"], true);
		write_solution_config(&join(&root, &["child"]), "tsconfig.json", &[], false);
		let child_config = join(&root, &["child", "tsconfig.json"]);
		let unknown_config = join(&root, &["unknown", "tsconfig.json"]);
		let (mut coordinator, drainer) = recording(&root);
		let output = select(&mut coordinator, &[unknown_config.clone(), child_config.clone()], &[]);
		assert!(output.error.is_some(), "unknown ownership unexpectedly succeeded");
		let results = results_by_config(&output.project_results);
		assert!(
			results.len() == 2
				&& results[&unknown_config].status == SolutionProjectStatus::Failed
				&& results[&child_config].status == SolutionProjectStatus::Success,
			"project results = {:?}, want failed unknown and successful valid selected project",
			output.project_results
		);
		assert_drained_project_names(&drainer, &["child"]);
	}
	// coordinator
	{
		let temp = Temp::new();
		let root = temp.dir();
		let root_config = join(&root, &["tsconfig.json"]);
		write_solution_config(&root, "tsconfig.json", &["./child"], true);
		write_solution_config(&join(&root, &["child"]), "tsconfig.json", &[], false);
		let child_config = join(&root, &["child", "tsconfig.json"]);
		let (mut coordinator, drainer) = recording(&root);
		let output = select(&mut coordinator, &[root_config.clone(), child_config.clone()], &[]);
		assert!(output.error.is_some(), "coordinator ownership unexpectedly succeeded");
		let results = results_by_config(&output.project_results);
		assert!(
			results.len() == 2
				&& results[&root_config].status == SolutionProjectStatus::Failed
				&& results[&child_config].status == SolutionProjectStatus::Success,
			"project results = {:?}, want failed coordinator claim and successful child",
			output.project_results
		);
		assert_drained_project_names(&drainer, &["child"]);
	}
	// missing dependency
	{
		let temp = Temp::new();
		let root = temp.dir();
		write_solution_config(&root, "tsconfig.json", &["./dependent", "./unrelated"], true);
		write_solution_config(&join(&root, &["shared"]), "tsconfig.json", &[], false);
		write_solution_config(&join(&root, &["app"]), "tsconfig.json", &["../shared"], false);
		write_solution_config(&join(&root, &["dependent"]), "tsconfig.json", &["../app"], false);
		write_solution_config(&join(&root, &["unrelated"]), "tsconfig.json", &[], false);
		let app_config = join(&root, &["app", "tsconfig.json"]);
		let dependent_config = join(&root, &["dependent", "tsconfig.json"]);
		let unrelated_config = join(&root, &["unrelated", "tsconfig.json"]);
		let (mut coordinator, drainer) = recording(&root);
		let output = select(
			&mut coordinator,
			&[app_config.clone(), dependent_config.clone(), unrelated_config.clone()],
			&[],
		);
		assert!(
			output.error.is_some(),
			"missing dependency ownership unexpectedly succeeded"
		);
		let results = results_by_config(&output.project_results);
		assert!(
			results.len() == 3
				&& results[&app_config].status == SolutionProjectStatus::Failed
				&& results[&dependent_config].status == SolutionProjectStatus::Blocked
				&& results[&unrelated_config].status == SolutionProjectStatus::Success,
			"project results = {:?}, want failed owner, blocked dependant, and successful unrelated project",
			output.project_results
		);
		assert_eq!(results[&dependent_config].blockers, [app_config], "dependent blockers");
		assert_drained_project_names(&drainer, &["unrelated"]);
	}
}

/// The single-failure checks the rejection tests share.
fn assert_rejected(
	output: &DrainOutput,
	drainer: &RecordingDrainer,
	error_part: &str,
	config: Option<&str>,
	code: &str,
) {
	let error = output.error.as_ref().map(ToString::to_string);
	assert!(
		error.as_deref().is_some_and(|error| error.contains(error_part)),
		"selection error = {error:?}, want {error_part}"
	);
	assert!(
		drainer.drained().is_empty(),
		"drained projects = {:?}, want none",
		drainer.drained()
	);
	let projects = &output.project_results;
	assert!(
		projects.len() == 1
			&& config.is_none_or(|config| projects[0].config_path == config)
			&& projects[0].status == SolutionProjectStatus::Failed,
		"project result = {projects:?}, want one failed result owned by {config:?}"
	);
	assert!(
		projects[0].diagnostics.len() == 1 && projects[0].diagnostics[0].code == code,
		"project diagnostics = {:?}, want structured {code} diagnostic",
		projects[0].diagnostics
	);
}

#[test]
fn solution_coordinator_rejects_overlapping_ownership_before_scheduling() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &[], false);
	let (mut coordinator, drainer) = recording(&root);
	let config = join(&root, &["tsconfig.json"]);
	let output = select(
		&mut coordinator,
		std::slice::from_ref(&config),
		&[join(&root, &[".", "tsconfig.json"])],
	);
	assert_rejected(
		&output,
		&drainer,
		"both selected and satisfied",
		Some(&config),
		"SOLUTION_SELECTION_OVERLAP",
	);
}

#[test]
fn solution_coordinator_rejects_unknown_ownership_before_scheduling() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &[], false);
	let (mut coordinator, drainer) = recording(&root);
	let unknown = join(&root, &["unknown", "tsconfig.json"]);
	let output = select(&mut coordinator, std::slice::from_ref(&unknown), &[]);
	assert_rejected(
		&output,
		&drainer,
		"not part of the discovered solution graph",
		Some(&unknown),
		"SOLUTION_SELECTION_UNKNOWN",
	);
}

#[test]
fn solution_coordinator_rejects_coordinator_only_ownership_before_scheduling() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	write_solution_config(&join(&root, &["child"]), "tsconfig.json", &[], false);
	let (mut coordinator, drainer) = recording(&root);
	let config = join(&root, &["tsconfig.json"]);
	let output = select(&mut coordinator, std::slice::from_ref(&config), &[]);
	assert_rejected(
		&output,
		&drainer,
		"coordinator-only",
		Some(&config),
		"SOLUTION_SELECTION_COORDINATOR",
	);
}

#[test]
fn solution_coordinator_rejects_missing_dependency_ownership_before_scheduling() {
	let temp = Temp::new();
	let root = temp.dir();
	let (app_config, shared_config) = app_shared_solution(&root);
	let (mut coordinator, drainer) = recording(&root);
	let output = select(&mut coordinator, std::slice::from_ref(&app_config), &[]);
	let error = output.error.as_ref().map(ToString::to_string).unwrap_or_default();
	assert!(
		error.contains(&shared_config) && error.contains("has no owner"),
		"selection error = {error:?}, want missing dependency ownership for {shared_config}"
	);
	assert_rejected(
		&output,
		&drainer,
		"has no owner",
		Some(&app_config),
		"SOLUTION_SELECTION_MISSING_DEPENDENCY",
	);
}

#[test]
fn solution_coordinator_rejects_empty_ownership_claim_before_scheduling() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &[], false);
	let (mut coordinator, drainer) = recording(&root);
	let output = select(&mut coordinator, &[], &[String::new()]);
	assert_rejected(
		&output,
		&drainer,
		"non-empty path",
		None,
		"SOLUTION_SELECTION_INVALID_PATH",
	);
}
