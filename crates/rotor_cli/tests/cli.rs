//! End-to-end tests of the `sloptor` binary: exit codes, the flag surface, `--json`, `check`,
//! `clean`, `schema` and completions (the behavioural half of `cmd/rotor/*_test.go`).

#![cfg(test)]

mod support;

use clap as _;
use clap_complete as _;
use ctrlc as _;
use icu_normalizer as _;
use jiff as _;
#[cfg(target_os = "macos")]
use libc as _;
use libmimalloc_sys2 as _;
use mimalloc_safe as _;
use notify as _;
use rotor_compile as _;
use rotor_lsp as _;
use rotor_tsrs as _;
use serde_json::Value;
use sha2 as _;
use support::{
	arg, buildable_project, checkable_project, diagnostics_project, reference_only_solution, run, run_binary,
	run_with_stdin, rust_binary, temp, write,
};
use terminal_size as _;

const VERSION: &str = rotor_support::version::VERSION;

fn json(text: &str) -> Value {
	serde_json::from_str(text.trim()).unwrap_or_else(|error| panic!("not one JSON object ({error}):\n{text}"))
}

#[test]
fn version_surfaces_print_the_bare_version() {
	let dir = temp();
	for args in [
		&["--version"][..],
		&["-v"],
		&["version"],
		&["build", "-v"],
		&["build", "--version"],
	] {
		let got = run(&dir.path, args);
		assert_eq!(
			(got.code, got.stdout.as_str(), got.stderr.as_str()),
			(0, format!("{VERSION}\n").as_str(), ""),
			"{args:?}"
		);
	}
	assert_eq!(run(&dir.path, &["--version"]).stdout.trim(), VERSION);
}

#[test]
fn help_exits_zero_and_failures_exit_one() {
	let dir = temp();
	for args in [
		&["--help"][..],
		&["-h"],
		&["build", "-h"],
		&["build", "--help"],
		&["check", "--help"],
		&["schema", "--help"],
	] {
		let got = run(&dir.path, args);
		assert_eq!(got.code, 0, "{args:?}: {}", got.stderr);
		assert!(got.stdout.contains("Usage: sloptor"), "{args:?}:\n{}", got.stdout);
		assert!(got.stderr.is_empty(), "{args:?}: {}", got.stderr);
	}
	// A bare `sloptor` prints the help on stderr and exits 1, as Go's "no command given" does.
	let got = run(&dir.path, &[]);
	assert_eq!(got.code, 1);
	assert!(got.stderr.contains("Usage: sloptor"), "{}", got.stderr);
	assert!(got.stdout.is_empty(), "wrote to stdout: {}", got.stdout);
	for args in [
		&["frobnicate"][..],
		&["build", "--bogus"],
		&["check", "--bogus"],
		&["clean", "--bogus"],
		&["schema", "--bogus"],
		&["build", "--use-polling"],
		&["build", "--checkers=0"],
		&["completion", "tcsh"],
	] {
		let got = run(&dir.path, args);
		assert_eq!(got.code, 1, "{args:?}");
		assert!(got.stderr.starts_with("error: "), "{args:?}: {}", got.stderr);
		assert!(got.stdout.is_empty(), "{args:?} wrote to stdout: {}", got.stdout);
	}
	// A runtime failure: no tsconfig anywhere up from an empty directory is not guaranteed, so
	// point at a file that is not a tsconfig directory instead.
	let got = run(&dir.path, &["check", "missing"]);
	assert_eq!(got.code, 1);
	assert!(got.stderr.starts_with("error: "), "{}", got.stderr);
}

#[test]
fn removed_commands_and_flags_are_unknown() {
	let dir = temp();
	for name in [
		"bundle",
		"minify",
		"pack",
		"dev",
		"sourcemap",
		"asset",
		"deploy",
		"init",
		"add",
		"doctor",
		"migrate",
	] {
		let got = run(&dir.path, &[name]);
		assert_eq!(got.code, 1, "{name}");
		assert_eq!(got.stdout, "");
		assert!(got.stderr.contains("unrecognized subcommand"), "{name}: {}", got.stderr);
	}
	let got = run(&dir.path, &["build", "--minify"]);
	assert_eq!(got.code, 1);
	assert!(got.stderr.contains("unexpected argument '--minify'"), "{}", got.stderr);
	assert!(
		got.stderr.contains("Usage: sloptor build [OPTIONS] [PATH]"),
		"{}",
		got.stderr
	);
}

