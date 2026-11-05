//! `project_test.go`

use std::fs;
use std::path::Path;

use rotor_transformer::ProjectType;
use rotor_tsrs::{CompilerOptions, Tristate};

use super::{
	NO_LIB_GLOBAL_STUBS, Temp, expect_compiled, expect_failed, fixture_project_dir, keys, large_stack, output,
	testdata, write_project,
};
use crate::compiler_options::{apply_automatic_types, apply_checker_override, apply_single_threaded_override};
use crate::project::{
	Outputs, compile_project, compile_project_with_options, group_source_files_for_test,
	new_project_program_with_options, project_options_for_referenced_config, project_source_files,
};
use crate::{Failure, ProjectOptions};

const RUNTIME_LIB_MODEL: &str = "-- Compiled with sloptor v3.0.0-beta.3\n\
	local TS = require(script.Parent.include.RuntimeLib)\n\
	local isFoo = TS.instanceof(inst, Foo)\n\
	print(isFoo)\n\
	return nil\n";

fn compile_runtime_lib_project(name: &str) -> Outputs {
	expect_compiled(large_stack(|| compile_project(&testdata(name))))
}

fn options(edit: impl FnOnce(&mut ProjectOptions)) -> ProjectOptions {
	let mut options = ProjectOptions::default();
	edit(&mut options);
	options
}

fn compile_with(dir: &str, opts: &ProjectOptions) -> Result<Outputs, Failure> {
	large_stack(|| compile_project_with_options(dir, opts))
}

#[test]
fn compile_project_runtime_lib_model() {
	let files = compile_runtime_lib_project("runtimelib_model");
	assert_eq!(output(&files, "out/main.luau"), RUNTIME_LIB_MODEL);
	assert_eq!(files.len(), 1, "produced files ({:?})", keys(&files));
}

#[test]
fn compile_project_runtime_lib_game() {
	let files = compile_runtime_lib_project("runtimelib_game");
	let want = "-- Compiled with sloptor v3.0.0-beta.3\n\
		local TS = require(game:GetService(\"ReplicatedStorage\"):WaitForChild(\"include\"):WaitForChild(\"RuntimeLib\"))\n\
		local isFoo = TS.instanceof(inst, Foo)\n\
		print(isFoo)\n\
		return nil\n";
	assert_eq!(output(&files, "out/main.luau"), want);
}

#[test]
fn compile_project_runtime_lib_package() {
	let files = compile_runtime_lib_project("runtimelib_package");
	let want = "-- Compiled with sloptor v3.0.0-beta.3\n\
		local TS = _G[script]\n\
		local isFoo = TS.instanceof(inst, Foo)\n\
		print(isFoo)\n\
		return nil\n";
	assert_eq!(output(&files, "out/main.luau"), want);
}

#[test]
fn compile_project_nested_rojo_project_file() {
	let temp = write_project(
		"nested-project-fixture",
		r#"{"name":"root","tree":{"Main":{"$path":"out/main.luau"},"Pkg":{"$path":"shared.project.json"},"include":{"$path":"include"}}}"#,
	);
	temp.write(
		"shared.project.json",
		r#"{"name":"IgnoredName","tree":{"Dep":{"$path":"out/dep.luau"}}}"#,
	);
	temp.write("src/main.ts", "import { value } from \"./dep\";\nprint(value);\n");
	temp.write("src/dep.ts", "export const value = 42;\n");

	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	let got = output(&files, "out/main.luau");
	assert!(
		got.contains(r#"TS.import(script, script.Parent, "Pkg", "Dep").value"#),
		"nested-project import did not use the mounted output path:\n{got}"
	);
}

/// `TestCompileProjectFixtureParity` is the diff gate, `rotor_transformer`'s `tests/diff.rs`,
/// which compiles the same project through [`compile_project`]; the upstream-corrected rows'
/// runtime verification needs Lune and the pinned upstream compiler.
#[test]
#[ignore = "covered by rotor_transformer's diff gate; the upstream-corrected rows need Lune and the pinned upstream compiler"]
fn compile_project_fixture_parity() {
	let files = expect_compiled(large_stack(|| compile_project(&fixture_project_dir())));
	assert!(!files.is_empty());
}

#[test]
fn compile_project_object_rest_destructuring() {
	let temp = write_project("@scope/object-rest-fixture", "");
	temp.write(
		"tsconfig.json",
		"{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"Preserve\",
		\"moduleResolution\": \"Bundler\",
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
}",
	);
	temp.write(
		"src/main.ts",
		"interface Props { readonly Change: number; readonly Other: string }\n\
		declare const props: Props;\n\
		const { Change: change, ...rest } = props;\n\
		print(change, rest.Other);\n",
	);

	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	let want = "-- Compiled with sloptor v3.0.0-beta.3\n\
		local change = props.Change\n\
		local _extracted = {\n\t[\"Change\"] = true,\n}\n\
		local _rest = {}\n\
		for _k, _v in props do\n\tif not _extracted[_k] then\n\t\t_rest[_k] = _v\n\tend\nend\n\
		local rest = _rest\n\
		print(change, rest.Other)\n\
		return nil\n";
	assert_eq!(output(&files, "out/main.luau"), want);
}

