//! Port of `internal/rojo/resolver_test.go`.

use std::fs;

use rotor_support::gopath;

use super::{
	FileRelation, NetworkType, RbxPathLookup, RbxType, RelativeRbxPathSegment, RojoResolver,
	find_rojo_config_file_path, realpath_or, relative,
};
use crate::config::{Tree, TreeEntry};
use crate::project_files::ProjectFiles;
use crate::test_support::{join_slashed, must_rbx_path, rbx, write_project};

// --- fixture project (Model type, the diff harness project) ---

fn fixture_project_dir() -> Option<String> {
	let dir = gopath::join(&[env!("CARGO_MANIFEST_DIR"), "..", "..", "testdata", "diff", "project"]);
	if fs::metadata(&dir).is_err() {
		// fixture project not present
		return None;
	}
	Some(dir)
}

#[test]
fn from_path_fixture_project() {
	let Some(dir) = fixture_project_dir() else {
		return;
	};
	let r = RojoResolver::from_path(&gopath::join(&[&dir, "default.project.json"]));

	assert!(r.warnings().is_empty(), "unexpected warnings: {:?}", r.warnings());
	assert!(!r.is_game, "fixture project should not be a game");

	// fromPath passes doNotPush=true, so the project name ("fixture") is NOT
	// part of any rbxPath (RojoResolver.ts L197-201, L243-244).
	let tests = [
		("out/01_literals.luau", rbx(&["01_literals"])),
		// .lua converts to .luau before lookup (convertToLuau, L154-158).
		("include/RuntimeLib.lua", rbx(&["include", "RuntimeLib"])),
		("include/RuntimeLib.luau", rbx(&["include", "RuntimeLib"])),
		(
			"node_modules/@rbxts/types/x.luau",
			rbx(&["node_modules", "@rbxts", "types", "x"]),
		),
	];
	for (rel, want) in tests {
		let got = must_rbx_path(&r, &join_slashed(&dir, rel));
		assert_eq!(got, want, "get_rbx_path_from_file_path({rel})");
	}

	// Partitions are LIFO: later tree entries were unshifted to the front
	// (RojoResolver.ts L272-275). The leading "src" partition is
	// @rbxts/react's own nested default.project.json (`"tree": {"$path":
	// "src"}`), parsed while walking the node_modules/@rbxts partition;
	// upstream parses nested project files the same way.
	let suffixes: Vec<String> = r.partitions().iter().map(|p| gopath::base(&p.fs_path)).collect();
	assert_eq!(suffixes, ["src", "@rbxts", "include", "out"], "partition order");

	// Files outside every partition have no Rojo data.
	assert!(
		r.get_rbx_path_from_file_path(&gopath::join(&[&dir, "src", "01_literals.ts"]))
			.is_none(),
		"src file should have no Rojo data (only out is mapped)"
	);

	// Runtime lib relative chain (digest 5.3): out/X.luau ->
	// include/RuntimeLib: [Parent, include, RuntimeLib].
	let file_rbx = must_rbx_path(&r, &gopath::join(&[&dir, "out", "01_literals.luau"]));
	let lib_rbx = must_rbx_path(&r, &gopath::join(&[&dir, "include", "RuntimeLib.lua"]));
	let want_rel = vec![
		RelativeRbxPathSegment::Parent,
		RelativeRbxPathSegment::Name("include".to_owned()),
		RelativeRbxPathSegment::Name("RuntimeLib".to_owned()),
	];
	assert_eq!(relative(&file_rbx, &lib_rbx), want_rel, "relative");
}

// --- game-shaped project ---

const GAME_PROJECT_JSON: &str = r#"{
	"name": "game",
	"tree": {
		"$className": "DataModel",
		"ServerScriptService": {
			"$className": "ServerScriptService",
			"TS": { "$path": "out/server" }
		},
		"ReplicatedStorage": {
			"$className": "ReplicatedStorage",
			"rbxts_include": {
				"$path": "include",
				"node_modules": {
					"$className": "Folder",
					"@rbxts": { "$path": "node_modules/@rbxts" }
				}
			},
			"TS": { "$path": "out/shared" }
		},
		"StarterPlayer": {
			"$className": "StarterPlayer",
			"StarterPlayerScripts": {
				"$className": "StarterPlayerScripts",
				"TS": { "$path": "out/client" }
			}
		}
	}
}"#;

