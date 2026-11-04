//! Byte parity of the machine-readable output with the Go CLI.
//!
//! Needs a Go build of `cmd/rotor` (`go build -trimpath -o <path> ./cmd/rotor`) named by
//! `ROTOR_GO_BIN`, so it is ignored by default:
//!
//! ```sh
//! ROTOR_GO_BIN=<path> mise x -- cargo nextest run -p rotor_cli --run-ignored only -E "binary(go_parity)"
//! ```

#![cfg(test)]

mod support;

use std::path::Path;

use clap as _;
use clap_complete as _;
use mimalloc_safe as _;
use rotor_compile as _;
use rotor_config as _;
use rotor_support as _;
use rotor_tsrs as _;
use serde_json as _;
use support::{
	Run, arg, buildable_project, checkable_project, reference_only_solution, run, run_binary, rust_binary, temp, write,
};

/// `"durationMs":<n>` becomes `"durationMs":0`: the one field that differs between any two runs.
fn normalize(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	let mut rest = text;
	while let Some(at) = rest.find("\"durationMs\":") {
		let (head, tail) = rest.split_at(at + "\"durationMs\":".len());
		out.push_str(head);
		out.push('0');
		rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
	}
	out.push_str(rest);
	out
}

/// Every fixture twice, once per binary, so neither sees the other's outputs or caches.
fn fixtures(root: &Path) {
	buildable_project(&root.join("clean"), "");
	buildable_project(&root.join("type-error"), "export const s: string = 5;\n");
	buildable_project(
		&root.join("transformer-error"),
		"declare const loose: any;\nexport const taken = loose.field;\n",
	);
	let two = buildable_project(&root.join("two-files"), "export const probeA: number = \"a\";\n");
	write(&two.join("src/other.ts"), "export const probeB: number = \"b\";\n");
	buildable_project(&root.join("wide"), "const é = \"😀\";\nexport const s: string = 5;\n");
	buildable_project(
		&root.join("missing-import"),
		"import { x } from \"./nope\";\nexport const y = x;\n",
	);
	checkable_project(
		&root.join("check-only"),
		"export const n: number = \"one\";\nlet unused: string = 2;\n",
		"",
	);
	reference_only_solution(&root.join("solution"));
}

fn compare(go: &Run, rust: &Run, args: &[&str]) {
	assert_eq!(
		rust.code, go.code,
		"exit code of {args:?}\ngo stderr: {}\nrust stderr: {}",
		go.stderr, rust.stderr
	);
	assert_eq!(normalize(&rust.stdout), normalize(&go.stdout), "stdout of {args:?}");
}

#[test]
#[ignore = "needs ROTOR_GO_BIN, a Go build of cmd/rotor"]
fn json_output_matches_go_byte_for_byte() {
	let go_binary = std::env::var_os("ROTOR_GO_BIN").expect("set ROTOR_GO_BIN to a Go build of cmd/rotor");
	let go_binary = Path::new(&go_binary);
	let go_root = temp();
	let rust_root = temp();
	fixtures(&go_root.path);
	fixtures(&rust_root.path);
	let mut cases: Vec<Vec<String>> = Vec::new();
	for project in [
		"clean",
		"type-error",
		"transformer-error",
		"two-files",
		"wide",
		"missing-import",
		"check-only",
	] {
		cases.push(vec!["build".into(), "--json".into(), project.into()]);
		cases.push(vec!["check".into(), "--json".into(), project.into()]);
		cases.push(vec!["--build".into(), project.into(), "--json".into()]);
	}
	cases.push(vec![
		"build".into(),
		"--json".into(),
		"--no-luau".into(),
		"--optimizedLoops=false".into(),
		"clean".into(),
	]);
	cases.push(vec![
		"build".into(),
		"--json".into(),
		"--build".into(),
		"--project".into(),
		"solution/tsconfig.json".into(),
		"--builders".into(),
		"1".into(),
	]);
	cases.push(vec![
		"build".into(),
		"--json".into(),
		"missing-dir-without-config/x".into(),
	]);
	cases.push(vec!["schema".into()]);
	cases.push(vec!["schema".into(), "--rbxts".into()]);
	cases.push(vec!["version".into()]);
	cases.push(vec!["--version".into()]);
	cases.push(vec!["build".into(), "-v".into()]);
	for case in &cases {
		let args: Vec<&str> = case.iter().map(String::as_str).collect();
		let go = run_binary(go_binary, &go_root.path, &args, &[]);
		let rust = run_binary(&rust_binary(), &rust_root.path, &args, &[]);
		compare(&go, &rust, &args);
	}
}

#[test]
#[ignore = "needs ROTOR_GO_BIN, a Go build of cmd/rotor"]
fn exit_codes_match_go() {
	let go_binary = std::env::var_os("ROTOR_GO_BIN").expect("set ROTOR_GO_BIN to a Go build of cmd/rotor");
	let go_binary = Path::new(&go_binary);
	let dir = temp();
	let project = buildable_project(&dir.path.join("p"), "");
	let project = arg(&project);
	let cases: Vec<Vec<&str>> = vec![
		vec![],
		vec!["frobnicate"],
		vec!["--help"],
		vec!["build", "--help"],
		vec!["build", "--bogus"],
		vec!["build", "--usePolling"],
		vec!["build", "--emitDeclarationOnly"],
		vec!["build", "--builders", "2"],
		vec!["build", "--checkers=0"],
		vec!["build", "--checkers", "-1"],
		vec!["build", "--max-errors=-1"],
		vec!["build", "--type", "bogus"],
		vec!["build", "--luau=maybe"],
		vec!["build", "a", "-p", "b"],
		vec!["build", "--timings="],
		vec!["build", "--watch", "--timings", "t.json"],
		vec!["check", "--builders", "2"],
		vec!["check", "missing"],
		vec!["clean", "--bogus"],
		vec!["schema", "extra"],
		vec!["completion", "tcsh"],
		vec!["completion", "bash"],
		vec!["build", "--no-luau", &project],
		vec!["build", "--writeOnlyChanged", "--logTruthyChanges=true", &project],
		vec!["clean", "--dry-run", &project],
	];
	for args in &cases {
		let go = run_binary(go_binary, &dir.path, args, &[]);
		let rust = run(&dir.path, args);
		assert_eq!(
			rust.code, go.code,
			"exit code of {args:?}\ngo: {}\nrust: {}",
			go.stderr, rust.stderr
		);
	}
}