#[test]
fn compile_project_reference_uses_referenced_out_dir() {
	let temp = Temp::new();
	for path in ["src", "test", "out", "include", "node_modules/@rbxts"] {
		fs::create_dir_all(temp.join(path)).expect("a fixture directory");
	}
	let files: [(&str, &str); 9] = [
		("package.json", r#"{"name":"project-reference-fixture"}"#),
		("include/RuntimeLib.lua", "return {}\n"),
		(
			"default.project.json",
			r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"lib":{"$path":"out"},"tests":{"$path":"out-test"}}}}"#,
		),
		(
			"tsconfig.lib.json",
			r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"include":["src"]}"#,
		),
		(
			"tsconfig.test.json",
			r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"test","outDir":"out-test"},"references":[{"path":"./tsconfig.lib.json"}],"include":["test"]}"#,
		),
		("src/value.ts", "export const value = 42;\n"),
		("out/value.d.ts", "export declare const value = 42;\n"),
		("test/globals.d.ts", NO_LIB_GLOBAL_STUBS),
		(
			"test/main.ts",
			"import { value } from \"../src/value\";\nprint(value);\n",
		),
	];
	for (path, content) in files {
		temp.write(path, content);
	}

	let config = temp.join("tsconfig.test.json").to_string_lossy().into_owned();
	let outputs = expect_compiled(compile_with(&temp.dir(), &options(|o| o.ts_config_path = config)));
	let got = output(&outputs, "out-test/main.luau");
	assert!(
		got.contains(r#"TS.import(script, game:GetService("ReplicatedStorage"), "lib", "value").value"#),
		"cross-project import did not use referenced outDir:\n{got}"
	);
}

#[test]
fn compile_project_validation_failures() {
	let tests = [
		(
			"non-package without rojo config",
			"",
			"Non-package projects must have a Rojo project file!",
		),
		(
			"include folder not covered",
			r#"{"name":"x","tree":{"$path":"out"}}"#,
			"Rojo project contained no data for include folder!",
		),
		(
			"runtime lib in server container",
			r#"{"name":"x","tree":{"$className":"DataModel","ServerScriptService":{"out":{"$path":"out"},"include":{"$path":"include"}}}}"#,
			"Runtime library cannot be in a server-only or client-only container!",
		),
		(
			"runtime lib in isolated container",
			r#"{"name":"x","tree":{"$className":"DataModel","PluginDebugService":{"include":{"$path":"include"}},"ReplicatedStorage":{"out":{"$path":"out"}}}}"#,
			"Runtime library cannot be in an isolated container!",
		),
	];
	for (name, rojo_config, want) in tests {
		let temp = write_project("validation-fixture", rojo_config);
		let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
		assert_eq!(diags, [want], "{name}");
	}
}

#[test]
fn compile_project_no_rojo_data_for_file() {
	let temp = write_project(
		"norojodata-fixture",
		r#"{"name":"x","tree":{"$path":"out/sub","include":{"$path":"include"}}}"#,
	);
	temp.write(
		"src/main.ts",
		"declare class Foo {}\ndeclare const inst: object;\nexport const isFoo = inst instanceof Foo;\n",
	);

	let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
	let want = format!(
		"Could not find Rojo data. There is no $path in your Rojo config that covers {}",
		Path::new("out").join("main.luau").display()
	);
	assert_eq!(diags, [want]);
}

