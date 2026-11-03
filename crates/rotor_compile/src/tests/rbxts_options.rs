//! `rbxts_options_test.go`

use std::fs;
use std::path::Path;

use rotor_support::logservice;
use rotor_transformer::ProjectType;

use super::{Temp, repo_root};
use crate::ProjectOptions;
use crate::project::project_options_for_referenced_config;
use crate::rbxts_options::{read_rbxts_options, read_rbxts_options_with_chain};
use crate::rbxts_schema::RBXTS_TS_CONFIG_SCHEMA;

fn read(path: &Path) -> crate::rbxts_options::RbxtsOptions {
	read_rbxts_options(&path.to_string_lossy())
		.unwrap_or_else(|error| panic!("{error}"))
		.expect("ReadRbxtsOptions returned nil")
}

fn same_file(a: &str, b: &Path) -> bool {
	fs::canonicalize(a).ok() == fs::canonicalize(b).ok()
}

#[test]
fn rbxts_options_cascade() {
	let root = Temp::new();
	root.write(
		"base/base.json",
		r#"{"rbxts":{"type":"package","luau":true,"luauTypes":true,"nativeTypes":true,"noInclude":true,"optimizedArrayAppends":true,"rojo":"./base.project.json"}}"#,
	);
	root.write(
		"child/tsconfig.json",
		r#"{"extends":"../base/base.json","rbxts":{"type":"game","luau":false,"includePath":"./runtime","rojo":"./child.project.json"}}"#,
	);
	let child_dir = root.join("child");

	let opts = read(&root.join("child/tsconfig.json"));
	assert_eq!(opts.project_type.as_deref(), Some("game"), "type");
	assert_eq!(opts.luau, Some(false), "luau");
	assert_eq!(opts.no_include, Some(true), "noInclude, want true from parent");
	assert_eq!(
		opts.optimized_array_appends,
		Some(true),
		"optimizedArrayAppends, want true from parent"
	);
	assert_eq!(opts.native_types, Some(true), "nativeTypes, want true from parent");
	assert_eq!(opts.luau_types, Some(true), "luauTypes, want true from parent");
	assert_eq!(
		opts.include_path.as_deref(),
		Some(&*child_dir.join("runtime").to_string_lossy()),
		"includePath, want child-relative absolute path"
	);
	assert_eq!(
		opts.rojo.as_deref(),
		Some(&*child_dir.join("child.project.json").to_string_lossy()),
		"rojo, want child-relative absolute path"
	);

	root.write(
		"node_modules/@scope/base/tsconfig.json",
		r#"{"rbxts":{"rojo":"./base.project.json"}}"#,
	);
	root.write(
		"child/module-tsconfig.json",
		r#"{"extends":"@scope/base/tsconfig.json"}"#,
	);
	let module_opts = read(&root.join("child/module-tsconfig.json"));
	let want_module_rojo = root.join("node_modules/@scope/base/base.project.json");
	let got = module_opts.rojo.as_deref().unwrap_or_default();
	assert_eq!(
		Path::new(got).parent().and_then(|dir| fs::canonicalize(dir).ok()),
		want_module_rojo.parent().and_then(|dir| fs::canonicalize(dir).ok()),
		"module extends rojo = {got:?}, want base-relative path"
	);
	assert!(got.ends_with("base.project.json"), "module extends rojo = {got:?}");

	let schema: serde_json::Value = serde_json::from_str(RBXTS_TS_CONFIG_SCHEMA).expect("rbxts schema is valid JSON");
	assert_eq!(
		schema["$id"], "https://raw.githubusercontent.com/howmanyslop/sloptor/main/rbxts-tsconfig.schema.json",
		"schema $id"
	);
	let rbxts = &schema["properties"]["rbxts"];
	assert_eq!(
		rbxts["description"], "roblox-ts compiler options. CLI flags override these at runtime.",
		"rbxts schema description"
	);
	assert_eq!(rbxts["additionalProperties"], false, "schema additionalProperties");
	assert!(
		!rbxts["properties"]["nativeTypes"].is_null() && !rbxts["properties"]["luauTypes"].is_null(),
		"schema does not declare nativeTypes and luauTypes"
	);

	root.write(
		"override.json",
		r#"{"extends":"./base/base.json","rbxts":{"nativeTypes":false,"luauTypes":false}}"#,
	);
	let override_opts = read(&root.join("override.json"));
	assert_eq!(
		override_opts.native_types,
		Some(false),
		"child nativeTypes, want explicit false"
	);
	assert_eq!(
		override_opts.luau_types,
		Some(false),
		"child luauTypes, want explicit false"
	);
}

