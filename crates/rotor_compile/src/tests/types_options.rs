//! `luau_types_test.go`, `luau_types_emit_test.go` and `native_types_test.go`.

use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack, write_project};
use crate::ProjectOptions;
use crate::compile::compile_file_detailed_with_options;
use crate::luau_types::effective_luau_types;
use crate::native_types::effective_native_types;
use crate::project::{new_project_context, new_project_program_with_options};
use crate::rbxts_options::RbxtsOptions;

/// `luauTypesArgv`
fn luau_types_argv(value: bool) -> RbxtsOptions {
	RbxtsOptions {
		luau_types: Some(value),
		..RbxtsOptions::default()
	}
}

/// A project directory holding `tsconfig.json` with `rbxts` (when not empty) and `rotor.toml`.
fn option_project(rbxts: &str, toml: &str) -> (Temp, String) {
	let temp = Temp::new();
	let rbxts = if rbxts.is_empty() {
		String::new()
	} else {
		format!(r#","rbxts": {{{rbxts}}}"#)
	};
	temp.write("tsconfig.json", format!(r#"{{"compilerOptions": {{}}{rbxts}}}"#));
	if !toml.is_empty() {
		temp.write("rotor.toml", toml);
	}
	let config = temp.join("tsconfig.json").to_string_lossy().into_owned();
	(temp, config)
}

/// A case's name, argv layer, tsconfig `rbxts` entry, `rotor.toml`, fallback and wanted value.
type LuauTypesCase<'a> = (&'a str, Option<&'a RbxtsOptions>, &'a str, &'a str, bool, bool);

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test's table")]
fn effective_luau_types_precedence() {
	let empty = RbxtsOptions::default();
	let (on, off) = (luau_types_argv(true), luau_types_argv(false));
	let cases: [LuauTypesCase<'_>; 14] = [
		("default off", None, "", "", false, false),
		("fallback only", None, "", "", true, true),
		("tsconfig true", None, r#""luauTypes": true"#, "", false, true),
		(
			"tsconfig false beats fallback",
			None,
			r#""luauTypes": false"#,
			"",
			true,
			false,
		),
		("rotor.toml true", None, "", "luauTypes = true\n", false, true),
		(
			"rotor.toml false beats fallback",
			None,
			"",
			"luauTypes = false\n",
			true,
			false,
		),
		(
			"tsconfig true beats rotor.toml false",
			None,
			r#""luauTypes": true"#,
			"luauTypes = false\n",
			false,
			true,
		),
		(
			"tsconfig false beats rotor.toml true",
			None,
			r#""luauTypes": false"#,
			"luauTypes = true\n",
			false,
			false,
		),
		(
			"argv true beats tsconfig false",
			Some(&on),
			r#""luauTypes": false"#,
			"",
			false,
			true,
		),
		(
			"argv false beats tsconfig true",
			Some(&off),
			r#""luauTypes": true"#,
			"",
			false,
			false,
		),
		(
			"argv true beats rotor.toml false",
			Some(&on),
			"",
			"luauTypes = false\n",
			false,
			true,
		),
		(
			"argv false beats rotor.toml true and fallback",
			Some(&off),
			"",
			"luauTypes = true\n",
			true,
			false,
		),
		(
			"argv without the flag defers to tsconfig",
			Some(&empty),
			r#""luauTypes": true"#,
			"",
			false,
			true,
		),
		(
			"argv without the flag defers to rotor.toml",
			Some(&empty),
			"",
			"luauTypes = false\n",
			true,
			false,
		),
	];
	for (name, argv, tsconfig, toml, fallback, want) in cases {
		let (temp, config) = option_project(tsconfig, toml);
		assert_eq!(
			effective_luau_types(&temp.dir(), &config, argv, fallback),
			want,
			"{name}: effectiveLuauTypes"
		);
		if tsconfig.is_empty() {
			assert_eq!(
				effective_luau_types(&temp.dir(), "", argv, fallback),
				want,
				"{name}: effectiveLuauTypes without an active tsconfig"
			);
		}
	}
}

#[test]
fn project_context_resolves_luau_types() {
	let temp = write_project("@scope/luau-types-context", "");
	let dir = temp.dir();
	large_stack(|| {
		let (abs_dir, program) = new_project_program_with_options(&dir, "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {failure}"));
		let resolve = |opts: &ProjectOptions| {
			new_project_context(&abs_dir, program, opts)
				.unwrap_or_else(|failure| panic!("newProjectContext: {failure}"))
				.luau_types
		};
		assert!(
			!resolve(&ProjectOptions::default()),
			"an option-free project must resolve luauTypes off"
		);
		let luau_types = ProjectOptions {
			luau_types: true,
			..ProjectOptions::default()
		};
		assert!(resolve(&luau_types), "ProjectOptions.LuauTypes was ignored");
		std::fs::write(std::path::Path::new(&dir).join("rotor.toml"), "luauTypes = true\n").expect("rotor.toml");
		assert!(
			resolve(&ProjectOptions::default()),
			"rotor.toml luauTypes = true did not reach the project context"
		);
		let argv_off = ProjectOptions {
			luau_types: true,
			solution_argv: Some(luau_types_argv(false)),
			..ProjectOptions::default()
		};
		assert!(
			!resolve(&argv_off),
			"--luauTypes=false must beat rotor.toml and the fallback"
		);
	});
}

#[test]
fn effective_native_types_precedence() {
	let cases: [(&str, &str, &str, bool, bool); 8] = [
		("default off", "", "", false, false),
		("fallback only", "", "", true, true),
		("tsconfig true", r#""nativeTypes": true"#, "", false, true),
		(
			"tsconfig false beats fallback",
			r#""nativeTypes": false"#,
			"",
			true,
			false,
		),
		("rotor.toml true", "", "nativeTypes = true\n", false, true),
		(
			"rotor.toml false beats fallback",
			"",
			"nativeTypes = false\n",
			true,
			false,
		),
		(
			"tsconfig true beats rotor.toml false",
			r#""nativeTypes": true"#,
			"nativeTypes = false\n",
			false,
			true,
		),
		(
			"tsconfig false beats rotor.toml true",
			r#""nativeTypes": false"#,
			"nativeTypes = true\n",
			false,
			false,
		),
	];
	for (name, tsconfig, toml, fallback, want) in cases {
		let (temp, config) = option_project(tsconfig, toml);
		assert_eq!(
			effective_native_types(&temp.dir(), &config, fallback),
			want,
			"{name}: effectiveNativeTypes"
		);
		if tsconfig.is_empty() {
			assert_eq!(
				effective_native_types(&temp.dir(), "", fallback),
				want,
				"{name}: effectiveNativeTypes without an active tsconfig"
			);
		}
	}
}

/// The project tsconfig with `"rbxts": {"<key>": <declared>}` when `declared` is not empty
/// (`luauTypesEmitTsconfig` and the native-types test's `tsconfig`).
fn types_tsconfig(key: &str, declared: &str) -> String {
	let rbxts = if declared.is_empty() {
		String::new()
	} else {
		format!(",\n\t\"rbxts\": {{\"{key}\": {declared}}}")
	};
	format!(
		"{{
	\"compilerOptions\": {{
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
	}},
	\"include\": [\"src\"]{rbxts}
}}"
	)
}

