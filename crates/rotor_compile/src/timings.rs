//! `timings.go`: the build timing report (`--timings`). Top-level `totalMs` is elapsed wall
//! time; stage values are aggregate work milliseconds, so concurrent projects can make their sum
//! exceed it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rotor_support::{gopath, logservice};

use crate::gojson;
use crate::project::lock;

/// `BuildTimingSchemaVersion`
pub const BUILD_TIMING_SCHEMA_VERSION: i64 = 2;
/// `StageSemanticsWorkMs`
pub const STAGE_SEMANTICS_WORK_MS: &str = "workMs";
/// `ProjectTimingStatusBuilt`
pub const PROJECT_TIMING_STATUS_BUILT: &str = "built";
/// `ProjectTimingStatusSkipped`
pub const PROJECT_TIMING_STATUS_SKIPPED: &str = "skipped";
/// `ProjectTimingStatusBlocked`
pub const PROJECT_TIMING_STATUS_BLOCKED: &str = "blocked";
/// `ProjectTimingStatusFailed`
pub const PROJECT_TIMING_STATUS_FAILED: &str = "failed";

/// `buildTimingStage`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
	/// `initialProgramStage`
	InitialProgram,
	/// `incrementalSelectionStage`
	IncrementalSelection,
	/// `cleanupStage`
	Cleanup,
	/// `includeCopyStage`
	IncludeCopy,
	/// `nonCompiledCopyStage`
	NonCompiledCopy,
	/// `sidecarSessionWaitStage`
	SidecarSessionWait,
	/// `sidecarPreparationStage`
	SidecarPreparation,
	/// `sidecarRoundTripStage`
	SidecarRoundTrip,
	/// `sidecarResponseDecodeStage`
	SidecarResponseDecode,
	/// `overlayProgramStage`
	OverlayProgram,
	/// `projectContextStage`
	ProjectContext,
	/// `semanticDiagnosticsStage`
	SemanticDiagnostics,
	/// `nativeTransformRenderStage`
	NativeTransformRender,
	/// `compiledOutputWritesStage`
	CompiledOutputWrites,
	/// `declarationEmitWritesStage`
	DeclarationEmitWrites,
	/// `declarationEmitStage`
	DeclarationEmit,
	/// `incrementalManifestStage`
	IncrementalManifest,
	/// `persistenceStage`
	Persistence,
}

impl Stage {
	/// `traceName`: the verbose and trace label.
	#[must_use]
	pub const fn trace_name(self) -> &'static str {
		match self {
			Self::InitialProgram => "program creation and parse/load",
			Self::IncrementalSelection => "incremental selection",
			Self::Cleanup => "cleanup",
			Self::IncludeCopy => "copy include files",
			Self::NonCompiledCopy => "copy non-compiled files",
			Self::SidecarSessionWait => "transformer sidecar wait",
			Self::SidecarPreparation => "transformer sidecar preparation",
			Self::SidecarRoundTrip => "transformer sidecar",
			Self::SidecarResponseDecode => "transformer sidecar decode",
			Self::OverlayProgram => "overlay program creation and parse/load",
			Self::ProjectContext => "project context",
			Self::SemanticDiagnostics => "semantic diagnostics",
			Self::NativeTransformRender => "transform/render",
			Self::CompiledOutputWrites => "compiled output writes",
			Self::DeclarationEmitWrites => "declaration emit/write",
			Self::DeclarationEmit => "declaration emit",
			Self::IncrementalManifest => "incremental manifest",
			Self::Persistence => "persistence",
		}
	}
}

/// `BuildTimingStages`: milliseconds per stage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(missing_docs, reason = "one field per stage, named as in the JSON report")]
pub struct BuildTimingStages {
	pub initial_program_ms: i64,
	pub incremental_selection_ms: i64,
	pub cleanup_ms: i64,
	pub include_copy_ms: i64,
	pub non_compiled_copy_ms: i64,
	pub sidecar_session_wait_ms: i64,
	pub sidecar_preparation_ms: i64,
	pub sidecar_round_trip_ms: i64,
	pub sidecar_response_decode_ms: i64,
	pub overlay_program_ms: i64,
	pub project_context_ms: i64,
	pub semantic_diagnostics_ms: i64,
	pub native_transform_render_ms: i64,
	pub compiled_output_writes_ms: i64,
	pub declaration_emit_writes_ms: i64,
	pub declaration_emit_ms: i64,
	pub incremental_manifest_ms: i64,
	pub persistence_ms: i64,
}