fn game_resolver() -> (RojoResolver, crate::test_support::Project) {
	let project = write_project(&[
		("default.project.json", GAME_PROJECT_JSON),
		("out/server/", ""),
		("out/shared/", ""),
		("out/client/", ""),
		("include/", ""),
		("node_modules/@rbxts/", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	assert!(r.warnings().is_empty(), "unexpected warnings: {:?}", r.warnings());
	(r, project)
}

#[test]
fn game_project() {
	let (r, project) = game_resolver();
	assert!(r.is_game, "$className DataModel must set is_game");

	let server_main = must_rbx_path(&r, &project.path("out/server/main.server.luau"));
	assert_eq!(server_main, rbx(&["ServerScriptService", "TS", "main"]), "server main");

	let shared_mod = must_rbx_path(&r, &project.path("out/shared/mod.luau"));
	assert_eq!(shared_mod, rbx(&["ReplicatedStorage", "TS", "mod"]), "shared mod");

	// init.luau collapses into its directory (script exts only).
	let client_foo = must_rbx_path(&r, &project.path("out/client/foo/init.luau"));
	assert_eq!(
		client_foo,
		rbx(&["StarterPlayer", "StarterPlayerScripts", "TS", "foo"]),
		"client foo init"
	);

	let runtime_lib = must_rbx_path(&r, &project.path("include/RuntimeLib.lua"));
	assert_eq!(
		runtime_lib,
		rbx(&["ReplicatedStorage", "rbxts_include", "RuntimeLib"]),
		"runtime lib"
	);

	// Network types (SERVER_CONTAINERS checked before CLIENT_CONTAINERS).
	assert_eq!(
		r.get_network_type(&server_main),
		NetworkType::Server,
		"server network type"
	);
	assert_eq!(
		r.get_network_type(&client_foo),
		NetworkType::Client,
		"client network type"
	);
	assert_eq!(
		r.get_network_type(&shared_mod),
		NetworkType::Unknown,
		"shared network type"
	);
	assert_eq!(
		r.get_network_type(&runtime_lib),
		NetworkType::Unknown,
		"runtime lib network type"
	);

	// Isolation: StarterPlayer/StarterPlayerScripts is a default isolated
	// container; ServerScriptService and ReplicatedStorage are not.
	assert!(r.is_isolated(&client_foo), "StarterPlayerScripts path must be isolated");
	assert!(
		!r.is_isolated(&server_main) && !r.is_isolated(&shared_mod) && !r.is_isolated(&runtime_lib),
		"non-Starter containers must not be isolated"
	);

	// File relations.
	let rel_tests = [
		("server->shared", &server_main, &shared_mod, FileRelation::OutToOut),
		("client->shared", &client_foo, &shared_mod, FileRelation::InToOut),
		("shared->client", &shared_mod, &client_foo, FileRelation::OutToIn),
		(
			"client->client",
			&client_foo,
			&rbx(&["StarterPlayer", "StarterPlayerScripts", "TS", "other"]),
			FileRelation::InToIn,
		),
		// Different isolated containers: OutToIn.
		(
			"gui->client",
			&rbx(&["StarterGui", "x"]),
			&client_foo,
			FileRelation::OutToIn,
		),
	];
	for (name, file, module, want) in rel_tests {
		assert_eq!(r.get_file_relation(file, module), want, "get_file_relation({name})");
	}
}

#[test]
fn isolation_only_applies_to_games() {
	// getContainer requires isGame (RojoResolver.ts L351-361); a Model project
	// treats Starter-shaped paths as ordinary.
	let project = write_project(&[
		("default.project.json", r#"{"name":"model","tree":{"$path":"out"}}"#),
		("out/", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	let p = rbx(&["StarterPlayer", "StarterPlayerScripts", "x"]);
	assert!(!r.is_isolated(&p), "non-game resolver must not isolate");
	assert_eq!(
		r.get_file_relation(&p, &rbx(&["y"])),
		FileRelation::OutToOut,
		"non-game relation"
	);
	// Network containers do NOT require isGame (getNetworkType uses
	// getContainer too... which checks isGame) so Unknown here.
	assert_eq!(
		r.get_network_type(&rbx(&["ServerScriptService", "x"])),
		NetworkType::Unknown,
		"non-game network type"
	);
}

// --- synthetic ---

#[test]
fn synthetic() {
	let project = write_project(&[]);
	let r = RojoResolver::synthetic(&project.dir);
	assert!(!r.is_game, "synthetic resolver is never a game");
	let tests = [
		("foo.luau", rbx(&["foo"])),
		("a/b.luau", rbx(&["a", "b"])),
		("foo/init.luau", rbx(&["foo"])),
		("init.luau", rbx(&[])),
		("data.json", rbx(&["data"])),
		// Sub-extensions strip before the init check.
		("foo/init.client.luau", rbx(&["foo"])),
		// The trailing-init pop only applies to script exts: init.json keeps
		// its name (RojoResolver.ts L331 checks ROJO_SCRIPT_EXTS).
		("foo/init.json", rbx(&["foo", "init"])),
	];
	for (rel, want) in tests {
		let got = must_rbx_path(&r, &project.path(rel));
		assert_eq!(got, want, "synthetic get_rbx_path_from_file_path({rel})");
	}
	assert!(
		r.get_rbx_path_from_file_path(&gopath::join(&[&gopath::dir(&project.dir), "outside.luau"]))
			.is_none(),
		"file outside basePath must have no Rojo data"
	);
}

// --- partition LIFO + exact file mappings ---

#[test]
fn package_imports_project_composition() {
	struct Case {
		name: &'static str,
		files: Vec<(&'static str, &'static str)>,
		path: &'static str,
		want: Vec<String>,
	}
	let cases = [
		Case {
			name: "nested default",
			files: vec![
				(
					"lib/out/Foo/default.project.json",
					r#"{"name":"FooRenamed","tree":{"$path":"src"}}"#,
				),
				("lib/out/Foo/src/init.luau", "return {}"),
			],
			path: "lib/out/Foo/src/init.luau",
			want: rbx(&["lib", "out", "FooRenamed"]),
		},
		Case {
			name: "explicit named project",
			files: vec![
				(
					"lib/default.project.json",
					r#"{"name":"runtime","tree":{"Child":{"$path":"layout.project.json"}}}"#,
				),
				(
					"lib/layout.project.json",
					r#"{"name":"ignored","tree":{"$path":"out"}}"#,
				),
				("lib/out/init.lua", "return {}"),
			],
			path: "lib/out/init.lua",
			want: rbx(&["runtime", "Child"]),
		},
		// Preserved behavior: defaults can mount a sibling output tree. Only
		// implicitly discovered named projects are excluded by this fix.
		Case {
			name: "nested default duplicate",
			files: vec![
				(
					"lib/.cache/place/default.project.json",
					r#"{"name":"testing","tree":{"$path":"../../out"}}"#,
				),
				("lib/out/init.luau", "return {}"),
			],
			path: "lib/out/init.luau",
			want: rbx(&["lib", ".cache", "testing"]),
		},
	];
	for case in cases {
		let project = write_project(&case.files);
		let r = RojoResolver::package_imports(&project.dir, &ProjectFiles::new());
		assert_eq!(
			must_rbx_path(&r, &project.path(case.path)),
			case.want,
			"{}: package path",
			case.name
		);
	}
}

#[test]
fn partitions_lifo() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"all":{"$path":"src"},"sub":{"$path":"src/special"}}}"#,
		),
		("src/", ""),
		("src/special/", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	// src/special is covered by BOTH partitions; the later tree entry was
	// unshifted to the front and wins.
	assert_eq!(
		must_rbx_path(&r, &project.path("src/special/x.luau")),
		rbx(&["sub", "x"]),
		"overlapping partition"
	);
	assert_eq!(
		must_rbx_path(&r, &project.path("src/y.luau")),
		rbx(&["all", "y"]),
		"outer partition"
	);
}

#[test]
fn exact_file_mapping() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"data":{"$path":"misc/data.json"},"legacy":{"$path":"misc/legacy.lua"}}}"#,
		),
		("misc/data.json", "{}"),
		("misc/legacy.lua", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	assert_eq!(
		must_rbx_path(&r, &project.path("misc/data.json")),
		rbx(&["data"]),
		"json mapping"
	);
	// $path "legacy.lua" was stored under the .luau key; both spellings hit.
	for name in ["legacy.lua", "legacy.luau"] {
		let got = must_rbx_path(&r, &project.path(&format!("misc/{name}")));
		assert_eq!(got, rbx(&["legacy"]), "lua mapping ({name})");
	}
	// Sibling files in the same dir are NOT covered (no partition was added).
	assert!(
		r.get_rbx_path_from_file_path(&project.path("misc/other.luau"))
			.is_none(),
		"sibling of exact file mapping must have no Rojo data"
	);
}

// --- nested project files ---

#[test]
fn nested_default_project_via_path() {
	// $path pointing at a dir that contains default.project.json parses that
	// config WITHOUT pushing its name (parsePath L268-270 -> parseConfig
	// doNotPush=true).
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"child":{"$path":"pkg"}}}"#,
		),
		(
			"pkg/default.project.json",
			r#"{"name":"pkgname","tree":{"$path":"src"}}"#,
		),
		("pkg/src/", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	assert_eq!(
		must_rbx_path(&r, &project.path("pkg/src/x.luau")),
		rbx(&["child", "x"]),
		"nested default project"
	);
}

