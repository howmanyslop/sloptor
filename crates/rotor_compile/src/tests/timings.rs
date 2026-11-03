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

#[test]
#[ignore = "needs 5b: a transformer plugin through the sidecar"]
fn build_timings_transformer_build_records_separate_sidecar_and_overlay_stages() {}

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
#[ignore = "needs 5b: recordPreparedTransformerProgram records the sidecar flags"]
fn build_timings_collector_sidecar_flags_or_across_children() {}

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

#[test]
#[ignore = "needs 5b: the sidecar response and its metrics"]
fn sidecar_metrics_json_round_trip() {}

#[test]
#[ignore = "needs 5b: the callback transformer host"]
fn transformer_phase_metrics_verbose_gate() {}

#[test]
#[ignore = "needs 5b: a transformer plugin through the sidecar"]
fn verbose_sidecar_stages_transformer_build_prints_every_sidecar_stage() {}

#[test]
#[ignore = "needs 5b: a transformer plugin through the sidecar"]
fn verbose_sidecar_stages_transformer_build_charges_the_round_trip_to_its_plugins() {}
