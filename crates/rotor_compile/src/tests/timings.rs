//! `timings_test.go`
//!
//! Go runs the collector tests under `synctest`'s fake clock; these sleep for real, and every
//! assertion is a lower bound, so the real clock only makes them slower.

use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use super::output::build;
use super::write_project;
use crate::ProjectOptions;
use crate::timings::{
	BUILD_TIMING_SCHEMA_VERSION, BuildTimings, BuildTimingsReport, STAGE_SEMANTICS_WORK_MS, Stage, start_stage,
	timed_persist,
};

fn timed(dir: &str) -> Arc<BuildTimings> {
	let timings = Arc::new(BuildTimings::new());
	build(
		dir,
		&ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		},
	);
	timings
}

#[test]
fn build_timings_unchanged_rebuild_performs_zero_output_writes() {
	let temp = write_project("@scope/timings-warm", "");
	build(&temp.dir(), &ProjectOptions::default());

	let counts = timed(&temp.dir()).report().counts;
	assert_eq!(counts.actual_writes, 0, "actual writes");
	assert_ne!(counts.hash_skips, 0, "hash skips = 0, want at least one");
}

#[test]
fn build_timings_pluginless_build_records_zero_sidecar_and_overlay_stages() {
	let temp = write_project("@scope/timings-pluginless", "");

	let report = timed(&temp.dir()).report();

	assert_eq!(report.schema_version, BUILD_TIMING_SCHEMA_VERSION, "schemaVersion");
	assert!(
		report.stages.sidecar_round_trip_ms == 0 && report.stages.overlay_program_ms == 0,
		"pluginless sidecar stages = {:?}, want zero",
		report.stages
	);
	assert!(
		report.counts.total_sources == 1 && report.counts.selected_sources == 1,
		"source counts = {:?}, want one total and selected source",
		report.counts
	);
	assert_build_timings_non_negative(&report);
}

/// The fixture the transformer timing tests build: `prefixStringPlugin` over one file.
fn write_timings_transformer_project(pkg_name: &str, prefix: &str) -> super::Temp {
	super::sidecar::set_repo_sidecar_path();
	let temp = write_project(pkg_name, "");
	super::sidecar::write_sidecar_plugin_fixture(
		&temp.dir(),
		"",
		&super::sidecar::plugin_config(&format!(
			r#"[{{"transform": "./plugins/prefix-string.js", "prefix": "{prefix}"}}]"#
		)),
	);
	temp.write("src/main.ts", "export const value = \"value\";\n");
	temp
}

