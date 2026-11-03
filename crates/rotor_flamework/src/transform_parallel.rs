//! `transform_parallel.go`: the per-file loop across checkers, and the recorded build-info
//! writes that keep its output identical to the serial loop's.
//!
//! As in Go, the files without a planned class go to one worker per checker of the program's
//! pool, all of them (`Program::for_each_checker_parallel_holding` runs checker 0's worker on the
//! caller's thread with the caller's checker, which `get_type_checker` locked and Go shares
//! unlocked); the planned files then run on the caller's state. Every worker prints its files,
//! and its build-info writes are recorded and replayed in input-file order.
//!
//! Which worker takes which file depends on thread timing, here as in Go. A file's output can
//! depend on its checker: the order of a union's members, which guards emit as they are, follows
//! type creation order, which differs between checkers. Go's output has the same freedom.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::checker::Checker;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::printer::new_emit_context;

use crate::build_info::BuildInfo;
use crate::id::new_uuid_v4;
use crate::plan::FilePlan;
use crate::project::{IdMode, Project};
use crate::state::DeclarationIdentity;
use crate::transform::transform_source_file;
use crate::transform_addpaths::AddPathsSourceIndex;
use crate::transform_diagnostics::Diagnostic;
use crate::transform_expressions::default_flamework_macro_runtime;
use crate::transform_state::{PrintFn, SourceMetadata, TransformInput, TransformState, print_source};
use crate::{Error, generate_identifier};

/// `parallelTransformMinFiles`: smaller projects stay on the serial loop.
const PARALLEL_TRANSFORM_MIN_FILES: usize = 256;

/// `parallelTransformEnv`: `0` forces the serial loop; `1` takes the parallel loop whenever it
/// is otherwise eligible.
const PARALLEL_TRANSFORM_ENV: &str = "ROTOR_FLAMEWORK_PARALLEL";

/// `recordedOperation`: one build-info write a file's transform asked for.
#[derive(Clone, Debug)]
pub(crate) enum RecordedOperation {
	Identifier { key: String, value: String },
	StringHash { key: String, value: String },
	Glob { pattern: String, origin: String },
}

/// `transformRecording`: a worker's half of the build info. Reads see the build info as it
/// stood before the loop plus this worker's requests; writes are recorded for replay in
/// input-file order.
#[derive(Debug, Default)]
pub(crate) struct TransformRecording {
	pub(crate) hashes: Arc<Mutex<HashMap<String, String>>>,
	pub(crate) identifiers: HashMap<String, String>,
	pub(crate) operations: Vec<RecordedOperation>,
}

impl TransformState<'_> {
	/// `Project.Identifier`, recorded for a parallel worker.
	pub(crate) fn identifier(&mut self, declaration: &DeclarationIdentity) -> Result<String, Error> {
		let Some(recording) = &mut self.recording else {
			return self.project.identifier(declaration);
		};
		let project = &*self.project;
		if let Some(existing) = project.build_info.identifier(&declaration.internal_id) {
			return Ok(existing);
		}
		if let Some(existing) = recording.identifiers.get(&declaration.internal_id) {
			return Ok(existing.clone());
		}
		let id = generate_identifier(&project.identifier_request(declaration, project.config.salt.clone())).map_err(
			|error| {
				error.wrap(&format!(
					"generate identifier {}",
					rotor_support::gostr::quote(&declaration.internal_id)
				))
			},
		)?;
		if declaration.is_package {
			return Ok(id);
		}
		recording
			.identifiers
			.insert(declaration.internal_id.clone(), id.clone());
		recording.operations.push(RecordedOperation::Identifier {
			key: declaration.internal_id.clone(),
			value: id.clone(),
		});
		Ok(id)
	}

	/// `Project.HashString`, recorded for a parallel worker.
	pub(crate) fn hash_string(&mut self, text: &str, context: &str) -> Result<String, Error> {
		let context = if context.is_empty() {
			crate::id::DEFAULT_HASH_CONTEXT
		} else {
			context
		};
		let Some(recording) = &mut self.recording else {
			return self.project.hash_string(text, context);
		};
		let key = format!("{context}:{text}");
		if let Some(value) = self.project.build_info.string_hash(&key) {
			return Ok(value.to_owned());
		}
		let value = {
			let mut hashes = recording
				.hashes
				.lock()
				.unwrap_or_else(std::sync::PoisonError::into_inner);
			if let Some(value) = hashes.get(&key) {
				value.clone()
			} else {
				let value = new_uuid_v4().map_err(|error| error.wrap("flamework: hash string"))?;
				hashes.insert(key.clone(), value.clone());
				value
			}
		};
		recording.operations.push(RecordedOperation::StringHash {
			key,
			value: value.clone(),
		});
		Ok(value)
	}

	/// `Project.AddGlob`, recorded for a parallel worker.
	pub(crate) fn add_glob(&mut self, pattern: &str, origin: &str) {
		match &mut self.recording {
			Some(recording) => recording.operations.push(RecordedOperation::Glob {
				pattern: pattern.to_owned(),
				origin: origin.to_owned(),
			}),
			None => self.project.add_glob(pattern, origin),
		}
	}
}

