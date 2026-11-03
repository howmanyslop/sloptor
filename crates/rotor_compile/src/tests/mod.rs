//! Ports of `internal/compile/*_test.go`, one module per Go test file. Expected strings are the
//! Go tests' verbatim. Tests that need the solution builder, the transformer plugin sidecar or
//! native Flamework (Stage 5b and Stage 6) are ported as `#[ignore]` with the reason.
//!
//! The shared helpers are the Go tests' own (`writeProject`, `noLibGlobalStubs`, `copyDir`).

mod array_append_optimization;
mod baseurl;
mod comment_directives;
mod compile;
mod copyfilesgate;
mod declmap;
mod diagnostic_string_code;
mod env_macro;
mod exportfrom;
mod getmoduletree_folder;
mod imports;
mod incremental;
mod jsx_build;
mod labels;
mod macro_inputs;
mod macro_test;
mod modes;
mod needs_later_stages;
mod output;
mod output_guard;
mod outputwriter;
mod overlayfs;
mod parallelwrite;
mod pnpm;
mod project;
mod rbxts_options;
mod rotortypes;
mod sourcemap;
mod types_options;
mod unreachable_function;
mod validateoptions;
mod vector_math;

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::Failure;
use crate::project::Outputs;

/// `noLibGlobalStubs`: the fundamental global types the checker resolves at initialization
/// under `noLib` (`@rbxts/compiler-types` provides them in real projects).
pub const NO_LIB_GLOBAL_STUBS: &str = "declare function print(...params: Array<unknown>): void;\n\
	interface Array<T> {}\ninterface Boolean {}\ninterface CallableFunction {}\n\
	interface Function {}\ninterface IArguments {}\ninterface NewableFunction {}\n\
	interface Number {}\ninterface Object {}\ninterface RegExp {}\ninterface String {}\n";

/// The repository root.
pub fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `internal/compile/testdata/<name>`, as a string.
pub fn testdata(name: &str) -> String {
	repo_root()
		.join("internal/compile/testdata")
		.join(name)
		.to_string_lossy()
		.into_owned()
}

/// `fixtureProjectDir`: `testdata/diff/project`.
pub fn fixture_project_dir() -> String {
	repo_root().join("testdata/diff/project").to_string_lossy().into_owned()
}

/// A temporary directory spelled by its real path, as Go's `TestMain` arranges for `TMPDIR`.
pub struct Temp {
	_guard: TempDir,
	path: PathBuf,
}

impl Temp {
	pub fn new() -> Self {
		let guard = TempDir::new().expect("a temporary directory");
		let path = fs::canonicalize(guard.path()).expect("the temporary directory's real path");
		Self { _guard: guard, path }
	}

	pub fn dir(&self) -> String {
		self.path.to_string_lossy().into_owned()
	}

	pub fn join(&self, rel: &str) -> PathBuf {
		self.path.join(rel)
	}

	/// Writes `text` at `rel`, creating its parents.
	pub fn write(&self, rel: &str, text: impl AsRef<[u8]>) {
		let path = self.join(rel);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).expect("the parent directory");
		}
		fs::write(path, text).expect("the file writes");
	}
}

/// `writeProject`: a minimal compilable project; an empty `rojo_config` means no
/// `default.project.json`.
pub fn write_project(pkg_name: &str, rojo_config: &str) -> Temp {
	let temp = Temp::new();
	temp.write("tsconfig.json", WRITE_PROJECT_TSCONFIG);
	temp.write("package.json", format!(r#"{{"name":"{pkg_name}"}}"#));
	if !rojo_config.is_empty() {
		temp.write("default.project.json", rojo_config);
	}
	temp.write("src/globals.d.ts", NO_LIB_GLOBAL_STUBS);
	temp.write("src/main.ts", "export {};\n");
	temp
}

pub const WRITE_PROJECT_TSCONFIG: &str = "{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"CommonJS\",
		\"moduleResolution\": \"Node\",
		\"noLib\": true,
		\"moduleDetection\": \"force\",
		\"strict\": true,
		\"target\": \"ESNext\",
		\"types\": [],
		\"typeRoots\": [\"node_modules/@rbxts\"],
		\"rootDir\": \"src\",
		\"outDir\": \"out\"
	},
	\"include\": [\"src\"]
}";

/// `copyDir`
pub fn copy_dir(src: &Path, dst: &Path) {
	fs::create_dir_all(dst).expect("the destination directory");
	for entry in fs::read_dir(src).expect("the source directory reads") {
		let entry = entry.expect("a directory entry");
		let target = dst.join(entry.file_name());
		let file_type = entry.file_type().expect("the entry's type");
		if file_type.is_dir() {
			copy_dir(&entry.path(), &target);
		} else {
			fs::copy(entry.path(), target).expect("the file copies");
		}
	}
}

/// An output as text.
pub fn output(outputs: &Outputs, key: &str) -> String {
	outputs
		.get(key)
		.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
		.unwrap_or_default()
}

/// `keys`
pub fn keys(outputs: &Outputs) -> Vec<&str> {
	outputs.keys().map(String::as_str).collect()
}

/// A compile that must succeed with no diagnostics.
pub fn expect_compiled(result: Result<Outputs, Failure>) -> Outputs {
	match result {
		Ok(outputs) => outputs,
		Err(failure) => panic!("CompileProject: {failure} (diags: {:?})", failure.messages()),
	}
}

/// A compile that must fail; returns its diagnostics.
pub fn expect_failed(result: Result<Outputs, Failure>) -> Vec<String> {
	match result {
		Ok(outputs) => panic!("expected a hard error, got {:?}", keys(&outputs)),
		Err(failure) => failure.messages(),
	}
}

/// Runs `work` on a stack large enough for the checker.
pub fn large_stack<R: Send>(work: impl FnOnce() -> R + Send) -> R {
	rotor_tsrs::with_large_stack(work)
}

/// `setPortabilityCompilerOptions`: merges `overrides` into the config's `compilerOptions`.
pub fn set_compiler_options(config_path: &Path, overrides: &serde_json::Value) {
	let data = fs::read(config_path).expect("the config reads");
	let mut config: serde_json::Value = serde_json::from_slice(&data).expect("the config parses");
	let options = config
		.get_mut("compilerOptions")
		.and_then(serde_json::Value::as_object_mut)
		.expect("fixture has no compilerOptions object");
	if let Some(overrides) = overrides.as_object() {
		for (key, value) in overrides {
			options.insert(key.clone(), value.clone());
		}
	}
	fs::write(config_path, serde_json::to_vec(&config).expect("the config encodes")).expect("the config writes");
}