#[test]
fn build_timings_transformer_build_records_separate_sidecar_and_overlay_stages() {
	// Given
	let _sessions = super::sidecar::SessionsGuard::new();
	let temp = write_timings_transformer_project("@scope/timings-transformer", "timings");
	let timings = Arc::new(BuildTimings::new());
	// When
	let opts = ProjectOptions {
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let (_, diags, error) = super::sidecar::build(&temp.dir(), &opts);
	// Then
	assert!(error.is_none(), "BuildProjectWithOptions: {error:?} (diags: {diags:?})");
	assert_eq!(
		timings.sidecar_flags(),
		(true, true),
		"transformer stages were not separately recorded: {:?}",
		timings.report().stages
	);
	assert_build_timings_non_negative(&timings.report());
}

/// `assertBuildTimingsNonNegative`
fn assert_build_timings_non_negative(report: &BuildTimingsReport) {
	let stages = &report.stages;
	for (label, duration) in [
		("total", report.total_ms),
		("initial program", stages.initial_program_ms),
		("incremental selection", stages.incremental_selection_ms),
		("declaration emit", stages.declaration_emit_ms),
		("cleanup", stages.cleanup_ms),
		("include copy", stages.include_copy_ms),
		("non-compiled copy", stages.non_compiled_copy_ms),
		("sidecar wait", stages.sidecar_session_wait_ms),
		("sidecar preparation", stages.sidecar_preparation_ms),
		("sidecar round trip", stages.sidecar_round_trip_ms),
		("sidecar decode", stages.sidecar_response_decode_ms),
		("overlay program", stages.overlay_program_ms),
		("project context", stages.project_context_ms),
		("semantic diagnostics", stages.semantic_diagnostics_ms),
		("transform/render", stages.native_transform_render_ms),
		("compiled output writes", stages.compiled_output_writes_ms),
		("declaration emit writes", stages.declaration_emit_writes_ms),
		("incremental manifest", stages.incremental_manifest_ms),
		("persistence", stages.persistence_ms),
	] {
		assert!(duration >= 0, "{label} = {duration}ms, want nonnegative");
	}
}

fn config_paths(paths: &[&str]) -> Vec<String> {
	paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn build_timings_collector_child_finish_does_not_set_parent_total_ms() {
	let parent = Arc::new(BuildTimings::new());
	parent.init_projects(&config_paths(&["/a/tsconfig.json", "/b/tsconfig.json"]));
	let first = parent.new_project("/a/tsconfig.json");
	let second = parent.new_project("/b/tsconfig.json");
	first.finish();
	sleep(Duration::from_millis(100));
	second.finish();
	parent.finish();
	let parent_total = parent.report().total_ms;
	let first_total = first.report().total_ms;
	assert!(
		parent_total >= 100,
		"parent totalMs = {parent_total}, want at least 100 after last child"
	);
	assert!(
		first_total < parent_total,
		"first child totalMs {first_total} should be smaller than parent {parent_total}"
	);
}

#[test]
fn build_timings_collector_counts_from_concurrent_children_are_summed() {
	let parent = Arc::new(BuildTimings::new());
	parent.init_projects(&config_paths(&["/z/tsconfig.json", "/a/tsconfig.json"]));
	let first = parent.new_project("/z/tsconfig.json");
	let second = parent.new_project("/a/tsconfig.json");
	first.set_source_counts(3, 2);
	first.set_emitted_entries(2);
	second.set_source_counts(4, 1);
	second.set_emitted_entries(1);
	second.finish();
	first.finish();
	let report = parent.report();
	assert!(
		report.counts.total_sources == 7 && report.counts.selected_sources == 3,
		"summed sources = {:?}, want total 7 selected 3",
		report.counts
	);
	assert_eq!(report.counts.emitted_entries, 3, "emitted");
	let got: Vec<&str> = report
		.projects
		.iter()
		.map(|project| project.config_path.as_str())
		.collect();
	assert_eq!(
		got,
		["/z/tsconfig.json", "/a/tsconfig.json"],
		"project order, want graph order"
	);
}

#[test]
fn build_timings_collector_overlapping_stage_work_may_exceed_wall_time() {
	let parent = Arc::new(BuildTimings::new());
	let first = parent.new_project("/a/tsconfig.json");
	let second = parent.new_project("/b/tsconfig.json");
	let stop_first = start_stage(Some(&first), Stage::InitialProgram);
	let stop_second = start_stage(Some(&second), Stage::InitialProgram);
	sleep(Duration::from_millis(50));
	stop_first.stop();
	stop_second.stop();
	first.finish();
	second.finish();
	parent.finish();
	let report = parent.report();
	assert!(
		report.stages.initial_program_ms >= 100,
		"work ms = {}, want at least 100",
		report.stages.initial_program_ms
	);
	assert!(report.total_ms >= 50, "totalMs = {}, want at least 50", report.total_ms);
	assert_eq!(report.stage_semantics, STAGE_SEMANTICS_WORK_MS, "stageSemantics");
}

#[test]
fn build_timings_collector_sidecar_flags_or_across_children() {
	let temp = write_project("@scope/timings-flags", "");
	let program = super::large_stack(|| {
		crate::project::new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("{}", failure.error))
			.1
	});
	let parent = Arc::new(BuildTimings::new());
	let pluginless = parent.new_project("/plain/tsconfig.json");
	pluginless.record_prepared_transformer_program(&crate::sidecar::PreparedTransformerProgram::plain(
		program,
		Vec::new(),
		None,
	));
	pluginless.finish();
	let transformer = parent.new_project("/plugin/tsconfig.json");
	let mut prepared = crate::sidecar::PreparedTransformerProgram::plain(program, Vec::new(), None);
	prepared.sidecar_round_trip_recorded = true;
	prepared.overlay_program_recorded = true;
	prepared.stats.round_trip = Duration::from_millis(1);
	transformer.record_prepared_transformer_program(&prepared);
	transformer.finish();
	let (round_trip, overlay) = parent.sidecar_flags();
	assert!(
		round_trip && overlay,
		"parent sidecar flags = roundTrip {round_trip} overlay {overlay}, want both true"
	);
}

#[test]
fn build_timings_collector_deferred_persist_is_measured_after_child_finish() {
	let parent = Arc::new(BuildTimings::new());
	parent.init_projects(&config_paths(&["/a/tsconfig.json"]));
	let child = parent.new_project("/a/tsconfig.json");
	child.finish();
	let persist = timed_persist(Some(&child), || {
		sleep(Duration::from_millis(50));
		Ok(())
	});
	persist().unwrap_or_else(|error| panic!("{error}"));
	parent.finish();
	let child_report = child.report();
	let report = parent.report();
	assert!(
		child_report.stages.persistence_ms >= 50 && report.stages.persistence_ms >= 50,
		"persistence child={} parent={}, want at least 50",
		child_report.stages.persistence_ms,
		report.stages.persistence_ms
	);
	assert!(
		report.projects[0].stages.persistence_ms >= 50,
		"project snapshot persistence = {}, want at least 50",
		report.projects[0].stages.persistence_ms
	);
	assert!(
		report.total_ms >= 50,
		"parent totalMs = {}, want persist included",
		report.total_ms
	);
}

/// The port decodes a worker response but never encodes one (Go's `json.Marshal` half of the
/// round trip has no Rust caller), so the decode is what is pinned.
#[test]
fn sidecar_metrics_json_round_trip() {
	use crate::transformer_host::{TransformerPhaseMetrics, TransformerResponse};
	let plain = TransformerResponse::from_json(r#"{"diagnostics":[],"transformed":[],"declarations":[]}"#)
		.unwrap_or_else(|error| panic!("unmarshal without metrics: {error}"));
	assert!(plain.metrics.is_none(), "metrics = {:?}, want nil", plain.metrics);
	let metered = TransformerResponse::from_json(
		r#"{"diagnostics":[],"transformed":[],"metrics":{"wallMs":12,"cpuUserUs":3,"cpuSystemUs":4,"nodeVersion":"v22.0.0"}}"#,
	)
	.unwrap_or_else(|error| panic!("unmarshal with metrics: {error}"));
	let metrics = metered.metrics.expect("metrics");
	assert!(
		metrics.wall_ms == 12 && metrics.node_version == "v22.0.0",
		"metrics = {metrics:?}"
	);
	assert!(
		metrics.phases.is_none(),
		"phases = {:?}, want nil for older worker",
		metrics.phases
	);
	let metered = TransformerResponse::from_json(
		r#"{"diagnostics":[],"transformed":[],"metrics":{"wallMs":21,"phases":{"configMs":1,"programMs":2,"factoriesMs":3,"transformMs":9,"parentRepairMs":5,"printMs":6,"futureMs":7},"futureMetric":8}}"#,
	)
	.unwrap_or_else(|error| panic!("unmarshal with phases and future fields: {error}"));
	let want = TransformerPhaseMetrics {
		config_ms: 1,
		program_ms: 2,
		factories_ms: 3,
		transform_ms: 9,
		parent_repair_ms: 5,
		print_ms: 6,
	};
	assert_eq!(metered.metrics.and_then(|metrics| metrics.phases), Some(want), "phases");
}

#[test]
fn transformer_phase_metrics_verbose_gate() {
	use crate::transformer_host::{
		TransformerHost, TransformerPhaseMetrics, TransformerResponse, TransformerResponseMetrics,
	};
	let temp = super::Temp::new();
	let dir = temp.dir();
	let host = crate::transformer_host_sidecar::CallbackTransformerHost::new(Arc::new(|_, _| {
		Ok(TransformerResponse {
			metrics: Some(TransformerResponseMetrics {
				phases: Some(TransformerPhaseMetrics {
					config_ms: 1,
					program_ms: 2,
					factories_ms: 3,
					transform_ms: 9,
					parent_repair_ms: 5,
					print_ms: 6,
				}),
				..TransformerResponseMetrics::default()
			}),
			..TransformerResponse::default()
		})
	}));
	let config_path = rotor_support::gopath::join(&[&dir, "tsconfig.json"]);
	let verbose = rotor_support::logservice::verbose();
	rotor_support::logservice::set_verbose(true);
	rotor_support::logservice::capture();
	host.validate(None, &dir, &config_path)
		.unwrap_or_else(|error| panic!("{error}"));
	let logged = rotor_support::logservice::take_capture();
	for line in [
		"transformer worker config ( 1 ms )",
		"transformer worker program ( 2 ms )",
		"transformer worker factories ( 3 ms )",
		"transformer worker transform (includes parent repair) ( 9 ms )",
		"transformer worker parent repair ( 5 ms )",
		"transformer worker print/trace map ( 6 ms )",
	] {
		assert!(logged.contains(line), "missing {line:?} in log: {logged}");
	}
	rotor_support::logservice::set_verbose(false);
	rotor_support::logservice::capture();
	host.validate(None, &dir, &config_path)
		.unwrap_or_else(|error| panic!("{error}"));
	let logged = rotor_support::logservice::take_capture();
	rotor_support::logservice::set_verbose(verbose);
	assert!(logged.is_empty(), "non-verbose phase log = {logged:?}, want empty");
}

/// `buildVerboseTransformerProject`: the verbose log of one transformer build.
fn build_verbose_transformer_project() -> String {
	let _sessions = super::sidecar::SessionsGuard::new();
	let temp = write_timings_transformer_project("@scope/verbose-sidecar", "verbose");
	let verbose = rotor_support::logservice::verbose();
	rotor_support::logservice::set_verbose(true);
	rotor_support::logservice::capture();
	let opts = ProjectOptions {
		timings: Some(Arc::new(BuildTimings::new())),
		..ProjectOptions::default()
	};
	let (_, diags, error) = super::sidecar::build(&temp.dir(), &opts);
	let logged = rotor_support::logservice::take_capture();
	rotor_support::logservice::set_verbose(verbose);
	assert!(error.is_none(), "BuildProjectWithOptions: {error:?} (diags: {diags:?})");
	logged
}

/// `assertStageLogged`: a start line and a completion line for `name`.
fn assert_stage_logged(logged: &str, name: &str) {
	let lines: Vec<&str> = logged.lines().collect();
	let strip = |line: &str| -> String { line.split_once(": ").map_or(line, |(_, rest)| rest).to_owned() };
	let started = lines
		.iter()
		.any(|line| *line == format!("{name}...") || strip(line) == format!("{name}..."));
	assert!(started, "missing start line for stage {name:?}; log: {logged}");
	let completed = lines.iter().any(|line| {
		let rest = if line.starts_with(name) {
			(*line).to_owned()
		} else {
			strip(line)
		};
		rest.strip_prefix(name)
			.and_then(|tail| tail.strip_prefix(" ( "))
			.and_then(|tail| tail.strip_suffix(" ms )"))
			.is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
	});
	assert!(completed, "missing completion line for stage {name:?}; log: {logged}");
}

#[test]
fn verbose_sidecar_stages_transformer_build_prints_every_sidecar_stage() {
	use crate::timings::Stage;
	let logged = build_verbose_transformer_project();
	for name in [
		Stage::SidecarSessionWait.trace_name().to_owned(),
		Stage::SidecarPreparation.trace_name().to_owned(),
		format!("{} (./plugins/prefix-string.js)", Stage::SidecarRoundTrip.trace_name()),
		Stage::SidecarResponseDecode.trace_name().to_owned(),
		Stage::OverlayProgram.trace_name().to_owned(),
	] {
		assert_stage_logged(&logged, &name);
	}
}

#[test]
fn verbose_sidecar_stages_transformer_build_charges_the_round_trip_to_its_plugins() {
	let logged = build_verbose_transformer_project();
	let ms_line = |line: &str, label: &str| -> bool {
		line.split_once(": ")
			.and_then(|(_, rest)| rest.strip_prefix(label))
			.and_then(|tail| tail.strip_prefix(" ( "))
			.and_then(|tail| tail.strip_suffix(" ms )"))
			.is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
	};
	assert!(
		logged
			.lines()
			.any(|line| ms_line(line, "transformer plugin ./plugins/prefix-string.js")),
		"missing per-plugin line; log: {logged}"
	);
	for phase in [
		"config",
		"program",
		"factories",
		"transform (includes parent repair)",
		"parent repair",
		"print/trace map",
	] {
		let label = format!("transformer worker {phase}");
		assert!(
			logged.lines().any(|line| ms_line(line, &label)),
			"missing worker phase {phase:?}; log: {logged}"
		);
	}
}