#[test]
fn compile_project_syntactic_error_fails() {
	let temp = write_project("@syntax/error-fixture", "");
	temp.write("src/main.ts", "export const x = 5;\n}\n");

	let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
	assert!(!diags.is_empty(), "expected diagnostics for the parse error");
	assert!(
		diags[0].contains("Declaration or statement expected"),
		"diags[0] = {:?}, want the TS1128 'Declaration or statement expected.' diagnostic (all: {diags:?})",
		diags[0]
	);
}

#[test]
fn compile_project_missing_root_dir_out_dir() {
	let temp = Temp::new();
	temp.write(
		"tsconfig.json",
		"{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"CommonJS\",
		\"moduleResolution\": \"Node\",
		\"noLib\": true,
		\"moduleDetection\": \"force\",
		\"strict\": true,
		\"target\": \"ESNext\",
		\"types\": [],
		\"typeRoots\": [\"node_modules/@rbxts\"]
	},
	\"include\": [\"src\"]
}",
	);
	temp.write("src/main.ts", "export {};\n");

	let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
	let want = "Invalid \"tsconfig.json\" configuration!\n\
		https://roblox-ts.com/docs/quick-start#project-folder-setup\n\
		- \"rootDir\" or \"rootDirs\" must be defined\n\
		- \"outDir\" must be defined\n";
	assert_eq!(diags, [want]);
}

/// `requireIncludeFiles`
fn require_include_files(include_dir: &Path) {
	for name in rotor_config::includefiles::names() {
		let got = fs::read(include_dir.join(name)).expect("include emission");
		let want = rotor_config::includefiles::read(name).expect("an embedded runtime file");
		assert_eq!(got, want, "{name}: emitted bytes differ from embedded runtime library");
	}
}

#[test]
fn compile_project_emit_include_files() {
	let temp = write_project(
		"include-fixture",
		r#"{"name":"x","tree":{"$path":"out","include":{"$path":"include"}}}"#,
	);

	expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert!(
		!temp.join("include").exists(),
		"plain CompileProject must not create include/"
	);

	expect_compiled(compile_with(&temp.dir(), &options(|o| o.emit_include_files = true)));
	require_include_files(&temp.join("include"));
}

#[test]
fn compile_project_emit_include_files_skips_package() {
	let temp = write_project("@include/package-fixture", "");
	expect_compiled(compile_with(&temp.dir(), &options(|o| o.emit_include_files = true)));
	assert!(!temp.join("include").exists(), "package project must not get include/");
}

#[test]
fn compile_project_include_path_override() {
	let temp = write_project(
		"includepath-fixture",
		r#"{"name":"x","tree":{"$path":"out","include":{"$path":"runtime"}}}"#,
	);
	let override_path = temp.join("runtime");

	let opts = options(|o| {
		o.include_path = override_path.to_string_lossy().into_owned();
		o.emit_include_files = true;
	});
	expect_compiled(compile_with(&temp.dir(), &opts));
	require_include_files(&override_path);
	assert!(
		!temp.join("include").exists(),
		"default include/ must stay absent when overridden"
	);

	let diags = expect_failed(compile_with(&temp.dir(), &ProjectOptions::default()));
	assert_eq!(diags, ["Rojo project contained no data for include folder!"]);
}

#[test]
fn compile_project_emit_include_files_despite_type_errors() {
	let temp = write_project(
		"include-errors-fixture",
		r#"{"name":"x","tree":{"$path":"out","include":{"$path":"include"}}}"#,
	);
	temp.write("src/main.ts", "const n: number = \"nope\";\nexport {};\n");

	let result = compile_with(&temp.dir(), &options(|o| o.emit_include_files = true));
	assert!(result.is_err(), "expected the type error to fail the compile");
	require_include_files(&temp.join("include"));
}

#[test]
fn compile_project_type_override_package_skips_rojo() {
	let temp = write_project("typeoverride-fixture", "");

	let diags = expect_failed(large_stack(|| compile_project(&temp.dir())));
	assert_eq!(diags, ["Non-package projects must have a Rojo project file!"]);

	let files = expect_compiled(compile_with(
		&temp.dir(),
		&options(|o| o.project_type = Some(ProjectType::Package)),
	));
	assert!(
		files.contains_key("out/main.luau"),
		"out/main.luau missing ({:?})",
		keys(&files)
	);
}