#[test]
fn camel_case_and_misspelled_flags_suggest_the_kebab_name() {
	let dir = temp();
	for (typo, want) in [
		("--optimizedLop", "--optimized-loops"),
		("--maxErrors", "--max-errors"),
		("--singlethreaded", "--single-threaded"),
	] {
		let got = run(&dir.path, &["build", typo]);
		assert_eq!(got.code, 1);
		assert!(
			got.stderr
				.contains(&format!("tip: a similar argument exists: '{want}'")),
			"{typo}: {}",
			got.stderr
		);
		assert!(!got.stderr.contains("[=<BOOL>]"), "{}", got.stderr);
	}
}

#[test]
fn a_leading_build_flag_is_the_build_command() {
	let dir = temp();
	let config = arg(&dir.path.join("tsconfig.lib.json"));
	for flag in ["--build", "-b"] {
		let want = run(&dir.path, &["build", flag, &config]);
		let got = run(&dir.path, &[flag, &config]);
		assert_eq!(
			(got.code, &got.stdout, &got.stderr),
			(want.code, &want.stdout, &want.stderr),
			"{flag}"
		);
	}
	let want = run(&dir.path, &["build", "--build", "--help"]);
	let got = run(&dir.path, &["--build", "--help"]);
	assert_eq!((got.code, &got.stdout), (want.code, &want.stdout));
	assert!(got.stdout.contains("Usage: sloptor build"));
}

#[test]
fn help_is_grouped_and_shows_kebab_names_only() {
	let dir = temp();
	let got = run(&dir.path, &["build", "--help"]);
	for heading in [
		"Input:",
		"Output:",
		"Emit:",
		"Optimization:",
		"Watch:",
		"Diagnostics:",
		"Runtime:",
		"Advanced:",
		"General:",
	] {
		assert!(
			got.stdout.contains(heading),
			"build help lacks {heading}:\n{}",
			got.stdout
		);
	}
	for flag in [
		"--optimized-loops",
		"--include-path [<DIR>]",
		"--builders <N>",
		"--checkers <N>",
		"--single-threaded",
		"Examples:",
	] {
		assert!(got.stdout.contains(flag), "build help lacks {flag}");
	}
	let short = run(&dir.path, &["build", "-h"]);
	for hidden in ["optimizedLoops", "includePath", "[=<BOOL>]", "--no-luau  "] {
		assert!(!short.stdout.contains(hidden), "build help shows {hidden}");
	}
	assert!(!got.stdout.contains("[=<BOOL>]"));
	let project = got.stdout.find("--project").expect("--project");
	let watch = got.stdout.find("--watch").expect("--watch");
	let max_errors = got.stdout.find("--max-errors").expect("--max-errors");
	assert!(project < watch && watch < max_errors);
	assert!(!short.stdout.contains("Examples:"));
	let root = run(&dir.path, &["--help"]);
	let banner = format!("sloptor v{VERSION}");
	for want in [
		banner.as_str(),
		"build",
		"check",
		"completion",
		"Environment:",
		"UV_THREADPOOL_SIZE",
		"Exit codes:",
	] {
		assert!(root.stdout.contains(want), "root help lacks {want}");
	}
}