/// `replayRecordedOperations`: one file's recorded writes, skipping what an earlier file wrote.
pub(crate) fn replay_recorded_operations(
	build_info: &mut BuildInfo,
	operations: &[RecordedOperation],
) -> Result<(), Error> {
	for operation in operations {
		match operation {
			RecordedOperation::Identifier { key, value } => {
				if build_info.identifier(key).is_none() {
					build_info.add_identifier(key, value)?;
				}
			}
			RecordedOperation::StringHash { key, value } => {
				if build_info.string_hash(key).is_none() {
					build_info.set_string_hash(key, value);
				}
			}
			RecordedOperation::Glob { pattern, origin } => build_info.add_glob(pattern, origin),
		}
	}
	Ok(())
}

#[cfg(test)]
thread_local! {
	/// Test seam for `ROTOR_FLAMEWORK_PARALLEL` on this thread: `Some(true)` is `1`, `Some(false)`
	/// is `0`.
	pub(crate) static PARALLEL_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
	/// Test probe: set when the parallel loop produced the result.
	pub(crate) static PARALLEL_RAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `parallelTransformEligible`: whether the per-file loop may run across the program's
/// checkers. Only full ID mode names identifiers without a running counter, and a caller's
/// macro runtime may keep state.
pub(crate) fn parallel_transform_eligible(input: &TransformInput<'_>) -> bool {
	#[cfg(test)]
	let forced = PARALLEL_OVERRIDE.get();
	#[cfg(not(test))]
	let forced: Option<bool> = None;
	let setting = match forced {
		Some(true) => Ok("1".to_owned()),
		Some(false) => Ok("0".to_owned()),
		None => std::env::var(PARALLEL_TRANSFORM_ENV),
	};
	match setting.as_deref() {
		Ok("0") => return false,
		Ok("1") => {}
		_ => {
			if input.files.len() < PARALLEL_TRANSFORM_MIN_FILES {
				return false;
			}
		}
	}
	input.macro_runtime.is_none() && input.project.id_mode == IdMode::Full && !input.program.single_threaded()
}

/// The worker a file ran on: a checker's group, or the caller's state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileWorker {
	Group(usize),
	Serial,
}

/// `parallelFileResult`
struct ParallelFileResult {
	transformed: P<SourceFile>,
	metadata: SourceMetadata,
	worker: FileWorker,
	operations: Vec<RecordedOperation>,
}

/// What every worker reads: the analysis results, shared and read-only, and the file queue and
/// result slots the workers share.
struct WorkerShared<'s> {
	program: &'static Program,
	project: &'s Project,
	plans: &'s [FilePlan],
	expression_surfaces: &'s Option<HashSet<String>>,
	add_paths_sources: Option<Arc<AddPathsSourceIndex>>,
	hashes: &'s Arc<Mutex<HashMap<String, String>>>,
	package_declarations: &'s crate::transform_symbols::PackageDeclarationCache,
	print: Option<&'s PrintFn<'s>>,
	files: &'s [P<SourceFile>],
	queue: &'s [usize],
	next: AtomicUsize,
	aborted: AtomicBool,
	results: &'s [Mutex<Option<ParallelFileResult>>],
	group_diagnostics: Mutex<BTreeMap<usize, Vec<Diagnostic>>>,
}

