//! The differential gate's compile: `rotor_compile`'s single-project pipeline, the port of the
//! `compile.CompileProject` Go's `internal/diff` runs.

use std::path::{Path, PathBuf};

use rotor_compile::project::Outputs;

/// The repository root.
pub fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `compile.CompileProject(projectDir)`: every file of the project, one program, checker
/// groups and `MultiState`s as in Go.
///
/// # Panics
///
/// When the project fails to compile or reports diagnostics (Go fails the gate).
#[expect(clippy::panic, reason = "a test harness fails loudly")]
pub fn compile_project(project_dir: &Path) -> Outputs {
	let project_dir = project_dir.to_string_lossy().into_owned();
	match rotor_compile::compile_project(&project_dir) {
		Ok(outputs) => outputs,
		Err(failure) => panic!(
			"CompileProject error: {} (diagnostics: {:?})",
			failure.error,
			failure.messages()
		),
	}
}