fn render(temp: &Temp, rel: &str, opts: &ProjectOptions) -> String {
	match large_stack(|| compile_file_detailed_with_options(&temp.dir(), rel, opts)) {
		Ok(compiled) => String::from_utf8_lossy(&compiled.text).into_owned(),
		Err(failure) => panic!("compile: {failure} (diagnostics: {:?})", failure.messages()),
	}
}

#[test]
fn compile_file_native_types_option() {
	let temp = write_project("@scope/native-types-e2e", "");
	temp.write(
		"src/native.ts",
		"//!native\nexport function scale(x: number, factor: number): number {\n\treturn x * factor;\n}\n",
	);
	let annotated = "local function scale(x: number, factor: number): number\n";
	let default = ProjectOptions::default();

	let got = render(&temp, "src/native.ts", &default);
	assert!(
		!got.contains(": number") && got.contains("--!native\n"),
		"default output must keep the directive and stay unannotated:\n{got}"
	);
	temp.write("rotor.toml", "nativeTypes = true\n");
	let got = render(&temp, "src/native.ts", &default);
	assert!(
		got.contains(annotated),
		"rotor.toml nativeTypes = true did not annotate:\n{got}"
	);
	temp.write("tsconfig.json", types_tsconfig("nativeTypes", "false"));
	let got = render(&temp, "src/native.ts", &default);
	assert!(
		!got.contains(": number"),
		"tsconfig nativeTypes = false must beat rotor.toml:\n{got}"
	);
	temp.write("tsconfig.json", types_tsconfig("nativeTypes", "true"));
	let got = render(&temp, "src/native.ts", &default);
	assert!(
		got.contains(annotated),
		"tsconfig nativeTypes = true did not annotate:\n{got}"
	);
	temp.write("rotor.toml", "nativeTypes = false\n");
	temp.write("tsconfig.json", types_tsconfig("nativeTypes", ""));
	let got = render(&temp, "src/native.ts", &default);
	assert!(
		!got.contains(": number"),
		"rotor.toml nativeTypes = false must stay unannotated:\n{got}"
	);
	std::fs::remove_file(temp.join("rotor.toml")).expect("rotor.toml removes");
	let native = ProjectOptions {
		native_types: true,
		..ProjectOptions::default()
	};
	let got = render(&temp, "src/native.ts", &native);
	assert!(
		got.contains(annotated),
		"ProjectOptions.NativeTypes was ignored:\n{got}"
	);
}

