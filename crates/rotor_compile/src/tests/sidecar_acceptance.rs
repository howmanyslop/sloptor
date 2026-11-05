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
	super::set_directory_modified(&fixture.join("src"), stamp).expect("chtimes");
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

#[test]
fn source_map_flamework_nested_callbacks() {
	// Use the real legacy plugin project, copied to a temporary directory with its installed
	// dependencies linked, just as the plugin integration suite does.
	let (temp, _sessions) = super::transformers_integration::legacy_flamework_fixture();
	let config_path = temp.join("tsconfig.json");
	let config = std::fs::read_to_string(&config_path).expect("read tsconfig");
	let with_maps = config.replacen(r#""outDir": "out""#, r#""outDir": "out", "sourceMap": true"#, 1);
	std::fs::write(&config_path, with_maps).expect("write tsconfig");
	let source = r#"import { OnStart, Service } from "@flamework/core";

@Service()
export class TestService implements OnStart {
	public onStart(): void {
		const callback = () => {
			print("outer-callback");
			const inner = () => {
				print("inner-callback");
			};
			inner();
		};
		callback();
	}
}
"#;
	let relative = "server/services/test.service";
	temp.write(&format!("src/{relative}.ts"), source);
	let (result, diags, error) = build(&temp.dir(), &ProjectOptions::default());
	assert!(
		error.is_none() && diags.is_empty(),
		"plugin build: {error:?} (diagnostics: {diags:?})"
	);
	let text = super::sidecar::output_text(&result.expect("a result"), &format!("out/{relative}.luau"));
	assert!(
		text.contains("defineMetadata") && text.contains("identifier"),
		"real Flamework plugin did not inject metadata:\n{text}"
	);
	let map = std::fs::read_to_string(temp.join(&format!("out/{relative}.luau.map"))).expect("read map");
	assert_source_map_contract(&text, &map, source);
	let mappings = super::sourcemap::decode_source_map_lines(&map);
	// Pin the whole nested-callback group, including both closing keywords. The expected authored
	// lines come from the literal source above, before Flamework inserts its imports, decorators
	// and metadata and reprints it.
	let want: [(&str, u32); 8] = [
		("local callback = function()", 6),
		(r#"print("outer-callback")"#, 7),
		("local inner = function()", 8),
		(r#"print("inner-callback")"#, 9),
		("end", 10),
		("inner()", 11),
		("end", 12),
		("callback()", 13),
	];
	let lines: Vec<&str> = text.split('\n').collect();
	let start = lines.iter().position(|content| content.trim() == want[0].0);
	let start = match start {
		Some(start) if start + want.len() <= lines.len() => start,
		_ => panic!("nested callback group missing:\n{text}"),
	};
	let mut errors = Vec::new();
	for (offset, (expected, expected_line)) in want.iter().enumerate() {
		let line = start + offset;
		let got = lines[line].trim();
		if got != *expected {
			errors.push(format!("callback line {} = {got:?}, want {expected:?}", offset + 1));
		}
		let segments = u32::try_from(line)
			.ok()
			.and_then(|line| mappings.get(&line))
			.map_or(&[][..], Vec::as_slice);
		let Some(first) = segments.first().filter(|first| first.generated_character == 0) else {
			errors.push(format!("callback Luau line {} has no column-zero segment", line + 1));
			continue;
		};
		if !first.is_source_mapping() || first.source_line.map(|source| source + 1) != Some(*expected_line) {
			errors.push(format!(
				"callback Luau line {} maps to {first:?}, want authored TS line {expected_line}",
				line + 1
			));
		}
	}
	assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
#[expect(
	clippy::significant_drop_tightening,
	reason = "the lock guards what the assertions read"
)]
fn react_syntax_origin_initial_prefix_suffix_and_next_build() {
	use crate::flamework_pipeline::prepare_compile_pipeline;
	use crate::sidecar::{prepare_transformer_program_with_host, transformer_host_for};
	use crate::transformer_host::{TransformerRequest, with_transformer_callback};

	set_test_env("ROTOR_REACT_SYNTAX_HOST", "1");
	let temp = write_project("@scope/react-stages", "");
	let config_path = temp.join("tsconfig.json");
	let config = std::fs::read_to_string(&config_path).expect("read tsconfig");
	let config = config.replacen(
		r#""types": []"#,
		r#""types": [], "plugins": [{"transform":"prefix"},{"transform":"suffix"}]"#,
		1,
	);
	std::fs::write(&config_path, config).expect("write tsconfig");
	let main_path = temp.join("src/main.ts").to_string_lossy().into_owned();
	let authored = "export const value=1;\n";
	std::fs::write(&main_path, authored).expect("write main");
	let requests: Arc<Mutex<Vec<TransformerRequest>>> = Arc::default();
	let recorded = Arc::clone(&requests);
	let mut opts = ProjectOptions::default();
	with_transformer_callback(
		&mut opts,
		Arc::new(move |_, request| {
			crate::project::lock(&recorded).push(request);
			Ok(TransformerResponse::default())
		}),
	);
	large_stack(|| {
		let (project_dir, program) = new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {} ({:?})", failure.error, failure.messages()));
		let files = project_source_files(program);
		if let Err(failure) = prepare_transformer_program_with_host(
			None,
			transformer_host_for(&opts),
			&project_dir,
			program,
			&files,
			&std::collections::BTreeMap::new(),
		) {
			panic!("initial stage: {} ({:?})", failure.error, failure.messages());
		}
		{
			let requests = crate::project::lock(&requests);
			assert!(
				requests.len() == 1 && requests[0].source_origin == TRANSFORMER_ORIGINAL_AUTHORED,
				"initial request provenance: {requests:?}"
			);
		}
		crate::project::lock(&requests).clear();
		temp.write("rotor.toml", "[flamework]\nafter = \"prefix\"\n");
		// Native Flamework reprints the source between prefix and suffix. The next build must
		// send disk text to revert that session overlay before admission.
		for build in 0..2 {
			if let Err(failure) =
				prepare_compile_pipeline(&project_dir, program, &files, &std::collections::BTreeMap::new(), &opts)
			{
				panic!("pipeline build {build}: {} ({:?})", failure.error, failure.messages());
			}
		}
	});
	let requests = crate::project::lock(&requests);
	assert_eq!(requests.len(), 4, "stage requests");
	for (index, request) in requests.iter().enumerate() {
		let want = if index % 2 == 0 {
			TRANSFORMER_ORIGINAL_AUTHORED
		} else {
			""
		};
		assert_eq!(request.source_origin, want, "stage {index} origin");
	}
	let text_for = |request: &TransformerRequest| -> String {
		request
			.changed_files
			.iter()
			.flatten()
			.find(|file| Path::new(&file.file_name) == Path::new(&main_path))
			.map(|file| file.text.clone())
			.unwrap_or_default()
	};
	let generated = text_for(&requests[1]);
	assert!(
		!generated.is_empty() && generated != authored,
		"suffix did not receive native-generated text: {generated:?}"
	);
	let got = text_for(&requests[2]);
	assert_eq!(got, authored, "next authored prefix retained suffix overlay");
}