#[test]
fn compile_project_type_override_game_model_require_rojo() {
	for project_type in [ProjectType::Game, ProjectType::Model] {
		let temp = write_project("@scope/typeoverride-fixture", "");

		expect_compiled(large_stack(|| compile_project(&temp.dir())));

		let diags = expect_failed(compile_with(
			&temp.dir(),
			&options(|o| o.project_type = Some(project_type)),
		));
		assert_eq!(
			diags,
			["Non-package projects must have a Rojo project file!"],
			"{project_type:?}"
		);
	}
}

#[test]
fn compile_project_type_override_package_emission() {
	let files = expect_compiled(compile_with(
		&testdata("runtimelib_model"),
		&options(|o| o.project_type = Some(ProjectType::Package)),
	));
	let want = "-- Compiled with sloptor v3.0.0-beta.3\n\
		local TS = _G[script]\n\
		local isFoo = TS.instanceof(inst, Foo)\n\
		print(isFoo)\n\
		return nil\n";
	assert_eq!(output(&files, "out/main.luau"), want);
}

/// The program and its checker groups' count.
fn program_groups(dir: &str, opts: &ProjectOptions) -> (Option<i64>, Tristate, usize) {
	large_stack(|| {
		let (_, program) = match new_project_program_with_options(dir, "", opts) {
			Ok(loaded) => loaded,
			Err(failure) => panic!(
				"newProjectProgramWithOptions: {failure} (diags: {:?})",
				failure.messages()
			),
		};
		let options = program.options();
		let groups = group_source_files_for_test(program, &project_source_files(program));
		(options.checkers, options.single_threaded, groups)
	})
}

#[test]
fn new_project_program_uses_multiple_checker_groups() {
	let temp = write_checker_project(",\n\t\t\"checkers\": 3");
	let (_, _, groups) = program_groups(&temp.dir(), &ProjectOptions::default());
	assert!(groups >= 2, "checker groups = {groups}, want at least 2");
}

#[test]
fn apply_single_threaded_override_cases() {
	let mut options = CompilerOptions::default();
	apply_single_threaded_override(&mut options, None);
	assert_eq!(
		options.single_threaded,
		Tristate::Unknown,
		"nil override changed singleThreaded"
	);
	apply_single_threaded_override(&mut options, Some(true));
	assert_eq!(options.single_threaded, Tristate::True, "true override");
	apply_single_threaded_override(&mut options, Some(false));
	assert_eq!(options.single_threaded, Tristate::False, "false override");
}

#[test]
fn apply_automatic_types_cases() {
	let mut options = CompilerOptions::default();
	apply_automatic_types(&mut options);
	assert_eq!(options.types.as_deref(), Some(&["*".to_owned()][..]), "missing types");

	let mut explicit_empty = CompilerOptions {
		types: Some(Vec::new()),
		..Default::default()
	};
	apply_automatic_types(&mut explicit_empty);
	assert_eq!(
		explicit_empty.types.as_deref(),
		Some(&[][..]),
		"explicit empty types changed"
	);

	let explicit_types = vec!["compiler-types".to_owned(), "jest-extended".to_owned()];
	let mut explicit = CompilerOptions {
		types: Some(explicit_types.clone()),
		..Default::default()
	};
	apply_automatic_types(&mut explicit);
	assert_eq!(explicit.types, Some(explicit_types), "explicit types changed");
}

#[test]
fn effective_solution_builders_single_threaded() {
	let options = |single_threaded| crate::ProjectOptions {
		builders: Some(4),
		single_threaded: Some(single_threaded),
		..crate::ProjectOptions::default()
	};
	assert_eq!(
		crate::effective_solution_builders(&options(true)),
		1,
		"singleThreaded builders"
	);
	assert_eq!(
		crate::effective_solution_builders(&options(false)),
		4,
		"singleThreaded=false builders"
	);
}

#[test]
fn apply_checker_override_cases() {
	let mut options = CompilerOptions {
		checkers: Some(1),
		single_threaded: Tristate::True,
		..Default::default()
	};

	apply_checker_override(&mut options, None);
	assert_eq!(options.checkers, Some(1), "nil override changed checkers");
	assert_eq!(
		options.single_threaded,
		Tristate::True,
		"nil override changed singleThreaded"
	);

	apply_checker_override(&mut options, Some(3));
	assert_eq!(options.checkers, Some(3), "CLI override checkers");
	assert_eq!(
		options.single_threaded,
		Tristate::True,
		"CLI override changed singleThreaded"
	);
}