#[test]
fn rbxts_options_resolve_package_export_extends() {
	let root = Temp::new();
	root.write(
		"node_modules/@isentinel/tsconfig/package.json",
		r#"{"exports":{"./roblox":"./configs/roblox/tsconfig.json"}}"#,
	);
	root.write(
		"node_modules/@isentinel/tsconfig/configs/roblox/tsconfig.json",
		r#"{"rbxts":{"type":"package","rojo":"./default.project.json"}}"#,
	);
	root.write(
		"packages/core/tsconfig.json",
		r#"{"extends":"@isentinel/tsconfig/roblox"}"#,
	);
	let project_config = root.join("packages/core/tsconfig.json");
	let base_config = root.join("node_modules/@isentinel/tsconfig/configs/roblox/tsconfig.json");

	let (opts, chain) =
		read_rbxts_options_with_chain(&project_config.to_string_lossy()).unwrap_or_else(|error| panic!("{error}"));
	let opts = opts.expect("package-export extends options");
	assert_eq!(opts.project_type.as_deref(), Some("package"), "inherited package type");
	assert_eq!(chain.len(), 2, "config chain = {chain:?}");
	assert!(
		same_file(&chain[0], &project_config),
		"config chain[0] = {:?}",
		chain[0]
	);
	assert!(same_file(&chain[1], &base_config), "config chain[1] = {:?}", chain[1]);
	let want_rojo = Path::new(&chain[1])
		.parent()
		.map(|dir| dir.join("default.project.json").to_string_lossy().into_owned());
	assert_eq!(opts.rojo, want_rojo, "rojo relative to exported config");
}

#[test]
fn committed_rbxts_schema_matches_constant() {
	let data = fs::read_to_string(repo_root().join("rbxts-tsconfig.schema.json"))
		.expect("committed rbxts schema missing - run `rotor schema --rbxts > rbxts-tsconfig.schema.json`");
	assert!(
		data == RBXTS_TS_CONFIG_SCHEMA,
		"rbxts-tsconfig.schema.json is out of sync with RbxtsTsConfigSchema"
	);
}

