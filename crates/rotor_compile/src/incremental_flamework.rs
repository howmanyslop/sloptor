//! `incremental_flamework.go`: native Flamework's incremental inputs are Stage 6. Without a
//! Flamework pipeline there are none, which is Go's behavior with Flamework off.

use crate::flamework_pipeline::FlameworkPipeline;

/// `FlameworkIncrementalInputs`: the files and globs native Flamework read (Stage 6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlameworkIncrementalInputs;

/// `flameworkIncrementalInputs`: `None` without a pipeline.
pub(crate) const fn flamework_incremental_inputs(
	_pipeline: Option<&FlameworkPipeline>,
) -> Option<FlameworkIncrementalInputs> {
	None
}

/// `flameworkIncrementalSalt`: empty without inputs.
pub(crate) const fn flamework_incremental_salt(_inputs: Option<&FlameworkIncrementalInputs>) -> String {
	String::new()
}