#[test]
fn new_project_program_checker_override() {
	struct Case {
		name: &'static str,
		config_options: &'static str,
		checkers: Option<i64>,
		want_checkers: Option<i64>,
		want_single: Tristate,
		want_group_count: usize,
	}
	let tests = [
		Case {
			name: "nil CLI preserves configured checkers",
			config_options: ",\n\t\t\"checkers\": 1",
			checkers: None,
			want_checkers: Some(1),
			want_single: Tristate::Unknown,
			want_group_count: 0,
		},
		Case {
			name: "CLI overrides configured checkers",
			config_options: ",\n\t\t\"checkers\": 1",
			checkers: Some(3),
			want_checkers: Some(3),
			want_single: Tristate::Unknown,
			want_group_count: 0,
		},
		Case {
			name: "absent config and CLI leaves checkers nil",
			config_options: "",
			checkers: None,
			want_checkers: None,
			want_single: Tristate::Unknown,
			want_group_count: 0,
		},
		Case {
			name: "single threaded config keeps one effective checker group",
			config_options: ",\n\t\t\"singleThreaded\": true",
			checkers: Some(4),
			want_checkers: Some(4),
			want_single: Tristate::True,
			want_group_count: 1,
		},
	];
	for case in tests {
		let temp = write_checker_project(case.config_options);
		let opts = options(|o| o.checkers = case.checkers);
		let (checkers, single, groups) = program_groups(&temp.dir(), &opts);
		assert_eq!(checkers, case.want_checkers, "{}: checkers", case.name);
		assert_eq!(single, case.want_single, "{}: singleThreaded", case.name);
		if case.want_group_count != 0 {
			assert_eq!(groups, case.want_group_count, "{}: checker groups", case.name);
		}
	}
}

#[test]
fn project_options_for_referenced_config_preserves_checker_overrides() {
	let temp = Temp::new();
	temp.write("tsconfig.json", r#"{"rbxts":{"type":"package"}}"#);
	let config_path = temp.join("tsconfig.json").to_string_lossy().into_owned();
	let entry = options(|o| {
		o.checkers = Some(3);
		o.builders = Some(2);
	});

	let got = project_options_for_referenced_config(&entry, &config_path, false).expect("the referenced options");
	assert_eq!(
		(got.checkers, got.builders),
		(Some(3), Some(2)),
		"referenced options lost CLI overrides"
	);
}

#[test]
fn package_json_name_matches_go_unmarshal() {
	let cases: [(&str, Option<&str>); 8] = [
		(r#"{"name":"@scope/pkg"}"#, Some("@scope/pkg")),
		(r#"{"NAME":"@scope/pkg"}"#, Some("@scope/pkg")),
		(r#"{"Name":"first","name":"@scope/last"}"#, Some("@scope/last")),
		(r#"{"name":"@scope/pkg","nAmE":null}"#, Some("@scope/pkg")),
		(r#"{"name":42}"#, None),
		(r#"["@scope/pkg"]"#, None),
		("null", Some("")),
		("{", None),
	];
	for (data, want) in cases {
		assert_eq!(
			crate::project::package_json_name(data.as_bytes()).as_deref(),
			want,
			"{data}"
		);
	}
}

#[test]
fn compile_project_package_name_key_is_case_insensitive() {
	let temp = write_project("unused", "");
	temp.write("package.json", r#"{"Name":"@scope/case-fixture"}"#);
	let files = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	assert!(
		files.contains_key("out/main.luau"),
		"a scoped Name did not make a package ({:?})",
		keys(&files)
	);
}

/// `writeCheckerProject`
fn write_checker_project(compiler_options: &str) -> Temp {
	let temp = write_project("@scope/checker-options-fixture", "");
	let config = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	let mut config_text = config.clone();
	if !compiler_options.is_empty() {
		config_text = config_text.replacen(
			"\n\t},\n\t\"include\"",
			&format!("{compiler_options}\n\t}},\n\t\"include\""),
			1,
		);
		assert_ne!(config_text, config, "checker project config insertion point not found");
	}
	temp.write("tsconfig.json", config_text);
	for name in ["a.ts", "b.ts", "c.ts", "d.ts"] {
		temp.write(&format!("src/{name}"), "export {};\n");
	}
	temp
}