#[test]
fn rbxts_options_validation() {
	let temp = Temp::new();
	temp.write(
		"unknown.json",
		r#"{"rbxts":{"rojo":"a.project.json","nativeTypes":true,"luauTypes":true,"watch":true,"unknownOpt":true}}"#,
	);
	let unknown = temp.join("unknown.json").to_string_lossy().into_owned();
	logservice::capture();
	let opts = read_rbxts_options(&unknown);
	let warnings = logservice::take_capture();
	let opts = opts.unwrap_or_else(|error| panic!("{error}")).expect("options");
	assert!(opts.rojo.is_some(), "known rbxts option was not preserved");
	assert_eq!(
		opts.native_types,
		Some(true),
		"nativeTypes, want the known key to parse"
	);
	assert_eq!(opts.luau_types, Some(true), "luauTypes, want the known key to parse");
	assert!(
		warnings.contains(&format!(r#"Unknown "rbxts" option "watch" in {unknown} (ignored)"#))
			&& warnings.contains(&format!(
				r#"Unknown "rbxts" option "unknownOpt" in {unknown} (ignored)"#
			)),
		"warning = {warnings:?}"
	);
	assert!(
		!warnings.contains("nativeTypes") && !warnings.contains("luauTypes"),
		"nativeTypes and luauTypes must be known keys, warning = {warnings:?}"
	);

	temp.write("bad-type.json", r#"{"rbxts":{"type":"library"}}"#);
	let bad_type = temp.join("bad-type.json").to_string_lossy().into_owned();
	let error = read_rbxts_options(&bad_type).err().map(|error| error.to_string());
	assert_eq!(
		error,
		Some(format!(
			r#"invalid "rbxts" config in {bad_type}: type must be "game", "model" or "package" (was "library")"#
		)),
		"bad type error"
	);

	temp.write("bad-rojo.json", r#"{"rbxts":{"rojo":42}}"#);
	let bad_rojo = temp.join("bad-rojo.json").to_string_lossy().into_owned();
	let error = read_rbxts_options(&bad_rojo).err().map(|error| error.to_string());
	assert_eq!(
		error,
		Some(format!(
			r#"invalid "rbxts" config in {bad_rojo}: rojo must be a string (was a number)"#
		)),
		"bad rojo error"
	);
}

#[test]
fn rbxts_options_rejects_extends_cycle() {
	let temp = Temp::new();
	temp.write("first.json", r#"{"extends":"./second.json"}"#);
	temp.write("second.json", r#"{"extends":"./first.json"}"#);
	let first = temp.join("first.json").to_string_lossy().into_owned();
	let Err(error) = read_rbxts_options(&first) else {
		panic!("ReadRbxtsOptions succeeded for an extends cycle");
	};
	let error = error.to_string();
	assert!(
		error.contains("cycle") && error.contains(&first),
		"cycle error = {error:?}, want the cycle and repeated config"
	);
}

/// The first two halves of `TestPerProjectRbxtsOptions`. Its last half builds a solution
/// (`BuildSolutionWithOptions`), which is 5b.
#[test]
fn per_project_rbxts_options() {
	let temp = Temp::new();
	temp.write(
		"tsconfig.json",
		r#"{"rbxts":{"type":"package","rojo":"./package.project.json","noInclude":true,"luau":false}}"#,
	);
	let entry = ProjectOptions {
		project_type: Some(ProjectType::Game),
		rojo_config_path: "entry.project.json".to_owned(),
		emit_include_files: true,
		lua_extension: false,
		..ProjectOptions::default()
	};
	let got = project_options_for_referenced_config(&entry, &temp.join("tsconfig.json").to_string_lossy(), false)
		.unwrap_or_else(|error| panic!("{error}"));
	assert_eq!(got.project_type, Some(ProjectType::Package), "referenced type");
	assert_eq!(
		got.rojo_config_path,
		temp.join("package.project.json").to_string_lossy(),
		"referenced Rojo config"
	);
	assert!(
		!got.emit_include_files && got.lua_extension,
		"referenced options did not apply noInclude/luau"
	);

	temp.write("empty.json", "{}");
	let got = project_options_for_referenced_config(&entry, &temp.join("empty.json").to_string_lossy(), false)
		.unwrap_or_else(|error| panic!("{error}"));
	assert!(
		got.project_type.is_none() && got.rojo_config_path.is_empty(),
		"referenced defaults should infer type and Rojo config"
	);
}

/// The last half of `TestPerProjectRbxtsOptions`: a solution build applies the child's own
/// `luau` option.
#[test]
fn per_project_rbxts_options_in_a_solution_build() {
	let temp = Temp::new();
	temp.write(
		"child/base.json",
		r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"}}"#,
	);
	temp.write(
		"child/tsconfig.json",
		r#"{"extends":"./base.json","compilerOptions":{"composite":true},"include":["src"],"rbxts":{"type":"package","luau":false}}"#,
	);
	temp.write("child/package.json", r#"{"name":"@scope/child"}"#);
	temp.write("child/src/globals.d.ts", super::NO_LIB_GLOBAL_STUBS);
	temp.write("child/src/main.ts", "export {};\n");
	temp.write("solution.json", r#"{"files":[],"references":[{"path":"./child"}]}"#);
	super::solution_drain::build_solution(
		&temp.join("solution.json").to_string_lossy(),
		&ProjectOptions::default(),
	);
	assert!(
		temp.join("child/out/main.lua").exists(),
		"per-project luau option was not applied"
	);
}
