//! Fixtures and a runner shared by the CLI's integration tests.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The global declarations the checker needs under `noLib`, so a fixture needs no
/// `node_modules` (`noLibGlobalStubs` in `cmd/rotor/build_test.go`).
pub const NO_LIB_GLOBAL_STUBS: &str = "declare function print(...params: Array<unknown>): void;\n\
	interface Array<T> {}\ninterface Boolean {}\ninterface CallableFunction {}\n\
	interface Function {}\ninterface IArguments {}\ninterface NewableFunction {}\n\
	interface Number {}\ninterface Object {}\ninterface RegExp {}\ninterface String {}\n";

/// What one run of a binary produced.
#[derive(Debug)]
pub struct Run {
	pub code: i32,
	pub stdout: String,
	pub stderr: String,
}

/// The binary under test.
pub fn rust_binary() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_sloptor"))
}

/// Runs `binary` in `dir` with a plain environment (no forced colour).
pub fn run_binary(binary: &Path, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
	let mut command = Command::new(binary);
	command
		.args(args)
		.current_dir(dir)
		.env_remove("NO_COLOR")
		.env_remove("FORCE_COLOR")
		.env_remove("CLICOLOR")
		.env_remove("CLICOLOR_FORCE")
		.env("ROTOR_MAX_BUILDS", "0");
	for (name, value) in env {
		command.env(name, value);
	}
	let output = command.output().expect("run the binary");
	Run {
		code: output.status.code().unwrap_or(-1),
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
	}
}

/// Runs `binary` in `dir` with `input` on stdin.
pub fn run_with_stdin(binary: &Path, dir: &Path, args: &[&str], input: &str) -> Run {
	use std::io::Write as _;
	let mut child = Command::new(binary)
		.args(args)
		.current_dir(dir)
		.env_remove("NO_COLOR")
		.env_remove("FORCE_COLOR")
		.env_remove("CLICOLOR")
		.env_remove("CLICOLOR_FORCE")
		.env("ROTOR_MAX_BUILDS", "0")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.expect("spawn the binary");
	child
		.stdin
		.take()
		.expect("stdin")
		.write_all(input.as_bytes())
		.expect("write stdin");
	let output = child.wait_with_output().expect("wait for the binary");
	Run {
		code: output.status.code().unwrap_or(-1),
		stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
		stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
	}
}

/// `writeDiagnosticsProject`: a package project with one `src/` file per entry.
pub fn diagnostics_project(root: &Path, files: &[(&str, &str)]) -> PathBuf {
	buildable_project(root, "");
	std::fs::remove_file(root.join("src/main.ts")).expect("remove main.ts");
	for (name, text) in files {
		write(&root.join("src").join(name), text);
	}
	root.to_path_buf()
}

/// Runs the Rust CLI in `dir`.
pub fn run(dir: &Path, args: &[&str]) -> Run {
	run_binary(&rust_binary(), dir, args, &[])
}

/// A temporary directory, by its real path (macOS temp dirs sit behind a `/var` symlink, and
/// rotor reports real paths).
pub struct Temp {
	_dir: tempfile::TempDir,
	pub path: PathBuf,
}

pub fn temp() -> Temp {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = rotor_support::fsutil::real_path(dir.path()).expect("canonical temp dir");
	Temp { _dir: dir, path }
}

pub fn write(path: &Path, content: &str) {
	std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
	std::fs::write(path, content).expect("write");
}

/// `writeBuildableProject`: a self-contained package project (a scoped name needs no Rojo
/// config). `main` replaces `src/main.ts` when not empty.
pub fn buildable_project(root: &Path, main: &str) -> PathBuf {
	write(
		&root.join("tsconfig.json"),
		r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}"#,
	);
	write(&root.join("package.json"), r#"{"name":"@scope/build-json-fixture"}"#);
	write(&root.join("src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
	write(
		&root.join("src/main.ts"),
		if main.is_empty() { "export {};\n" } else { main },
	);
	root.to_path_buf()
}

/// `writeCheckableProject`: a project `check` can typecheck without `node_modules`.
pub fn checkable_project(root: &Path, main: &str, compiler_options: &str) -> PathBuf {
	write(
		&root.join("tsconfig.json"),
		&format!(
			r#"{{
	"compilerOptions": {{
		"noLib": true,
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"{compiler_options}
	}},
	"include": ["src"]
}}"#
		),
	);
	write(&root.join("src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
	write(
		&root.join("src/main.ts"),
		if main.is_empty() { "export {};\n" } else { main },
	);
	root.to_path_buf()
}

/// A solution whose root has no files of its own and one referenced package
/// (`TestCmdBuildJSONReferenceOnlySolution`).
pub fn reference_only_solution(root: &Path) -> PathBuf {
	write(
		&root.join("tsconfig.json"),
		r#"{"files":[],"include":[],"references":[{"path":"./child"}]}"#,
	);
	let child = root.join("child");
	write(
		&child.join("tsconfig.json"),
		r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"include":["src"]}"#,
	);
	write(&child.join("package.json"), r#"{"name":"@scope/solution-child"}"#);
	write(&child.join("src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
	write(&child.join("src/main.ts"), "export const value = 1;\n");
	root.to_path_buf()
}

/// `p` as a string argument.
pub fn arg(path: &Path) -> String {
	path.to_string_lossy().into_owned()
}