#[test]
fn colour_follows_the_flag_and_the_environment() {
	let dir = temp();
	let binary = rust_binary();
	let ansi = |env: &[(&str, &str)], args: &[&str]| run_binary(&binary, &dir.path, args, env).stdout.contains("\x1b[");
	assert!(!ansi(&[], &["--help"]), "a pipe is plain");
	assert!(ansi(&[], &["--help", "--color", "always"]));
	assert!(ansi(&[], &["--color=always", "build", "-h"]));
	assert!(ansi(&[("CLICOLOR_FORCE", "1")], &["--help"]));
	assert!(ansi(&[("FORCE_COLOR", "1")], &["--help"]));
	assert!(!ansi(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")], &["--help"]));
	assert!(!ansi(&[("CLICOLOR_FORCE", "1")], &["--help", "--color", "never"]));
	assert!(ansi(&[("NO_COLOR", "1")], &["--help", "--color", "always"]));
	let error = run_binary(&binary, &dir.path, &["nope", "--color", "always"], &[]);
	assert!(
		error.stderr.contains("\x1b["),
		"usage errors honour --color: {:?}",
		error.stderr
	);
	let plain = run_binary(&binary, &dir.path, &["nope"], &[("NO_COLOR", "1")]);
	assert!(!plain.stderr.contains("\x1b["));
}

#[test]
fn build_json_reports_success_and_diagnostics() {
	let dir = temp();
	let clean = buildable_project(&dir.path.join("clean"), "");
	let got = run(&dir.path, &["build", "--json", &arg(&clean)]);
	assert_eq!(got.code, 0, "{}{}", got.stdout, got.stderr);
	let result = json(&got.stdout);
	assert_eq!(result["ok"], true);
	assert_eq!(result["version"], VERSION);
	assert!(result["files"].as_u64() > Some(0));
	assert_eq!(result["diagnostics"], Value::Array(Vec::new()));

	let failing = buildable_project(&dir.path.join("failing"), "export const s: string = 5;\n");
	let got = run(&dir.path, &["build", "--json", &arg(&failing)]);
	assert_eq!(got.code, 1);
	let result = json(&got.stdout);
	assert_eq!(result["ok"], false);
	let first = &result["diagnostics"][0];
	assert_eq!(first["severity"], "error");
	assert_eq!(first["code"], "TS2322");
	assert_eq!(first["file"], "src/main.ts");
	assert_eq!((first["line"].as_u64(), first["col"].as_u64()), (Some(1), Some(14)));

	let two = buildable_project(&dir.path.join("two"), "export const probeA: number = \"a\";\n");
	write(&two.join("src/other.ts"), "export const probeB: number = \"b\";\n");
	let result = json(&run(&dir.path, &["build", "--json", &arg(&two)]).stdout);
	let files: Vec<&str> = result["diagnostics"]
		.as_array()
		.expect("array")
		.iter()
		.filter_map(|d| d["file"].as_str())
		.collect();
	assert!(
		files.contains(&"src/main.ts") && files.contains(&"src/other.ts"),
		"{files:?}"
	);

	let transformer = buildable_project(
		&dir.path.join("noany"),
		"declare const loose: any;\nexport const taken = loose.field;\n",
	);
	let result = json(&run(&dir.path, &["build", "--json", &arg(&transformer)]).stdout);
	assert_eq!(result["diagnostics"][0]["code"], "noAny");
}

#[test]
fn build_json_of_a_reference_only_solution() {
	let dir = temp();
	let root = reference_only_solution(&dir.path);
	let config = arg(&root.join("tsconfig.json"));
	for args in [
		&[
			"build",
			"--build",
			"--project",
			&config,
			"--builders",
			"1",
			"--checkers",
			"1",
			"--json",
		][..],
		&["--build", &config, "--builders=1", "--json"],
	] {
		let got = run(&dir.path, args);
		assert_eq!(got.code, 0, "{args:?}: {}{}", got.stdout, got.stderr);
		let result = json(&got.stdout);
		assert_eq!(result["ok"], true);
		assert!(result["files"].as_u64() > Some(0));
	}
	assert!(root.join("child/out/main.luau").is_file());
	assert!(!root.join("out").exists());
}

#[test]
fn styled_build_failure_renders_code_frames_on_stderr() {
	let dir = temp();
	let failing = buildable_project(&dir.path.join("failing"), "export const s: string = 5;\n");
	let got = run(&dir.path, &["build", &arg(&failing)]);
	assert_eq!(got.code, 1);
	for want in [
		"error[TS2322]: Type 'number' is not assignable to type 'string'.",
		"╭▸ failing/src/main.ts:1:14",
		"1 │ export const s: string = 5;",
		"━",
		"      Failed failing in ",
		"1 error",
	] {
		assert!(got.stderr.contains(want), "stderr lacks {want}:\n{}", got.stderr);
	}
	assert!(!got.stderr.contains("\x1b["), "a pipe is plain");
	let clean = buildable_project(&dir.path.join("clean"), "");
	let got = run(&dir.path, &["build", &arg(&clean)]);
	assert_eq!(got.code, 0, "{}", got.stderr);
	let banner = format!("sloptor v{VERSION}  ·  build  ·  clean");
	for want in [
		banner.as_str(),
		"    Compiled clean (tsconfig.json)",
		"    Finished 1 file in ",
	] {
		assert!(got.stdout.contains(want), "stdout lacks {want}:\n{}", got.stdout);
	}
	assert!(!got.stdout.contains("\x1b["));
	assert!(
		!got.stderr.contains("\x1b["),
		"no progress line on a pipe: {:?}",
		got.stderr
	);
}

#[test]
fn negated_booleans_reach_the_build() {
	let dir = temp();
	for (name, flag, lua) in [
		("negated", "--no-luau", true),
		("equals", "--luau=false", true),
		("zero", "--luau=0", true),
		("negated-true", "--no-luau=true", true),
		("negated-false", "--no-luau=false", false),
		("plain", "--luau", false),
	] {
		let project = buildable_project(&dir.path.join(name), "");
		let got = run(&dir.path, &["build", flag, &arg(&project)]);
		assert_eq!(got.code, 0, "{flag}: {}", got.stderr);
		assert_eq!(project.join("out/main.lua").is_file(), lua, "{flag}");
		assert_eq!(project.join("out/main.luau").is_file(), !lua, "{flag}");
	}
}

#[test]
fn declaration_only_builds_skip_luau() {
	let dir = temp();
	let project = buildable_project(&dir.path, "");
	let config = std::fs::read_to_string(project.join("tsconfig.json")).expect("read");
	write(
		&project.join("tsconfig.json"),
		&config.replace(r#""outDir": "out""#, "\"declaration\": true,\n\t\t\"outDir\": \"out\""),
	);
	let got = run(
		&dir.path,
		&["build", "--build", "--emitDeclarationOnly", &arg(&project)],
	);
	assert_eq!(got.code, 0, "{}{}", got.stdout, got.stderr);
	assert!(!project.join("out/main.luau").exists());
	assert!(project.join("out/main.d.ts").is_file());
}

#[test]
fn timings_write_a_report_for_successful_and_failed_builds() {
	let dir = temp();
	let project = buildable_project(&dir.path.join("ok"), "");
	let report = dir.path.join("reports/ok.json");
	let got = run(&dir.path, &["build", "--timings", &arg(&report), &arg(&project)]);
	assert_eq!(got.code, 0, "{}", got.stderr);
	let timings = json(&std::fs::read_to_string(&report).expect("report"));
	assert_eq!(timings["ok"], true);
	assert_eq!(timings["schemaVersion"], 2);
	for field in ["totalMs", "stageSemantics", "stages", "counts", "metadata"] {
		assert!(timings.get(field).is_some(), "timings lack {field}");
	}
	assert_eq!(timings["counts"]["totalSources"], 1);

	let failing = buildable_project(&dir.path.join("failing"), "export const value: string = 1;\n");
	let report = dir.path.join("failed.json");
	assert_eq!(
		run(&dir.path, &["build", "--timings", &arg(&report), &arg(&failing)]).code,
		1
	);
	assert_eq!(json(&std::fs::read_to_string(&report).expect("report"))["ok"], false);

	let report = dir.path.join("watch.json");
	let got = run(
		&dir.path,
		&["build", "--watch", "--timings", &arg(&report), &arg(&project)],
	);
	assert_eq!(got.code, 1);
	assert!(
		got.stderr.contains("--timings cannot be used with --watch"),
		"{}",
		got.stderr
	);
	assert!(!report.exists());
}

#[test]
fn go_profiles_fail_clearly_before_building() {
	let dir = temp();
	let project = buildable_project(&dir.path, "");
	for flag in [
		"--cpuprofile",
		"--trace-out",
		"--blockprofile",
		"--mutexprofile",
		"--heapprofile",
	] {
		let got = run(&dir.path, &["build", flag, "profile.out", &arg(&project)]);
		assert_eq!(got.code, 1, "{flag}");
		assert!(
			got.stderr.contains(&format!("{flag} writes a Go runtime profile")),
			"{flag}: {}",
			got.stderr
		);
	}
	assert!(!project.join("out").exists());
	let got = run(&dir.path, &["build", "--heapprofile", "p.out", "--timings", "p.out"]);
	assert!(
		got.stderr
			.contains("--heapprofile and --timings cannot write to the same path"),
		"{}",
		got.stderr
	);
}

#[test]
fn check_json_reports_success_and_diagnostics() {
	let dir = temp();
	let clean = checkable_project(&dir.path.join("clean"), "", "");
	let got = run(&dir.path, &["check", "--json", &arg(&clean)]);
	assert_eq!(got.code, 0, "{}{}", got.stdout, got.stderr);
	let result = json(&got.stdout);
	assert_eq!(result["ok"], true);
	assert!(result["files"].as_u64() > Some(0));
	assert_eq!(result["diagnostics"], Value::Array(Vec::new()));

	let failing = checkable_project(&dir.path.join("failing"), "export const s: string = 5;\n", "");
	let got = run(&dir.path, &["check", "--json", &arg(&failing)]);
	assert_eq!(got.code, 1);
	let first = &json(&got.stdout)["diagnostics"][0];
	assert_eq!(first["code"], "TS2322");
	assert_eq!(first["severity"], "error");
	assert_eq!(first["file"], "src/main.ts");
	assert_eq!((first["line"].as_u64(), first["col"].as_u64()), (Some(1), Some(14)));

	// The column counts UTF-16 code units, as tsc's does.
	let wide = checkable_project(
		&dir.path.join("wide"),
		"const é = \"😀\"; export const s: string = 5;\n",
		"",
	);
	let first = &json(&run(&dir.path, &["check", "--json", &arg(&wide)]).stdout)["diagnostics"][0];
	assert_eq!(first["col"], 30);
}

#[test]
fn check_honours_checkers_and_rejects_build_only_flags() {
	let dir = temp();
	let project = checkable_project(&dir.path, "", r#","checkers": 1"#);
	for name in ["a.ts", "b.ts", "c.ts"] {
		write(&project.join("src").join(name), "export {};\n");
	}
	let styled = run(&dir.path, &["check", "--checkers", "3", &arg(&project)]);
	assert_eq!(styled.code, 0, "{}", styled.stderr);
	assert!(styled.stdout.contains("     Checked "), "{}", styled.stdout);
	assert!(styled.stdout.contains("no errors"), "{}", styled.stdout);
	let result = json(&run(&dir.path, &["check", "--json", "--checkers=3", &arg(&project)]).stdout);
	assert_eq!(result["ok"], true);
	for (args, want) in [
		(&["check", "--checkers"][..], "a value is required for '--checkers <N>'"),
		(&["check", "--checkers=0"], "must be a positive integer"),
		(&["check", "--checkers", "-1"], "must be a positive integer"),
		(&["check", "--builders", "2"], "unexpected argument '--builders'"),
	] {
		let got = run(&dir.path, args);
		assert_eq!(got.code, 1, "{args:?}");
		assert!(got.stderr.contains(want), "{args:?}: {}", got.stderr);
	}
}

const COMMONJS: &str = r#","module": "commonjs","moduleResolution": "Node""#;

/// `TestCmdCheckAllowsTopLevelObjectBindingInCommonJSModule`: rotor's tsgo drops the `Object`
/// collision check (it protects only the `CommonJS` JavaScript preamble, which Luau never has).
#[test]
fn check_allows_a_top_level_object_binding_in_commonjs() {
	let dir = temp();
	let object = checkable_project(
		&dir.path,
		"import Object from \"./lib/objutils\";\n\nexport const k = Object.keys({});\n",
		COMMONJS,
	);
	write(
		&object.join("src/lib/objutils.d.ts"),
		"declare const ObjectUtils: { keys(o: object): string[] };\nexport default ObjectUtils;\n",
	);
	let got = run(&dir.path, &["check", "--json", &arg(&object)]);
	assert_eq!(got.code, 0, "{}", got.stdout);
}

#[test]
fn check_still_reserves_require_and_exports_in_commonjs() {
	let dir = temp();
	let options = COMMONJS;
	for (name, source) in [
		("require", "const require = 1;\nexport { require };\n"),
		("exports", "const exports = 1;\nexport { exports };\n"),
	] {
		let project = checkable_project(&dir.path.join(name), source, options);
		let got = run(&dir.path, &["check", "--json", &arg(&project)]);
		assert_eq!(got.code, 1, "{name}");
		assert_eq!(json(&got.stdout)["diagnostics"][0]["code"], "TS2441", "{name}");
	}
}

fn clean_project(root: &std::path::Path, out_dir: &str) {
	write(
		&root.join("tsconfig.json"),
		&format!(r#"{{ "compilerOptions": {{ "rootDir": "src", "outDir": "{out_dir}" }}, "include": ["src"] }}"#),
	);
	write(&root.join("src/main.ts"), "export {};\n");
	write(&root.join("src/nested/mod.ts"), "export const x = 1;\n");
	write(&root.join(out_dir).join("main.luau"), "print('hi')\n");
	write(&root.join(out_dir).join("nested/mod.luau"), "return {}\n");
	write(&root.join("include/RuntimeLib.luau"), "return {}\n");
	for name in ["rotor.d.ts", "rotor-env.d.ts", "rotor-macros.d.ts"] {
		write(&root.join(name), "// generated\n");
	}
}

#[test]
fn clean_removes_outputs_and_never_source() {
	let dir = temp();
	clean_project(&dir.path, "out");
	let got = run(&dir.path, &["clean", &arg(&dir.path)]);
	assert_eq!(got.code, 0, "{}", got.stderr);
	assert!(!dir.path.join("out").exists() && !dir.path.join("include").exists());
	assert!(dir.path.join("src/main.ts").is_file() && dir.path.join("src/nested/mod.ts").is_file());
	assert!(dir.path.join("rotor.d.ts").is_file() && dir.path.join("rotor-env.d.ts").is_file());

	let got = run(&dir.path, &["clean", &arg(&dir.path), "--types"]);
	assert_eq!(got.code, 0);
	for name in ["rotor.d.ts", "rotor-env.d.ts", "rotor-macros.d.ts"] {
		assert!(!dir.path.join(name).exists(), "{name} survived --types");
	}
	let got = run(&dir.path, &["clean", &arg(&dir.path)]);
	assert_eq!(got.code, 0);
	assert!(got.stdout.contains("nothing to clean"), "{}", got.stdout);
}

#[test]
fn clean_dry_run_and_custom_out_dir() {
	let dir = temp();
	clean_project(&dir.path, "out");
	for flag in ["--dry-run", "-n"] {
		let got = run(&dir.path, &["clean", &arg(&dir.path), flag, "--types"]);
		assert_eq!(got.code, 0);
		assert!(got.stdout.contains("Would remove out/"), "{}", got.stdout);
		for path in [
			"out/main.luau",
			"include/RuntimeLib.luau",
			"rotor.d.ts",
			"rotor-env.d.ts",
			"rotor-macros.d.ts",
		] {
			assert!(dir.path.join(path).is_file(), "{flag} removed {path}");
		}
	}
	let custom = temp();
	clean_project(&custom.path, "build/luau");
	assert_eq!(run(&custom.path, &["clean"]).code, 0);
	assert!(!custom.path.join("build/luau").exists());
	assert!(custom.path.join("build").is_dir());
}

#[test]
fn clean_without_a_tsconfig_fails() {
	let dir = temp();
	let got = run(&dir.path, &["clean", "/"]);
	assert_eq!(got.code, 1);
	assert!(got.stderr.contains("Unable to find tsconfig.json!"), "{}", got.stderr);
}

#[test]
fn schema_prints_the_schemas_verbatim() {
	let dir = temp();
	let got = run(&dir.path, &["schema"]);
	assert_eq!(got.code, 0);
	assert_eq!(got.stdout, rotor_config::config::SCHEMA);
	json(&got.stdout);
	let got = run(&dir.path, &["schema", "--rbxts"]);
	assert_eq!(got.code, 0);
	assert!(json(&got.stdout)["properties"].get("rbxts").is_some());
}

#[test]
fn completion_scripts_are_plain_and_name_the_commands() {
	let dir = temp();
	for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
		for command in ["completion", "completions"] {
			let got = run_binary(&rust_binary(), &dir.path, &[command, shell], &[("CLICOLOR_FORCE", "1")]);
			assert_eq!(got.code, 0, "{shell}: {}", got.stderr);
			assert_eq!(got.stderr, "");
			assert!(!got.stdout.contains("\x1b["), "{shell} script has ANSI");
			for want in ["sloptor", "build", "optimized-loops"] {
				assert!(got.stdout.contains(want), "{shell} script lacks {want}");
			}
			assert!(
				!got.stdout.contains("optimizedLoops"),
				"{shell} script offers the alias"
			);
		}
	}
}

#[test]
fn stdio_servers_end_cleanly_at_end_of_input_and_reject_bad_flags() {
	let dir = temp();
	for args in [&["lsp", "--stdio"][..], &["lsp"], &["api-server"]] {
		let got = run_with_stdin(&rust_binary(), &dir.path, args, "");
		assert_eq!((got.code, got.stdout.as_str()), (0, ""), "{args:?}: {}", got.stderr);
	}
	let got = run_with_stdin(
		&rust_binary(),
		&dir.path,
		&["api-server"],
		"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":1,\"clientVersion\":\"0.0.0\"}}\n",
	);
	assert_eq!(
		got.stdout,
		format!(
			"{{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{{\"code\":\"VERSION_MISMATCH\",\"message\":\"client version 0.0.0 does not match server version {VERSION}\"}}}}\n"
		)
	);
	for args in [
		&["lsp", "--bogus"][..],
		&["api-server", "extra"],
		&["diagnostics", "--bogus"],
		&["gen-docs"],
		&["diagnostics", "--builders", "2"],
	] {
		let got = run(&dir.path, args);
		assert_eq!(got.code, 1, "{args:?}");
		assert!(!got.stderr.is_empty(), "{args:?} is a usage error");
	}
}

#[test]
fn build_allows_a_top_level_object_binding_in_commonjs() {
	let dir = temp();
	let project = buildable_project(
		&dir.path,
		"import Object from \"./lib/objutils\";\n\nexport const k = Object.keys({});\n",
	);
	write(
		&project.join("src/lib/objutils.d.ts"),
		"declare const ObjectUtils: { keys(o: object): string[] };\nexport default ObjectUtils;\n",
	);
	let got = run(&dir.path, &["build", "--json", &arg(&project)]);
	assert!(!got.stdout.contains("TS2441"), "{}", got.stdout);
	assert_eq!(got.code, 0, "{}", got.stdout);
}

fn census_project(root: &std::path::Path) -> std::path::PathBuf {
	diagnostics_project(
		root,
		&[
			("clean.ts", "export const clean = 1;\n"),
			("typebad.ts", "export const s: string = 5;\n"),
			(
				"noany.ts",
				"declare const loose: any;\nexport const taken = loose.field;\n",
			),
		],
	)
}

fn outcomes(result: &Value) -> Vec<(String, String)> {
	let mut outcomes: Vec<(String, String)> = result["fileDiagnostics"]
		.as_array()
		.expect("fileDiagnostics")
		.iter()
		.map(|file| {
			let name = file["file"].as_str().unwrap_or_default();
			let name = name.rsplit('/').next().unwrap_or(name).to_owned();
			(name, file["outcome"].as_str().unwrap_or_default().to_owned())
		})
		.collect();
	outcomes.sort();
	outcomes
}

#[test]
fn diagnostics_json_classifies_every_file_and_exits_zero() {
	let dir = temp();
	let project = census_project(&dir.path.join("p"));
	let got = run(&dir.path, &["diagnostics", "--project", &arg(&project), "--json"]);
	assert_eq!(got.code, 0, "{}{}", got.stdout, got.stderr);
	let result = json(&got.stdout);
	assert_eq!(result["ok"], false);
	assert_eq!(result["files"], 3);
	assert_eq!(result["transformed"], 3);
	assert_eq!(
		outcomes(&result),
		[
			("clean.ts".to_owned(), "ok".to_owned()),
			("noany.ts".to_owned(), "transformerDiagnostic".to_owned()),
			("typebad.ts".to_owned(), "typeError".to_owned()),
		]
	);
	assert!(
		result.get("projects").is_none(),
		"a single-project census has no projects key"
	);
	assert!(!project.join("out").exists(), "diagnostics writes nothing");

	let clean = diagnostics_project(&dir.path.join("clean"), &[("a.ts", "export const a = 1;\n")]);
	let result = json(&run(&dir.path, &["diagnostics", "--json", &arg(&clean)]).stdout);
	assert_eq!(result["ok"], true);
}

#[test]
fn diagnostics_overlays_come_from_stdin() {
	let dir = temp();
	let project = census_project(&dir.path.join("p"));
	let typebad = arg(&project.join("src/typebad.ts"));
	let request =
		serde_json::json!({ "overlays": { typebad.clone(): "export const s: string = \"fixed\";\n" } }).to_string();
	let got = run_with_stdin(
		&rust_binary(),
		&dir.path,
		&["diagnostics", "--json", &arg(&project)],
		&request,
	);
	assert_eq!(got.code, 0, "{}", got.stderr);
	let result = json(&got.stdout);
	assert_eq!(result["overlayMatches"], 1);
	assert!(outcomes(&result).contains(&("typebad.ts".to_owned(), "ok".to_owned())));
	assert_eq!(
		std::fs::read_to_string(&typebad).expect("read"),
		"export const s: string = 5;\n"
	);

	let missing = serde_json::json!({ "overlays": { arg(&project.join("src/nope.ts")): "x" } }).to_string();
	let got = run_with_stdin(
		&rust_binary(),
		&dir.path,
		&["diagnostics", "--json", &arg(&project)],
		&missing,
	);
	assert_eq!(got.code, 1, "an overlay that matches nothing fails the run");
	assert_eq!(json(&got.stdout)["ok"], false);

	for bad in [r#"{"overlay":{}}"#, "{", "[1]"] {
		let got = run_with_stdin(
			&rust_binary(),
			&dir.path,
			&["diagnostics", "--json", &arg(&project)],
			bad,
		);
		assert_eq!(got.code, 1, "{bad}");
		assert!(
			got.stderr.contains("parse overlay request from stdin"),
			"{bad}: {}",
			got.stderr
		);
	}
}

#[test]
fn diagnostics_text_and_setup_failures() {
	let dir = temp();
	let project = census_project(&dir.path.join("p"));
	let got = run(&dir.path, &["diagnostics", &arg(&project)]);
	assert_eq!(got.code, 0, "{}", got.stderr);
	for want in [
		"  Type error p/src/typebad.ts",
		" Transformer p/src/noany.ts",
		"error[TS2322]",
		"error[noAny]",
		"    Censused 3 files in ",
		"ok 1  ·  typeError 1  ·  transformerDiagnostic 1  ·  internalCompilerError 0",
	] {
		assert!(got.stdout.contains(want), "stdout lacks {want}:\n{}", got.stdout);
	}
	let got = run(&dir.path, &["diagnostics", "--json", "/"]);
	assert_eq!(got.code, 1);
	assert!(got.stderr.contains("Unable to find tsconfig.json!"), "{}", got.stderr);
	let got = run(&dir.path, &["diagnostics", "--builders", "2"]);
	assert_eq!(got.code, 1);
	assert!(got.stderr.contains("--builders requires --build"), "{}", got.stderr);
}

#[test]
fn gen_docs_writes_a_page_per_visible_command() {
	let dir = temp();
	let docs = dir.path.join("docs/cli");
	let got = run(&dir.path, &["gen-docs", &arg(&docs)]);
	assert_eq!(got.code, 0, "{}", got.stderr);
	let index = std::fs::read_to_string(docs.join("index.mdx")).expect("index.mdx");
	assert!(index.contains("[`build`](./build/)"), "{index}");
	assert!(!index.contains("api-server") && !index.contains("gen-docs"));
	let check = std::fs::read_to_string(docs.join("check.mdx")).expect("check.mdx");
	assert!(
		check.contains("title: check") && check.contains("`--checkers <N>`"),
		"{check}"
	);
}