impl BuildTimingStages {
	const fn field(&mut self, stage: Stage) -> &mut i64 {
		match stage {
			Stage::InitialProgram => &mut self.initial_program_ms,
			Stage::IncrementalSelection => &mut self.incremental_selection_ms,
			Stage::Cleanup => &mut self.cleanup_ms,
			Stage::IncludeCopy => &mut self.include_copy_ms,
			Stage::NonCompiledCopy => &mut self.non_compiled_copy_ms,
			Stage::SidecarSessionWait => &mut self.sidecar_session_wait_ms,
			Stage::SidecarPreparation => &mut self.sidecar_preparation_ms,
			Stage::SidecarRoundTrip => &mut self.sidecar_round_trip_ms,
			Stage::SidecarResponseDecode => &mut self.sidecar_response_decode_ms,
			Stage::OverlayProgram => &mut self.overlay_program_ms,
			Stage::ProjectContext => &mut self.project_context_ms,
			Stage::SemanticDiagnostics => &mut self.semantic_diagnostics_ms,
			Stage::NativeTransformRender => &mut self.native_transform_render_ms,
			Stage::CompiledOutputWrites => &mut self.compiled_output_writes_ms,
			Stage::DeclarationEmitWrites => &mut self.declaration_emit_writes_ms,
			Stage::DeclarationEmit => &mut self.declaration_emit_ms,
			Stage::IncrementalManifest => &mut self.incremental_manifest_ms,
			Stage::Persistence => &mut self.persistence_ms,
		}
	}

	/// The stages as Go's `json.Marshal` writes them.
	#[must_use]
	pub fn to_json(&self) -> String {
		gojson::Object::new()
			.int("initialProgramMs", self.initial_program_ms)
			.int("incrementalSelectionMs", self.incremental_selection_ms)
			.int("cleanupMs", self.cleanup_ms)
			.int("includeCopyMs", self.include_copy_ms)
			.int("nonCompiledCopyMs", self.non_compiled_copy_ms)
			.int("sidecarSessionWaitMs", self.sidecar_session_wait_ms)
			.int("sidecarPreparationMs", self.sidecar_preparation_ms)
			.int("sidecarRoundTripMs", self.sidecar_round_trip_ms)
			.int("sidecarResponseDecodeMs", self.sidecar_response_decode_ms)
			.int("overlayProgramMs", self.overlay_program_ms)
			.int("projectContextMs", self.project_context_ms)
			.int("semanticDiagnosticsMs", self.semantic_diagnostics_ms)
			.int("nativeTransformRenderMs", self.native_transform_render_ms)
			.int("compiledOutputWritesMs", self.compiled_output_writes_ms)
			.int("declarationEmitWritesMs", self.declaration_emit_writes_ms)
			.int("declarationEmitMs", self.declaration_emit_ms)
			.int("incrementalManifestMs", self.incremental_manifest_ms)
			.int("persistenceMs", self.persistence_ms)
			.finish()
	}
}

/// `BuildTimingCounts`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(missing_docs, reason = "one field per count, named as in the JSON report")]
pub struct BuildTimingCounts {
	pub scheduled_projects: i64,
	pub selected_projects: i64,
	pub satisfied_projects: i64,
	pub transformed_projects: i64,
	pub emitted_projects: i64,
	pub total_sources: i64,
	pub selected_sources: i64,
	pub emitted_entries: i64,
	pub scheduled_source_map_writes: i64,
	pub scheduled_declaration_writes: i64,
	pub actual_writes: i64,
	pub hash_skips: i64,
	pub unique_prepared_directories: i64,
	pub effective_write_workers: i64,
	pub sidecar_request_bytes: i64,
	pub sidecar_response_bytes: i64,
	pub sidecar_stats: i64,
	pub sidecar_source_reads: i64,
	pub sidecar_changed_files: i64,
	pub sidecar_spawns: i64,
	pub sidecar_restarts: i64,
	pub node_wall_ms: i64,
	pub node_cpu_user_us: i64,
	pub node_cpu_system_us: i64,
	pub parse_cache_hits: i64,
	pub parse_cache_misses: i64,
}