#[test]
fn nested_project_file_via_path() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"root","tree":{"Pkg":{"$path":"projects/shared.project.json"}}}"#,
		),
		(
			"projects/shared.project.json",
			r#"{"name":"IgnoredName","tree":{"$path":"src","Nested":{"$path":"nested.project.json"}}}"#,
		),
		(
			"projects/nested.project.json",
			r#"{"name":"AlsoIgnored","tree":{"$path":"nested-src"}}"#,
		),
		("projects/src/root.luau", "return {}"),
		("projects/nested-src/leaf.luau", "return {}"),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));

	assert_eq!(
		must_rbx_path(&r, &project.path("projects/src/root.luau")),
		rbx(&["Pkg", "root"]),
		"direct nested project path"
	);
	assert_eq!(
		must_rbx_path(&r, &project.path("projects/nested-src/leaf.luau")),
		rbx(&["Pkg", "Nested", "leaf"]),
		"recursive nested project path"
	);

	let state = r.state();
	let shared_config = gopath::clean(&realpath_or(&project.path("projects/shared.project.json")));
	let nested_config = gopath::clean(&realpath_or(&project.path("projects/nested.project.json")));
	assert!(
		state.walked_configs.contains(&shared_config) && state.walked_configs.contains(&nested_config),
		"walked configs = {:?}, want {shared_config:?} and {nested_config:?}",
		state.walked_configs
	);
	for file_path in &state.file_paths {
		assert!(
			file_path.path != shared_config && file_path.path != nested_config,
			"nested project was registered as an exact module mapping: {file_path:?}"
		);
	}
	assert!(
		r.get_rbx_path_from_file_path(&shared_config).is_none(),
		"nested project config itself must not resolve as a Roblox module"
	);
}