/// `newWorkerState`: a state for one worker; everything a file's visit writes is its own.
fn new_worker_state<'a>(
	shared: &WorkerShared<'_>,
	checker: &'a mut Checker,
	project: &'a mut Project,
) -> TransformState<'a> {
	let emit_context = new_emit_context();
	let factory = emit_context.factory.as_node_factory().clone();
	TransformState {
		program: shared.program,
		checker,
		project,
		factory,
		emit_context,
		// The parallel loop runs only without a caller's runtime, so every worker has the default one.
		macro_runtime: default_flamework_macro_runtime(),
		plans: shared.plans.to_vec(),
		plan_index: std::cell::OnceCell::new(),
		diagnostics: Vec::new(),
		generated_names: HashMap::new(),
		guard_library: String::new(),
		surfaces: HashMap::new(),
		expression_surfaces: HashMap::new(),
		flamework_expression_surfaces: shared.expression_surfaces.clone(),
		add_paths_sources: shared.add_paths_sources.clone(),
		recording: Some(TransformRecording {
			hashes: Arc::clone(shared.hashes),
			..TransformRecording::default()
		}),
		package_declarations: Arc::clone(shared.package_declarations),
	}
}

/// What the caller's state keeps while it runs the planned files as Go's serial worker.
struct SavedState {
	diagnostics: Vec<Diagnostic>,
	generated_names: HashMap<String, HashMap<String, usize>>,
	guard_library: String,
	surfaces: HashMap<P<SourceFile>, bool>,
	expression_surfaces: HashMap<P<SourceFile>, bool>,
	recording: Option<TransformRecording>,
}

impl SavedState {
	/// Turns `state` into the serial worker: fresh per-file state, writes recorded.
	fn enter(state: &mut TransformState<'_>, hashes: &Arc<Mutex<HashMap<String, String>>>) -> Self {
		Self {
			diagnostics: std::mem::take(&mut state.diagnostics),
			generated_names: std::mem::take(&mut state.generated_names),
			guard_library: std::mem::take(&mut state.guard_library),
			surfaces: std::mem::take(&mut state.surfaces),
			expression_surfaces: std::mem::take(&mut state.expression_surfaces),
			recording: state.recording.replace(TransformRecording {
				hashes: Arc::clone(hashes),
				..TransformRecording::default()
			}),
		}
	}

	/// Restores `state`, returning the serial worker's diagnostics.
	fn restore(self, state: &mut TransformState<'_>) -> Vec<Diagnostic> {
		let diagnostics = std::mem::replace(&mut state.diagnostics, self.diagnostics);
		state.generated_names = self.generated_names;
		state.guard_library = self.guard_library;
		state.surfaces = self.surfaces;
		state.expression_surfaces = self.expression_surfaces;
		state.recording = self.recording;
		diagnostics
	}
}

/// One file transformed and printed on a worker's state; `None` when either failed.
fn transform_and_print(
	state: &mut TransformState<'_>,
	print: Option<&PrintFn<'_>>,
	source_file: P<SourceFile>,
	runtime: &crate::transform_macro_types::MacroRuntime,
	worker: FileWorker,
) -> Option<ParallelFileResult> {
	if let Some(recording) = &mut state.recording {
		recording.operations.clear();
	}
	let transformed = transform_source_file(state, source_file, runtime).ok()?;
	let metadata = print_source(print, SourceMetadata::new(source_file, transformed, state.emit_context)).ok()?;
	let operations = state
		.recording
		.as_mut()
		.map(|recording| std::mem::take(&mut recording.operations))
		.unwrap_or_default();
	Some(ParallelFileResult {
		transformed,
		metadata,
		worker,
		operations,
	})
}

