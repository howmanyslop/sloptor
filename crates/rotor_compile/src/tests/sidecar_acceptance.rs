//! The 5b halves of `sourcemap_plugin_acceptance_test.go`,
//! `sourcemap_performance_acceptance_test.go` and `react_syntax_origin_test.go`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::sidecar::{SessionsGuard, build, set_repo_sidecar_path};
use super::sourcemap::{annotate_source_map_segments, assert_source_map_contract};
use super::{Temp, copy_dir, large_stack, repo_root, write_project};
use crate::project::{ProjectOptions, new_project_program_with_options, project_source_files};
use crate::sidecar::{close_sidecar_sessions, set_test_env};
use crate::source_trace::SourceTraceMap;
use crate::source_trace_sourcemap::rewrite_source_map_with_trace;
use crate::transformer_host::{TRANSFORMER_ORIGINAL_AUTHORED, TransformerCall, TransformerHost, TransformerResponse};
use crate::transformer_host_sidecar::{CallbackTransformerHost, SidecarTransformerHost};

/// `annotateSourceMap`
fn annotate_source_map(luau: &str, map_json: &str) -> String {
	annotate_source_map_segments(luau, map_json, false)
}

#[test]
fn source_map_trace_retains_generated_only_lines() {
	// Three Luau lines initially refer to three reprinted TS lines. The transform has no
	// authored counterpart before its first trace anchor.
	let trace = SourceTraceMap::new(
		r#"{"version":3,"mappings":";AAAA;AACA"}"#,
		"/authored.ts",
		"first();\nlast();\n",
	)
	.unwrap_or_else(|error| panic!("{error}"));
	let raw = r#"{"version":3,"file":"main.luau","sources":["/printed.ts"],"sourcesContent":["injected();\nfirst();\nlast();\n"],"mappings":"AAAA;AACA;AACA"}"#;
	let rewritten = rewrite_source_map_with_trace(raw, &trace).unwrap_or_else(|error| panic!("{error}"));
	let want = " 1 0->?   | injected()\n 2 0->1:0 | first()\n 3 0->2:0 | last()\n";
	assert_eq!(
		annotate_source_map("injected()\nfirst()\nlast()\n", &rewritten),
		want,
		"trace rewrite"
	);
	assert_source_map_contract("injected()\nfirst()\nlast()\n", &rewritten, "first();\nlast();\n");
}

#[test]
fn source_map_trace_deduplicates_composed_positions() {
	let trace = SourceTraceMap::new(
		r#"{"version":3,"mappings":"AAAA;AACA"}"#,
		"/authored.ts",
		"first();\nlast();\n",
	)
	.unwrap_or_else(|error| panic!("{error}"));
	let raw = r#"{"version":3,"file":"main.luau","sources":["/printed.ts"],"sourcesContent":["first();\nlast();\n"],"mappings":"AAAA,KAAK,KAAK;AACV"}"#;
	let rewritten = rewrite_source_map_with_trace(raw, &trace).unwrap_or_else(|error| panic!("{error}"));
	let want = " 1 0->1:0 | first()\n 2 0->2:0 | last()\n";
	assert_eq!(
		annotate_source_map("first()\nlast()\n", &rewritten),
		want,
		"composed source map"
	);
}

/// `stagePerformanceOutputFixture`: the fork-parity performance fixture with its globals, and
/// its golden tree.
fn stage_performance_output_fixture() -> Temp {
	let root = repo_root().join("testdata/forkparity/project/performance-output");
	let fixture = Temp::new();
	copy_dir(&root.join("tree"), Path::new(&fixture.dir()));
	copy_dir(
		&root.join("support/globals"),
		&fixture.join("node_modules/@rbxts/globals"),
	);
	let stamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_hours(262_968);
	std::fs::File::open(fixture.join("src"))
		.and_then(|dir| dir.set_modified(stamp))
		.expect("chtimes");
	fixture
}