#[test]
fn nested_project_file_warnings() {
	// missing
	{
		let project = write_project(&[(
			"default.project.json",
			r#"{"name":"root","tree":{"Pkg":{"$path":"missing.project.json"}}}"#,
		)]);
		let r = RojoResolver::from_path(&project.path("default.project.json"));
		let warnings = r.warnings();
		assert!(
			warnings.len() == 1 && warnings[0].contains("Path does not exist"),
			"warnings = {warnings:?}, want one missing-path warning"
		);
		assert!(
			r.get_rbx_path_from_file_path(&project.path("missing.project.json"))
				.is_none(),
			"missing nested project must not become an exact module mapping"
		);
	}

	// malformed
	{
		let project = write_project(&[
			(
				"default.project.json",
				r#"{"name":"root","tree":{"Pkg":{"$path":"broken.project.json"}}}"#,
			),
			("broken.project.json", "{not json"),
		]);
		let r = RojoResolver::from_path(&project.path("default.project.json"));
		let warnings = r.warnings();
		assert!(
			warnings.len() == 1 && warnings[0].contains("Invalid configuration"),
			"warnings = {warnings:?}, want one invalid-configuration warning"
		);
		assert!(
			r.get_rbx_path_from_file_path(&project.path("broken.project.json"))
				.is_none(),
			"malformed nested project must not become an exact module mapping"
		);
	}
}