impl BuildTimingCounts {
	/// The counts as Go's `json.Marshal` writes them (`omitempty` on the sidecar counts).
	#[must_use]
	pub fn to_json(&self) -> String {
		let mut object = gojson::Object::new()
			.int("scheduledProjects", self.scheduled_projects)
			.int("selectedProjects", self.selected_projects)
			.int("satisfiedProjects", self.satisfied_projects)
			.int("transformedProjects", self.transformed_projects)
			.int("emittedProjects", self.emitted_projects)
			.int("totalSources", self.total_sources)
			.int("selectedSources", self.selected_sources)
			.int("emittedEntries", self.emitted_entries)
			.int("scheduledSourceMapWrites", self.scheduled_source_map_writes)
			.int("scheduledDeclarationWrites", self.scheduled_declaration_writes)
			.int("actualWrites", self.actual_writes)
			.int("hashSkips", self.hash_skips)
			.int("uniquePreparedDirectories", self.unique_prepared_directories)
			.int("effectiveWriteWorkers", self.effective_write_workers);
		for (key, value) in [
			("sidecarRequestBytes", self.sidecar_request_bytes),
			("sidecarResponseBytes", self.sidecar_response_bytes),
			("sidecarStats", self.sidecar_stats),
			("sidecarSourceReads", self.sidecar_source_reads),
			("sidecarChangedFiles", self.sidecar_changed_files),
			("sidecarSpawns", self.sidecar_spawns),
			("sidecarRestarts", self.sidecar_restarts),
			("nodeWallMs", self.node_wall_ms),
			("nodeCpuUserUs", self.node_cpu_user_us),
			("nodeCpuSystemUs", self.node_cpu_system_us),
			("parseCacheHits", self.parse_cache_hits),
			("parseCacheMisses", self.parse_cache_misses),
		] {
			if value != 0 {
				object = object.int(key, value);
			}
		}
		object.finish()
	}
}

/// `ProjectBuildTimings`: one project's share of a solution report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectBuildTimings {
	/// The project's config path.
	pub config_path: String,
	/// `built`, `skipped`, `blocked`, `failed` or `satisfied`.
	pub status: String,
	/// The project that blocked this one.
	pub blocked_by: String,
	/// The project's wall time.
	pub build_wall_ms: i64,
	/// The project's stages.
	pub stages: BuildTimingStages,
	/// The project's counts.
	pub counts: BuildTimingCounts,
}

impl ProjectBuildTimings {
	/// The project as Go's `json.Marshal` writes it.
	#[must_use]
	pub fn to_json(&self) -> String {
		gojson::Object::new()
			.string("configPath", &self.config_path)
			.string("status", &self.status)
			.string_omitempty("blockedBy", &self.blocked_by)
			.int("buildWallMs", self.build_wall_ms)
			.raw("stages", &self.stages.to_json())
			.raw("counts", &self.counts.to_json())
			.finish()
	}
}

/// `BuildTimingMetadata`.
///
/// Go's runtime fields have Rust stand-ins where one exists (`goos`, `goarch` and `gomaxprocs` from
/// the target and the available parallelism); Go-only knobs (`goVersion`, `GOGC`, the memory limit)
/// are left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildTimingMetadata {
	/// The product version.
	pub version: String,
	/// The VCS revision.
	pub revision: String,
	/// Whether the build tree was modified.
	pub dirty: bool,
	/// The operating system, in Go's spelling.
	pub goos: String,
	/// The architecture, in Go's spelling.
	pub goarch: String,
	/// The available parallelism.
	pub gomaxprocs: i64,
	/// `--builders`, when given.
	pub requested_builders: Option<i64>,
	/// `--checkers`, when given.
	pub requested_checkers: Option<i64>,
	/// The effective solution builders.
	pub effective_builders: i64,
	/// The effective write workers.
	pub effective_write_workers: i64,
	/// The transformer sidecar's Node version.
	pub node_version: String,
}