/// `transformFilesParallel`: the files without a planned class spread over the program's
/// checkers, then the planned files on the caller's state (a class transform re-parents the
/// shared original tree, which no other checker may walk meanwhile). `None` when anything
/// failed; the caller then runs the serial loop from scratch, which reports the same error, and
/// nothing recorded here was applied.
pub(crate) fn transform_files_parallel(
	state: &mut TransformState<'_>,
	files: &[P<SourceFile>],
	print: Option<&PrintFn<'_>>,
) -> Option<(Vec<P<SourceFile>>, Vec<SourceMetadata>)> {
	let mut unplanned = Vec::new();
	let mut planned = Vec::new();
	for (index, &source_file) in files.iter().enumerate() {
		match state.plan_for_file(source_file) {
			Err(_) => return None,
			Ok(Some(_)) => planned.push(index),
			Ok(None) => unplanned.push(index),
		}
	}

	let hashes = Arc::new(Mutex::new(HashMap::new()));
	let program = state.program;
	if program.checker_pool_size() < 2 {
		return None;
	}
	// Workers pull files from one queue, largest first, so a few heavy files cannot leave one
	// checker running long after the rest. A stable sort, as Go's `slices.SortStableFunc`.
	let mut queue = unplanned;
	queue.sort_by(|&left, &right| files[right].text().len().cmp(&files[left].text().len()));
	let results: Vec<Mutex<Option<ParallelFileResult>>> = files.iter().map(|_| Mutex::new(None)).collect();
	let shared = WorkerShared {
		program,
		project: state.project,
		plans: &state.plans,
		expression_surfaces: &state.flamework_expression_surfaces,
		add_paths_sources: state.add_paths_sources.clone(),
		hashes: &hashes,
		package_declarations: &state.package_declarations,
		print,
		files,
		queue: &queue,
		next: AtomicUsize::new(0),
		aborted: AtomicBool::new(false),
		results: &results,
		group_diagnostics: Mutex::new(BTreeMap::new()),
	};
	program.for_each_checker_parallel_holding(state.checker, |group, checker| run_worker(&shared, group, checker));
	if shared.aborted.load(Ordering::SeqCst) {
		return None;
	}
	let mut group_diagnostics = shared
		.group_diagnostics
		.into_inner()
		.unwrap_or_else(std::sync::PoisonError::into_inner);

	let saved = SavedState::enter(state, &hashes);
	let runtime = state.macro_runtime();
	let mut serial_failed = false;
	for &index in &planned {
		let Some(result) = transform_and_print(state, print, files[index], &runtime, FileWorker::Serial) else {
			serial_failed = true;
			break;
		};
		*lock(&results[index]) = Some(result);
	}
	let serial_diagnostics = saved.restore(state);
	if serial_failed {
		return None;
	}

	// Every checker ran its worker until the queue was empty, so every file has a result.
	let results: Vec<ParallelFileResult> = results
		.into_iter()
		.map(|slot| slot.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner))
		.collect::<Option<_>>()?;
	for result in &results {
		replay_recorded_operations(state.project.build_info_mut(), &result.operations).ok()?;
	}
	#[cfg(test)]
	PARALLEL_RAN.set(true);
	let mut serial_diagnostics = Some(serial_diagnostics);
	let mut seen = Vec::new();
	let mut transformed_files = Vec::with_capacity(files.len());
	let mut sources = Vec::with_capacity(files.len());
	for result in results {
		if !seen.contains(&result.worker) {
			seen.push(result.worker);
			let diagnostics = match result.worker {
				FileWorker::Group(group) => group_diagnostics.remove(&group).unwrap_or_default(),
				FileWorker::Serial => serial_diagnostics.take().unwrap_or_default(),
			};
			state.diagnostics.extend(diagnostics);
		}
		transformed_files.push(result.transformed);
		sources.push(result.metadata);
	}
	Some((transformed_files, sources))
}

/// One worker's share of the loop: files from the queue until it is empty or a file failed.
fn run_worker(shared: &WorkerShared<'_>, group: usize, checker: &mut Checker) {
	let mut project = shared.project.clone();
	let mut worker = new_worker_state(shared, checker, &mut project);
	let runtime = worker.macro_runtime();
	loop {
		let position = shared.next.fetch_add(1, Ordering::SeqCst);
		if position >= shared.queue.len() || shared.aborted.load(Ordering::SeqCst) {
			break;
		}
		let index = shared.queue[position];
		let Some(result) = transform_and_print(
			&mut worker,
			shared.print,
			shared.files[index],
			&runtime,
			FileWorker::Group(group),
		) else {
			shared.aborted.store(true, Ordering::SeqCst);
			break;
		};
		*lock(&shared.results[index]) = Some(result);
	}
	lock(&shared.group_diagnostics).insert(group, std::mem::take(&mut worker.diagnostics));
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