#[test]
fn nested_project_file_symlink() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"root","tree":{"Pkg":{"$path":"links/shared.project.json"}}}"#,
		),
		(
			"real/shared.project.json",
			r#"{"name":"ignored","tree":{"$path":"../content"}}"#,
		),
		("content/value.luau", "return {}"),
		("links/", ""),
	]);
	let link_path = project.path("links/shared.project.json");
	if !crate::test_support::file_link(&link_path, &project.path("real/shared.project.json")) {
		// cannot create project-file symlink
		return;
	}

	let r = RojoResolver::from_path(&project.path("default.project.json"));
	assert_eq!(
		must_rbx_path(&r, &project.path("content/value.luau")),
		rbx(&["Pkg", "value"]),
		"symlinked nested project path"
	);
	let want = gopath::clean(&realpath_or(&project.path("real/shared.project.json")));
	assert!(
		r.state().walked_configs.contains(&want),
		"walked configs = {:?}, want real target {want:?}",
		r.state().walked_configs
	);
}

#[test]
fn search_directory_finds_nested_projects() {
	// A default.project.json discovered while WALKING a partition directory
	// goes through parseConfig with doNotPush=false: its name IS pushed
	// (searchDirectory L288-291), and the dir name it lives in is not.
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"parent":{"$path":"area"}}}"#,
		),
		("area/y.luau", ""),
		(
			"area/inner/default.project.json",
			r#"{"name":"innerproj","tree":{"$path":"lib"}}"#,
		),
		("area/inner/lib/", ""),
		(
			"area/other.project.json",
			r#"{"name":"other","tree":{"$path":"otherlib"}}"#,
		),
		("area/otherlib/", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));

	assert_eq!(
		must_rbx_path(&r, &project.path("area/y.luau")),
		rbx(&["parent", "y"]),
		"partition file"
	);
	// LIFO also guarantees the inner partition beats the outer one.
	assert_eq!(
		must_rbx_path(&r, &project.path("area/inner/lib/x.luau")),
		rbx(&["parent", "innerproj", "x"]),
		"nested-by-walk project"
	);
	// Non-default *.project.json files are parsed too (name pushed).
	assert_eq!(
		must_rbx_path(&r, &project.path("area/otherlib/z.luau")),
		rbx(&["parent", "other", "z"]),
		"named project file"
	);
}

// --- GetRbxTypeFromFilePath ---

#[test]
fn get_rbx_type_from_file_path() {
	let project = write_project(&[]);
	let r = RojoResolver::synthetic(&project.dir);
	let tests = [
		("foo.luau", RbxType::ModuleScript),
		("foo.server.luau", RbxType::Script),
		("foo.client.luau", RbxType::LocalScript),
		// .lua converts before classification.
		("foo.server.lua", RbxType::Script),
		// Unrecognized sub-extension on a script ext -> Unknown.
		("foo.weird.luau", RbxType::Unknown),
		// Non-script exts cannot use sub-extensions: always ModuleScript.
		("foo.json", RbxType::ModuleScript),
		("foo.server.json", RbxType::ModuleScript),
		("foo.toml", RbxType::ModuleScript),
		("foo.txt", RbxType::ModuleScript),
	];
	for (input, want) in tests {
		assert_eq!(
			r.get_rbx_type_from_file_path(input),
			want,
			"get_rbx_type_from_file_path({input:?})"
		);
	}
}

// --- Relative ---