impl BuildTimingMetadata {
	/// The metadata as Go's `json.Marshal` writes it (every field `omitempty`).
	#[must_use]
	pub fn to_json(&self) -> String {
		let mut object = gojson::Object::new()
			.string_omitempty("version", &self.version)
			.string_omitempty("revision", &self.revision);
		if self.dirty {
			object = object.bool("dirty", true);
		}
		object = object
			.string_omitempty("goos", &self.goos)
			.string_omitempty("goarch", &self.goarch);
		for (key, value) in [
			("gomaxprocs", Some(self.gomaxprocs).filter(|value| *value != 0)),
			("requestedBuilders", self.requested_builders),
			("requestedCheckers", self.requested_checkers),
			(
				"effectiveBuilders",
				Some(self.effective_builders).filter(|value| *value != 0),
			),
			(
				"effectiveWriteWorkers",
				Some(self.effective_write_workers).filter(|value| *value != 0),
			),
		] {
			if let Some(value) = value {
				object = object.int(key, value);
			}
		}
		object.string_omitempty("nodeVersion", &self.node_version).finish()
	}
}

/// `captureRuntimeMetadata`
fn capture_runtime_metadata() -> BuildTimingMetadata {
	let goos = match std::env::consts::OS {
		"macos" => "darwin",
		other => other,
	};
	let goarch = match std::env::consts::ARCH {
		"x86_64" => "amd64",
		"aarch64" => "arm64",
		"x86" => "386",
		other => other,
	};
	BuildTimingMetadata {
		goos: goos.to_owned(),
		goarch: goarch.to_owned(),
		gomaxprocs: std::thread::available_parallelism().map_or(1, |count| i64::try_from(count.get()).unwrap_or(1)),
		..BuildTimingMetadata::default()
	}
}

/// The report's exported fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildTimingsReport {
	/// `schemaVersion`
	pub schema_version: i64,
	/// `ok`
	pub ok: bool,
	/// `totalMs`
	pub total_ms: i64,
	/// `stageSemantics`
	pub stage_semantics: String,
	/// `stages`
	pub stages: BuildTimingStages,
	/// `counts`
	pub counts: BuildTimingCounts,
	/// `projects`
	pub projects: Vec<ProjectBuildTimings>,
	/// `metadata`
	pub metadata: BuildTimingMetadata,
}

impl BuildTimingsReport {
	/// The report as Go's `json.Marshal` writes it.
	#[must_use]
	pub fn to_json(&self) -> String {
		let mut object = gojson::Object::new()
			.int("schemaVersion", self.schema_version)
			.bool("ok", self.ok)
			.int("totalMs", self.total_ms)
			.string("stageSemantics", &self.stage_semantics)
			.raw("stages", &self.stages.to_json())
			.raw("counts", &self.counts.to_json());
		if !self.projects.is_empty() {
			object = object.raw(
				"projects",
				&gojson::array(self.projects.iter().map(ProjectBuildTimings::to_json)),
			);
		}
		object.raw("metadata", &self.metadata.to_json()).finish()
	}
}

#[derive(Default)]
struct TimingsState {
	report: BuildTimingsReport,
	finished: bool,
	config_path: String,
	status: String,
	blocked_by: String,
	build_wall_ms: i64,
	project_index: HashMap<String, usize>,
	prepared_directories: HashSet<String>,
	sidecar_round_trip_recorded: bool,
	overlay_program_recorded: bool,
	/// Time a nested stage already reported that the enclosing stage would count again.
	nested_stage_debt: HashMap<Stage, Duration>,
}

/// `BuildTimings`: a build's timing report, shared by its stages and, in a solution, linked to
/// the solution's report.
pub struct BuildTimings {
	state: Mutex<TimingsState>,
	started: Instant,
	parent: Option<Arc<Self>>,
}

impl Default for BuildTimings {
	fn default() -> Self {
		Self::new()
	}
}

/// A running stage; [`StageTimer::stop`] records it. Dropping it without stopping records
/// nothing (Go's callers always call the stop function).
#[must_use]
pub struct StageTimer<'a> {
	timings: Option<&'a BuildTimings>,
	stage: Stage,
	label: String,
	started: Instant,
}

impl StageTimer<'_> {
	/// Ends the stage and records its duration.
	pub fn stop(self) {
		let measured = self.started.elapsed();
		let duration = match self.timings {
			Some(timings) => timings.settle_nested_stage_debt(self.stage, measured),
			None => measured,
		};
		if let Some(timings) = self.timings {
			timings.add_stage_duration(self.stage, duration);
		}
		logservice::write_stage_done_if_verbose(&self.label, self.stage.trace_name(), duration);
	}
}