#[test]
fn source_map_performance_goldens() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let fixture = stage_performance_output_fixture();
	let golden = repo_root().join("testdata/forkparity/project/performance-output/golden/tree");
	let (_, diags, error) = build(&fixture.dir(), &ProjectOptions::default());
	assert!(
		error.is_none() && diags.is_empty(),
		"build: {error:?} (diagnostics: {diags:?})"
	);
	for (name, want) in [
		(
			"alpha",
			" 1 -                | -- Compiled with sloptor vX\n 2 0->3:0 14->3:27  | local alpha = {\n 3 0->3:29 9->3:36  | \tlabel = \"alpha\",\n 4 0->3:45 10->3:53 | \tweight = 1,\n 5 0->3:27          | }\n 6 -                | return {\n 7 0->3:0           | \talpha = alpha,\n 8 -                | }\n",
		),
		(
			"beta",
			" 1 -                | -- Compiled with sloptor vX\n 2 0->6:0 13->6:26  | local beta = {\n 3 0->6:28 9->6:35  | \tlabel = \"beta\",\n 4 0->6:43 10->6:51 | \tweight = 2,\n 5 0->6:26          | }\n 6 -                | return {\n 7 0->6:0           | \tbeta = beta,\n 8 -                | }\n",
		),
	] {
		let path = format!("out/{name}.luau");
		let read = |relative: &str| std::fs::read_to_string(fixture.join(relative)).expect("read");
		let (text, map_json) = (read(&path), read(&format!("{path}.map")));
		assert_eq!(
			text.as_bytes(),
			std::fs::read(golden.join(&path)).expect("golden"),
			"{name}: performance fixture Luau bytes changed"
		);
		// TypeScript's source reader strips a UTF-8 BOM before parsing; sourcesContent retains
		// that decoded text, including its CRLFs.
		let source_text = read(&format!("src/{name}.ts"));
		let source = source_text.strip_prefix('\u{feff}').unwrap_or(&source_text);
		assert_source_map_contract(&text, &map_json, source);
		assert_eq!(annotate_source_map(&text, &map_json), want, "{name}: performance map");
		assert_eq!(
			map_json.as_bytes(),
			std::fs::read(golden.join(format!("{path}.map"))).expect("golden"),
			"{name}: stale performance map golden"
		);
	}
}

#[test]
fn react_syntax_origin_survives_both_transports() {
	let temp = write_project("@scope/react-origin", "");
	let worker = Temp::new();
	// Observe the actual stdio request after argument expansion/reconstruction.
	worker.write(
		"main.js",
		"const fs = require(\"node:fs\");\nconst path = require(\"node:path\");\nrequire(\"node:readline\").createInterface({input:process.stdin}).on(\"line\", line => {\n  fs.writeFileSync(path.join(__dirname, \"request.json\"), line);\n  process.stdout.write(JSON.stringify({diagnostics:[],transformed:[]}) + \"\\n\");\n});",
	);
	set_test_env("ROTOR_SIDECAR_PATH", &worker.dir());
	close_sidecar_sessions();
	let _sessions = SessionsGuard::new();
	let callback_request = Arc::new(Mutex::new(String::new()));
	let recorded = Arc::clone(&callback_request);
	let callback = CallbackTransformerHost::new(Arc::new(move |_, request| {
		*crate::project::lock(&recorded) = request.to_json();
		Ok(TransformerResponse::default())
	}));
	large_stack(|| {
		let (project_dir, program) = new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {}", failure.error));
		let files = project_source_files(program);
		let mut call = TransformerCall {
			dir: project_dir,
			config_path: program.options().config_file_path.clone(),
			compile_files: files.clone(),
			stamp_files: files,
			..TransformerCall::default()
		};
		for transport in ["callback", "stdio"] {
			let host: &dyn TransformerHost = if transport == "stdio" {
				&SidecarTransformerHost
			} else {
				&callback
			};
			let read_request = || -> String {
				if transport == "callback" {
					crate::project::lock(&callback_request).clone()
				} else {
					std::fs::read_to_string(worker.join("request.json")).expect("the recorded request")
				}
			};
			for (enabled, origin, want) in [
				("0", TRANSFORMER_ORIGINAL_AUTHORED, ""),
				("1", "", ""),
				("1", "unknown", ""),
				("1", TRANSFORMER_ORIGINAL_AUTHORED, TRANSFORMER_ORIGINAL_AUTHORED),
			] {
				set_test_env("ROTOR_REACT_SYNTAX_HOST", enabled);
				origin.clone_into(&mut call.source_origin);
				if let Err(error) = host.transform(None, call.clone()) {
					panic!("{transport} transform: {error}");
				}
				let wire: serde_json::Value = serde_json::from_str(&read_request()).expect("JSON");
				let value = wire.get("sourceOrigin");
				assert!(
					value.is_some() != want.is_empty() && value.is_none_or(|value| value == want),
					"{transport} enabled={enabled} origin={origin:?}: sourceOrigin={value:?}"
				);
			}
			if let Err(error) = host.validate(None, &call.dir, &call.config_path) {
				panic!("{error}");
			}
			assert!(
				!read_request().contains(r#""sourceOrigin""#),
				"{transport} validation carried authored provenance"
			);
		}
	});
}