#[test]
fn relative_paths() {
	let seg = |name: &str| RelativeRbxPathSegment::Name(name.to_owned());
	let parent = RelativeRbxPathSegment::Parent;
	let tests = [
		("identical", rbx(&["a", "b"]), rbx(&["a", "b"]), vec![]),
		(
			"sibling",
			rbx(&["a", "b"]),
			rbx(&["a", "c"]),
			vec![parent.clone(), seg("c")],
		),
		("descend", rbx(&["a"]), rbx(&["a", "b", "c"]), vec![seg("b"), seg("c")]),
		(
			"ascend",
			rbx(&["a", "b", "c"]),
			rbx(&["a"]),
			vec![parent.clone(), parent.clone()],
		),
		("disjoint", rbx(&["x"]), rbx(&["y"]), vec![parent, seg("y")]),
		("empty from", rbx(&[]), rbx(&["a"]), vec![seg("a")]),
	];
	for (name, from, to, want) in tests {
		assert_eq!(relative(&from, &to), want, "relative({name})");
	}
}

// --- FindRojoConfigFilePath ---

#[test]
fn find_rojo_config_file_path_cases() {
	// default wins
	{
		let project = write_project(&[("default.project.json", "{}"), ("other.project.json", "{}")]);
		let (got, warnings) = find_rojo_config_file_path(&project.dir);
		assert_eq!(got, Some(project.path("default.project.json")));
		assert!(warnings.is_empty(), "warnings = {warnings:?}");
	}
	// single candidate
	{
		let project = write_project(&[("game.project.json", "{}")]);
		let (got, warnings) = find_rojo_config_file_path(&project.dir);
		assert_eq!(got, Some(project.path("game.project.json")));
		assert!(warnings.is_empty(), "warnings = {warnings:?}");
	}
	// legacy name
	{
		let project = write_project(&[("roblox-project.json", "{}")]);
		let (got, _) = find_rojo_config_file_path(&project.dir);
		assert_eq!(got, Some(project.path("roblox-project.json")));
	}
	// multiple candidates warn
	{
		let project = write_project(&[("a.project.json", "{}"), ("b.project.json", "{}")]);
		let (got, warnings) = find_rojo_config_file_path(&project.dir);
		assert_eq!(got, Some(project.path("a.project.json")));
		assert!(
			warnings.len() == 1 && warnings[0].contains("Multiple *.project.json files found"),
			"warnings = {warnings:?}"
		);
	}
	// none
	{
		let project = write_project(&[]);
		let (got, warnings) = find_rojo_config_file_path(&project.dir);
		assert!(
			got.is_none() && warnings.is_empty(),
			"path = {got:?}, warnings = {warnings:?}"
		);
	}
}

// --- config warnings ---

#[test]
fn invalid_config_warnings() {
	// missing file
	{
		let project = write_project(&[]);
		let r = RojoResolver::from_path(&project.path("default.project.json"));
		let w = r.warnings();
		assert!(w.len() == 1 && w[0].contains("Path does not exist"), "warnings = {w:?}");
	}
	// malformed json
	{
		let project = write_project(&[("default.project.json", "{not json")]);
		let r = RojoResolver::from_path(&project.path("default.project.json"));
		let w = r.warnings();
		assert!(
			w.len() == 1 && w[0].contains("Invalid configuration"),
			"warnings = {w:?}"
		);
	}
	// schema violations
	{
		let bad = [
			r#"{"tree":{}}"#,                             // missing name
			r#"{"name":"p"}"#,                            // missing tree
			r#"{"name":5,"tree":{}}"#,                    // name not string
			r#"{"name":"p","tree":[]}"#,                  // tree not object
			r#"{"name":"p","tree":{"$className":5}}"#,    // $className not string
			r#"{"name":"p","tree":{"$path":5}}"#,         // $path neither string nor object
			r#"{"name":"p","tree":{"child":"x"}}"#,       // child not a tree
			r#"{"name":"p","servePort":"80","tree":{}}"#, // servePort not integer
		];
		for src in bad {
			let project = write_project(&[("default.project.json", src)]);
			let r = RojoResolver::from_path(&project.path("default.project.json"));
			let w = r.warnings();
			assert!(
				w.len() == 1 && w[0].contains("Invalid configuration"),
				"config {src}: warnings = {w:?}"
			);
		}
	}
	// optional path object form
	{
		let project = write_project(&[(
			"default.project.json",
			r#"{"name":"p","tree":{"opt":{"$path":{"optional":"maybe"}}}}"#,
		)]);
		let r = RojoResolver::from_path(&project.path("default.project.json"));
		assert!(r.warnings().is_empty(), "warnings = {:?}", r.warnings());
		// Nonexistent optional paths still create a partition (parsePath does
		// not require existence).
		let got = must_rbx_path(&r, &project.path("maybe/x.luau"));
		assert_eq!(got, rbx(&["opt", "x"]), "optional path");
	}
}

