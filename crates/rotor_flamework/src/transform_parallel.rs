//! `transform_parallel.go`: the per-file loop across checkers, and the recorded build-info
//! writes that keep its output identical to the serial loop's.
//!
//! The recording ([`TransformRecording`]) and its in-order replay are ported. Running workers on
//! the program's checkers is not yet: tsrs's `for_each_checker_parallel` needs a `Sync` closure,
//! and [`TransformState`] holds the project mutably. [`transform_files_parallel`] therefore
//! declines, and [`crate::transform::transform`] runs the serial loop, which Go's parallel loop
//! reproduces byte for byte.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::build_info::BuildInfo;
use crate::id::new_uuid_v4;
use crate::project::IdMode;
use crate::state::DeclarationIdentity;
use crate::transform_state::{TransformInput, TransformState};
use crate::{Error, generate_identifier};

/// `parallelTransformMinFiles`: smaller projects stay on the serial loop.
const PARALLEL_TRANSFORM_MIN_FILES: usize = 256;

/// `parallelTransformEnv`: `0` forces the serial loop; `1` takes the parallel loop whenever it
/// is otherwise eligible.
const PARALLEL_TRANSFORM_ENV: &str = "ROTOR_FLAMEWORK_PARALLEL";

/// `recordedOperation`: one build-info write a file's transform asked for.
#[derive(Clone, Debug)]
#[expect(
	dead_code,
	reason = "read by the in-order replay, which the parallel loop will drive"
)]
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

#[expect(dead_code, reason = "the parallel loop will drive the replay")]
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

/// `parallelTransformEligible`: whether the per-file loop may run across the program's
/// checkers. Only full ID mode names identifiers without a running counter, and a caller's
/// macro runtime may keep state.
pub(crate) fn parallel_transform_eligible(input: &TransformInput<'_>) -> bool {
	match std::env::var(PARALLEL_TRANSFORM_ENV).as_deref() {
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

/// `transformFilesParallel`: declines (see the module documentation), so the caller runs the
/// serial loop.
pub(crate) const fn transform_files_parallel() -> bool {
	false
}