#[test]
fn compile_file_luau_types_option() {
	let alias = "export type Point = { x: number, y: number }\n";
	let signature = "local function scale(point: Point, factor: number): Point\n";
	let temp = write_project("@scope/luau-types-e2e", "");
	temp.write(
		"src/globals.d.ts",
		format!(
			"{NO_LIB_GLOBAL_STUBS}interface Array<T> {{ size(): number }}\ninterface ReadonlyArray<T> {{ size(): number }}\n"
		),
	);
	temp.write(
		"src/point.ts",
		"export interface Point {\n\tx: number;\n\ty: number;\n}\n\nexport function scale(point: Point, factor: number): Point {\n\treturn { x: point.x * factor, y: point.y * factor };\n}\n",
	);
	let options = |luau_types: bool, argv: Option<RbxtsOptions>| ProjectOptions {
		luau_types,
		solution_argv: argv,
		..ProjectOptions::default()
	};
	let assert_on = |surface: &str, got: String| {
		assert!(
			got.contains(alias) && got.contains(signature),
			"{surface}: output lacks the alias or the annotated signature:\n{got}"
		);
	};

	let baseline = render(&temp, "src/point.ts", &options(false, None));
	assert!(
		!baseline.contains("export type") && !baseline.contains(": number"),
		"the option-free output must stay unannotated:\n{baseline}"
	);
	assert!(
		baseline.contains("local function scale(point, factor)\n"),
		"the option-free output lost the plain signature:\n{baseline}"
	);
	let assert_off =
		|surface: &str, got: String| assert_eq!(got, baseline, "{surface}: output differs from the option-free build");

	assert_on(
		"ProjectOptions.LuauTypes",
		render(&temp, "src/point.ts", &options(true, None)),
	);
	temp.write("rotor.toml", "luauTypes = true\n");
	assert_on(
		"rotor.toml luauTypes = true",
		render(&temp, "src/point.ts", &options(false, None)),
	);
	temp.write("tsconfig.json", types_tsconfig("luauTypes", "false"));
	assert_off(
		"tsconfig luauTypes = false over rotor.toml true",
		render(&temp, "src/point.ts", &options(false, None)),
	);
	temp.write("tsconfig.json", types_tsconfig("luauTypes", "true"));
	assert_on(
		"tsconfig luauTypes = true",
		render(&temp, "src/point.ts", &options(false, None)),
	);
	assert_off(
		"--luauTypes=false over tsconfig true",
		render(&temp, "src/point.ts", &options(false, Some(luau_types_argv(false)))),
	);
	temp.write("tsconfig.json", types_tsconfig("luauTypes", "false"));
	assert_on(
		"--luauTypes over tsconfig false",
		render(&temp, "src/point.ts", &options(false, Some(luau_types_argv(true)))),
	);
	assert_off(
		"silent argv over tsconfig false",
		render(&temp, "src/point.ts", &options(true, Some(RbxtsOptions::default()))),
	);
	temp.write("tsconfig.json", types_tsconfig("luauTypes", ""));
	temp.write("rotor.toml", "luauTypes = false\n");
	assert_off(
		"rotor.toml luauTypes = false over ProjectOptions true",
		render(&temp, "src/point.ts", &options(true, None)),
	);
	std::fs::remove_file(temp.join("rotor.toml")).expect("rotor.toml removes");
	assert_off(
		"option-free project",
		render(&temp, "src/point.ts", &options(false, None)),
	);
}