/// `startStage`: starts timing `stage` on `timings` (or only logging it, without timings).
pub fn start_stage(timings: Option<&BuildTimings>, stage: Stage) -> StageTimer<'_> {
	let label = timings.map(BuildTimings::project_label).unwrap_or_default();
	logservice::write_stage_start_if_verbose(&label, stage.trace_name());
	StageTimer {
		timings,
		stage,
		label,
		started: Instant::now(),
	}
}

/// `projectLabel`: the name of the config's directory.
pub(crate) fn project_label(config_path: &str) -> String {
	if config_path.is_empty() {
		return String::new();
	}
	gopath::base(&gopath::dir(config_path))
}

/// `logStageNamed`: verbose start and completion lines around work timed elsewhere; the returned
/// function reports the elapsed time.
pub(crate) fn log_stage_named(config_path: &str, name: &str) -> impl FnOnce() -> Duration {
	let label = project_label(config_path);
	logservice::write_stage_start_if_verbose(&label, name);
	let started = Instant::now();
	let name = name.to_owned();
	move || {
		let elapsed = started.elapsed();
		logservice::write_stage_done_if_verbose(&label, &name, elapsed);
		elapsed
	}
}

fn milliseconds(duration: Duration) -> i64 {
	i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn usize_count(count: usize) -> i64 {
	i64::try_from(count).unwrap_or(i64::MAX)
}

impl BuildTimings {
	/// `NewBuildTimings`
	#[must_use]
	pub fn new() -> Self {
		Self {
			state: Mutex::new(TimingsState {
				report: BuildTimingsReport {
					schema_version: BUILD_TIMING_SCHEMA_VERSION,
					stage_semantics: STAGE_SEMANTICS_WORK_MS.to_owned(),
					metadata: capture_runtime_metadata(),
					..BuildTimingsReport::default()
				},
				..TimingsState::default()
			}),
			started: Instant::now(),
			parent: None,
		}
	}

	/// `newProject`: a project's report under this solution report.
	#[must_use]
	pub fn new_project(self: &Arc<Self>, config_path: &str) -> Arc<Self> {
		let child = Self {
			parent: Some(Arc::clone(self)),
			..Self::new()
		};
		{
			let mut state = lock(&child.state);
			config_path.clone_into(&mut state.config_path);
			state.report.counts.scheduled_projects = 1;
			state.report.counts.selected_projects = 1;
		}
		Arc::new(child)
	}

	/// The report's current values.
	#[must_use]
	pub fn report(&self) -> BuildTimingsReport {
		lock(&self.state).report.clone()
	}

	/// The config path the report belongs to.
	#[must_use]
	pub fn config_path(&self) -> String {
		lock(&self.state).config_path.clone()
	}

	/// Sets the config path, if none is set.
	pub(crate) fn default_config_path(&self, config_path: &str) {
		let mut state = lock(&self.state);
		if state.config_path.is_empty() {
			config_path.clone_into(&mut state.config_path);
		}
	}

	fn project_label(&self) -> String {
		project_label(&lock(&self.state).config_path)
	}

	/// `addParseCacheCounts`
	pub fn add_parse_cache_counts(&self, hits: i64, misses: i64) {
		let mut state = lock(&self.state);
		state.report.counts.parse_cache_hits += hits;
		state.report.counts.parse_cache_misses += misses;
	}

	/// `setHashSkips`
	pub(crate) fn set_hash_skips(&self, skips: usize) {
		lock(&self.state).report.counts.hash_skips = usize_count(skips);
	}

	/// `addStageDuration`
	pub(crate) fn add_stage_duration(&self, stage: Stage, duration: Duration) {
		let (finished, snapshot) = {
			let mut guard = lock(&self.state);
			*guard.report.stages.field(stage) += milliseconds(duration);
			let finished = guard.finished;
			let snapshot = (finished && self.parent.is_some()).then(|| snapshot_locked(&guard));
			drop(guard);
			(finished, snapshot)
		};
		if let Some(parent) = &self.parent {
			parent.add_stage_duration(stage, duration);
			if finished && let Some(snapshot) = snapshot {
				parent.replace_project_snapshot(snapshot);
			}
		}
	}

	/// `reportNestedStage`: charges `duration` to `to` and books it to come back out of the
	/// enclosing stage `from` when that stage reports.
	pub(crate) fn report_nested_stage(&self, from: Stage, to: Stage, duration: Duration) {
		if duration.is_zero() {
			return;
		}
		*lock(&self.state).nested_stage_debt.entry(from).or_default() += duration;
		self.add_stage_duration(to, duration);
	}

	/// `settleNestedStageDebt`
	fn settle_nested_stage_debt(&self, stage: Stage, measured: Duration) -> Duration {
		let debt = lock(&self.state).nested_stage_debt.remove(&stage).unwrap_or_default();
		measured.saturating_sub(debt)
	}

	/// `setSourceCounts`
	pub(crate) fn set_source_counts(&self, total_sources: usize, selected_sources: usize) {
		let mut state = lock(&self.state);
		state.report.counts.total_sources = usize_count(total_sources);
		state.report.counts.selected_sources = usize_count(selected_sources);
	}

	/// `setEffectiveWriteWorkers`
	pub(crate) fn set_effective_write_workers(&self, workers: usize) {
		let mut state = lock(&self.state);
		state.report.counts.effective_write_workers = usize_count(workers);
		state.report.metadata.effective_write_workers = usize_count(workers);
	}

	/// `addScheduledSourceMapWrite`
	pub(crate) fn add_scheduled_source_map_write(&self) {
		lock(&self.state).report.counts.scheduled_source_map_writes += 1;
	}

	/// `addScheduledDeclarationWrites`
	pub(crate) fn add_scheduled_declaration_writes(&self, writes: usize) {
		lock(&self.state).report.counts.scheduled_declaration_writes += usize_count(writes);
	}

	/// `recordOutputWrite`
	pub(crate) fn record_output_write(&self, path: &str, wrote: bool) {
		if !wrote {
			return;
		}
		let mut state = lock(&self.state);
		state.report.counts.actual_writes += 1;
		state.prepared_directories.insert(gopath::clean(&gopath::dir(path)));
		state.report.counts.unique_prepared_directories = usize_count(state.prepared_directories.len());
	}

	/// `setEmittedEntries`
	pub(crate) fn set_emitted_entries(&self, entries: usize) {
		let mut state = lock(&self.state);
		state.report.counts.emitted_entries = usize_count(entries);
		if entries > 0 {
			state.report.counts.emitted_projects = 1;
		}
	}

	/// `markProjectTransformed`
	pub(crate) fn mark_project_transformed(&self) {
		lock(&self.state).report.counts.transformed_projects = 1;
	}

	/// `finish`: records the elapsed time; a project's report attaches to its solution's.
	pub fn finish(&self) {
		let (snapshot, prepared, round_trip, overlay) = {
			let mut state = lock(&self.state);
			if state.finished {
				return;
			}
			state.finished = true;
			let elapsed = milliseconds(self.started.elapsed());
			state.report.total_ms = elapsed;
			if self.parent.is_none() {
				return;
			}
			state.build_wall_ms = elapsed;
			if state.status.is_empty() {
				PROJECT_TIMING_STATUS_BUILT.clone_into(&mut state.status);
			}
			(
				snapshot_locked(&state),
				state.prepared_directories.clone(),
				state.sidecar_round_trip_recorded,
				state.overlay_program_recorded,
			)
		};
		if let Some(parent) = &self.parent {
			parent.attach_project(snapshot, &prepared, round_trip, overlay);
		}
	}

	/// `SetOK`
	pub fn set_ok(&self, ok: bool) {
		lock(&self.state).report.ok = ok;
	}

	/// `SetProductVersion`
	pub fn set_product_version(&self, version: &str) {
		version.clone_into(&mut lock(&self.state).report.metadata.version);
	}

	/// `setConcurrencyMetadata`
	pub fn set_concurrency_metadata(
		&self,
		builders: usize,
		requested_builders: Option<i64>,
		requested_checkers: Option<i64>,
	) {
		let mut state = lock(&self.state);
		state.report.metadata.effective_builders = usize_count(builders);
		state.report.metadata.requested_builders = requested_builders;
		state.report.metadata.requested_checkers = requested_checkers;
	}

	/// `setProjectStatus`
	#[expect(
		clippy::significant_drop_tightening,
		reason = "the project entry is written under the lock"
	)]
	pub fn set_project_status(&self, config_path: &str, status: &str, blocked_by: &str) {
		let mut state = lock(&self.state);
		if let Some(parent) = &self.parent {
			status.clone_into(&mut state.status);
			blocked_by.clone_into(&mut state.blocked_by);
			let finished = state.finished;
			let snapshot = snapshot_locked(&state);
			drop(state);
			if finished {
				parent.replace_project_snapshot(snapshot);
			}
			return;
		}
		if let Some(&index) = state.project_index.get(config_path) {
			let project = &mut state.report.projects[index];
			status.clone_into(&mut project.status);
			blocked_by.clone_into(&mut project.blocked_by);
			config_path.clone_into(&mut project.config_path);
		}
	}

	/// `initProjects`: one project entry per config path, in order.
	pub fn init_projects(&self, config_paths: &[String]) {
		let mut state = lock(&self.state);
		state.report.projects = config_paths
			.iter()
			.map(|config_path| ProjectBuildTimings {
				config_path: config_path.clone(),
				counts: BuildTimingCounts {
					scheduled_projects: 1,
					selected_projects: 1,
					..BuildTimingCounts::default()
				},
				..ProjectBuildTimings::default()
			})
			.collect();
		state.project_index = config_paths
			.iter()
			.enumerate()
			.map(|(index, config_path)| (config_path.clone(), index))
			.collect();
		state.report.counts.scheduled_projects = usize_count(config_paths.len());
		state.report.counts.selected_projects = usize_count(config_paths.len());
	}

	fn attach_project(
		&self,
		snapshot: ProjectBuildTimings,
		prepared: &HashSet<String>,
		round_trip: bool,
		overlay: bool,
	) {
		let mut state = lock(&self.state);
		let counts = snapshot.counts;
		if let Some(&index) = state.project_index.get(&snapshot.config_path) {
			state.report.projects[index] = snapshot;
		} else {
			let index = state.report.projects.len();
			state.project_index.insert(snapshot.config_path.clone(), index);
			state.report.projects.push(snapshot);
		}
		let totals = &mut state.report.counts;
		totals.total_sources += counts.total_sources;
		totals.selected_sources += counts.selected_sources;
		totals.emitted_entries += counts.emitted_entries;
		totals.transformed_projects += counts.transformed_projects;
		totals.emitted_projects += counts.emitted_projects;
		totals.scheduled_source_map_writes += counts.scheduled_source_map_writes;
		totals.scheduled_declaration_writes += counts.scheduled_declaration_writes;
		totals.actual_writes += counts.actual_writes;
		totals.hash_skips += counts.hash_skips;
		if totals.effective_write_workers == 0 {
			totals.effective_write_workers = counts.effective_write_workers;
		}
		state.prepared_directories.extend(prepared.iter().cloned());
		state.report.counts.unique_prepared_directories = usize_count(state.prepared_directories.len());
		state.sidecar_round_trip_recorded |= round_trip;
		state.overlay_program_recorded |= overlay;
	}

	fn replace_project_snapshot(&self, snapshot: ProjectBuildTimings) {
		let mut state = lock(&self.state);
		if let Some(&index) = state.project_index.get(&snapshot.config_path) {
			state.report.projects[index] = snapshot;
		}
	}
}

fn snapshot_locked(state: &TimingsState) -> ProjectBuildTimings {
	ProjectBuildTimings {
		config_path: state.config_path.clone(),
		status: state.status.clone(),
		blocked_by: state.blocked_by.clone(),
		build_wall_ms: state.build_wall_ms,
		stages: state.report.stages,
		counts: state.report.counts,
	}
}

/// `timedPersist`: wraps a persistence step in the persistence stage.
pub fn timed_persist<'a>(
	timings: Option<&'a BuildTimings>,
	persist: impl FnOnce() -> Result<(), crate::compile::Error> + 'a,
) -> impl FnOnce() -> Result<(), crate::compile::Error> + 'a {
	move || {
		let stop = start_stage(timings, Stage::Persistence);
		let result = persist();
		stop.stop();
		result
	}
}