// --- FromTree ---

#[test]
fn from_tree() {
	let project = write_project(&[("src/", ""), ("lib/", "")]);
	let src_path = gopath::join(&[&project.dir, "src"]);
	let lib_path = gopath::join(&[&project.dir, "lib"]);
	let tree = Tree {
		children: vec![TreeEntry {
			name: "Game".to_owned(),
			tree: Tree {
				class_name: "DataModel".to_owned(),
				children: vec![TreeEntry {
					name: "ReplicatedStorage".to_owned(),
					tree: Tree {
						children: vec![
							TreeEntry {
								name: "src".to_owned(),
								tree: Tree {
									path: Some(src_path.clone()),
									..Tree::default()
								},
							},
							TreeEntry {
								name: "lib".to_owned(),
								tree: Tree {
									path: Some(lib_path),
									..Tree::default()
								},
							},
						],
						..Tree::default()
					},
				}],
				..Tree::default()
			},
		}],
		..Tree::default()
	};
	let r = RojoResolver::from_tree(&project.dir, &tree);
	assert!(r.is_game, "DataModel in tree must set is_game");
	let got = must_rbx_path(&r, &gopath::join(&[&src_path, "x.luau"]));
	assert_eq!(got, rbx(&["Game", "ReplicatedStorage", "src", "x"]), "from_tree path");
}

// --- Go's nil rbx path (resolver.go L432: `slices.Clone` of a never-pushed path) ---

#[test]
fn tree_root_module_path_is_nil() {
	let project = write_project(&[
		("default.project.json", r#"{"name":"p","tree":{"$path":"init.lua"}}"#),
		("init.lua", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	assert_eq!(
		r.lookup_rbx_path(&project.path("init.luau")),
		RbxPathLookup::Nil,
		"a tree-root module $path stores Go's nil path"
	);
	assert_eq!(
		r.get_rbx_path_from_file_path(&project.path("init.luau")),
		Some(Vec::new()),
		"the nil path still reports ok"
	);
	let state = r.state();
	assert_eq!(state.file_paths.len(), 1, "one exact mapping");
	assert_eq!(state.file_paths[0].rbx_path, None, "the snapshot keeps the nil path");
}

#[test]
fn pushed_module_paths_are_not_nil() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"Child":{"$path":"child.lua"},"Nested":{"$path":"nested"}}}"#,
		),
		("child.lua", ""),
		(
			"nested/default.project.json",
			r#"{"name":"n","tree":{"$path":"init.lua"}}"#,
		),
		("nested/init.lua", ""),
	]);
	let r = RojoResolver::from_path(&project.path("default.project.json"));
	assert_eq!(
		r.lookup_rbx_path(&project.path("child.luau")),
		RbxPathLookup::Found(rbx(&["Child"])),
		"a pushed mapping is found"
	);
	assert_eq!(
		r.lookup_rbx_path(&project.path("nested/init.luau")),
		RbxPathLookup::Found(rbx(&["Nested"])),
		"a nested project mounted under a pushed name is found"
	);
}

#[test]
fn package_root_module_path_is_nil() {
	// A type root that is itself a project mounts it without a push.
	let project = write_project(&[
		(
			"@scope/default.project.json",
			r#"{"name":"pkg","tree":{"$path":"init.lua"}}"#,
		),
		("@scope/init.lua", ""),
	]);
	let r = RojoResolver::package_imports(&project.path("@scope"), &ProjectFiles::new());
	assert_eq!(
		r.lookup_rbx_path(&project.path("@scope/init.luau")),
		RbxPathLookup::Nil,
		"the package resolver stores Go's nil path"
	);
}
