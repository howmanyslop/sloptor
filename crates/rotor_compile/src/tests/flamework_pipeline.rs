//! `flamework_pipeline_test.go`

use std::sync::{Arc, Mutex};

use super::sidecar::{SessionsGuard, build, repo_sidecar_dir, set_repo_sidecar_path, write_sidecar_plugin_fixture};
use super::sidecar_declaration::sidecar_declaration_config;
use super::sidecar_positions::{POSITION_FIXTURE_SOURCE, assert_locates_use_mount_effect};
use super::task7_acceptance::task7_flamework_project;
use super::{Temp, large_stack, write_project};
use crate::flamework_config::LEGACY_FLAMEWORK_TRANSFORMER;
use crate::flamework_pipeline::{FlameworkPipeline, run_compile_pipeline_with_options};
use crate::project::{ProjectOptions, new_project_program_with_options};
use crate::source_trace::{SourcePosition, SourceTraceMap, compose_source_trace_maps};
use crate::timings::BuildTimings;
use crate::transformer_order::{TransformerPluginConfig, effective_transformer_plugins, split_transformer_plugins};

fn plugin(transform: &str) -> TransformerPluginConfig {
	TransformerPluginConfig {
		transform: transform.to_owned(),
		raw: String::new(),
	}
}

/// `transformerPluginNames`
fn transformer_plugin_names(plugins: &[TransformerPluginConfig]) -> Vec<&str> {
	plugins.iter().map(|plugin| plugin.transform.as_str()).collect()
}

/// `writePipelineConfig`
fn write_pipeline_config(temp: &Temp, name: &str, text: &str) -> String {
	temp.write(name, text);
	temp.join(name).to_string_lossy().into_owned()
}

#[test]
fn effective_transformer_plugins_resolves_complete_extends_chain() {
	// Given: the child inherits plugins through two config levels.
	let temp = Temp::new();
	write_pipeline_config(
		&temp,
		"base.json",
		r#"{"compilerOptions":{"plugins":[{"transform":"prefix"},{"transform":"suffix"}]}}"#,
	);
	write_pipeline_config(
		&temp,
		"middle.json",
		r#"{"extends":"./base.json","compilerOptions":{"strict":true}}"#,
	);
	let child = write_pipeline_config(
		&temp,
		"tsconfig.json",
		r#"{"extends":"./middle.json","include":["src"]}"#,
	);

	// When: the effective transformer list is resolved.
	let plugins = effective_transformer_plugins(&child).expect("effective plugins");
	// Then: the inherited declaration order is preserved.
	assert_eq!(
		transformer_plugin_names(&plugins),
		["prefix", "suffix"],
		"effective plugins"
	);
}

#[test]
fn split_transformer_plugins_defaults_native_first() {
	let plugins = [plugin("first"), plugin("second")];
	let (prefix, suffix) = split_transformer_plugins(&plugins, "").expect("split");
	assert!(
		prefix.is_empty() && transformer_plugin_names(&suffix) == ["first", "second"],
		"split = ({:?}, {:?}), want native then all external plugins",
		transformer_plugin_names(&prefix),
		transformer_plugin_names(&suffix)
	);
}

#[test]
fn split_transformer_plugins_anchors_after_unique_effective_plugin() {
	let plugins = [plugin("prefix"), plugin("anchor"), plugin("suffix")];
	let (prefix, suffix) = split_transformer_plugins(&plugins, "anchor").expect("split");
	assert!(
		transformer_plugin_names(&prefix) == ["prefix", "anchor"] && transformer_plugin_names(&suffix) == ["suffix"],
		"split = ({:?}, {:?}), want ([prefix anchor], [suffix])",
		transformer_plugin_names(&prefix),
		transformer_plugin_names(&suffix)
	);
}

#[test]
fn split_transformer_plugins_rejects_missing_duplicate_and_self_anchors() {
	let tests: [(&str, Vec<TransformerPluginConfig>, &str, &str); 3] = [
		("missing", vec![plugin("present")], "missing", "does not match"),
		("duplicate", vec![plugin("same"), plugin("same")], "same", "matches 2"),
		("self", Vec::new(), LEGACY_FLAMEWORK_TRANSFORMER, "cannot anchor"),
	];
	for (name, plugins, after, want) in tests {
		let result = split_transformer_plugins(&plugins, after);
		assert!(
			result.as_ref().is_err_and(|error| error.contains(want)),
			"{name}: splitTransformerPlugins() error = {:?}, want containing {want:?}",
			result.err()
		);
	}
}

#[test]
fn compose_source_trace_maps_maps_across_every_stage() {
	// Given: suffix -> native -> prefix -> authored mappings.
	let authored = Arc::new(SourceTraceMap::for_test("main.ts", "authored", &[(3, 0, 7, 2)]));
	let native = Arc::new(SourceTraceMap::for_test("main.ts", "prefix", &[(2, 0, 3, 0)]));
	let suffix = Arc::new(SourceTraceMap::for_test("main.ts", "native", &[(1, 0, 2, 0)]));

	let composed = compose_source_trace_maps(Some(suffix), compose_source_trace_maps(Some(native), Some(authored)))
		.expect("a composed trace");
	let position = composed.original_position(SourcePosition { line: 1, column: 0 });

	assert!(
		position.is_some_and(|position| position.line == 7 && position.column == 2),
		"composed original = {position:?}, want 7:2"
	);
	assert_eq!(composed.original_source_text(), "authored", "composed source text");
}

/// `flameworkStagePlugin`
const FLAMEWORK_STAGE_PLUGIN: &str = r#"const ts = require("typescript");
module.exports = function (_program, config) {
	const source = (context) => (sourceFile) => {
		let nativeMetadataPresent = false;
		function inspect(node) {
			if (ts.isStringLiteral(node) && node.text === "identifier") nativeMetadataPresent = true;
			ts.forEachChild(node, inspect);
		}
		inspect(sourceFile);
		const value = config.phase === "suffix" ? (nativeMetadataPresent ? "suffix-after-native" : "suffix-before-native") : "prefix-stage";
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__" + config.phase.toUpperCase() + "_STAGE__", undefined, undefined, context.factory.createStringLiteral(value)),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, config.phase === "prefix" ? [marker, ...sourceFile.statements] : [...sourceFile.statements, marker]);
	};
	const declaration = (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__" + config.phase.toUpperCase() + "_DECLARATION__", undefined, undefined, context.factory.createStringLiteral(config.phase + "-declaration")),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, [...sourceFile.statements, marker]);
	};
	return { before: source, afterDeclarations: declaration };
};
"#;

/// `symlinkTransformerFixtureModules`
pub fn symlink_transformer_fixture_modules(temp: &Temp) {
	let fixture = std::path::Path::new(&repo_sidecar_dir()).join("../../testdata/transformers/project/node_modules");
	#[cfg(unix)]
	std::os::unix::fs::symlink(fixture, temp.join("node_modules")).expect("symlink");
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(fixture, temp.join("node_modules")).expect("symlink");
}

#[test]
fn flamework_pipeline_uses_configured_after_on_real_build_path() {
	// Given: native Flamework is enabled and anchored after a real external transform.
	let temp = write_project("flamework-pipeline-real-build", "");
	let dir = temp.dir();
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	symlink_transformer_fixture_modules(&temp);
	temp.write(
		"default.project.json",
		r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include","node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"},"@flamework":{"$path":"node_modules/@flamework"}}}}}}"#,
	);
	let plugins = r#"[{"transform":"./plugins/prefix.js","phase":"prefix"},{"transform":"./plugins/suffix.js","phase":"suffix"}]"#;
	let config_text = sidecar_declaration_config(plugins).replacen(
		r#""declaration": true,"#,
		r#""declaration": true, "experimentalDecorators": true,"#,
		1,
	);
	let config_text = config_text.replacen(
		r#""typeRoots": ["node_modules/@rbxts"]"#,
		r#""typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"]"#,
		1,
	);
	write_sidecar_plugin_fixture(&dir, "", &config_text);
	temp.write("rotor.toml", "[flamework]\nafter = \"./plugins/prefix.js\"\n");
	for name in ["prefix.js", "suffix.js"] {
		temp.write(&format!("plugins/{name}"), FLAMEWORK_STAGE_PLUGIN);
	}
	temp.write(
		"src/main.ts",
		"import { Service } from \"@flamework/core\";\n@Service()\nexport class TestService {}\n",
	);

	// When: the actual build surface runs.
	let timings = Arc::new(BuildTimings::new());
	let opts = ProjectOptions {
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	let (result, diags, error) = build(&dir, &opts);
	// Then: both the prefix plugin and native stage reached emitted Luau.
	assert!(error.is_none(), "BuildProjectWithOptions: {error:?} (diags: {diags:?})");
	let counts = timings.report().counts;
	assert_ne!(
		counts.sidecar_stats, 0,
		"sidecar stats = 0, want a disk snapshot on the first stage"
	);
	assert!(
		counts.total_sources == 0 || counts.sidecar_source_reads <= 2 * counts.total_sources + 8,
		"sidecar source reads = {} for {} sources, want at most one compile-file pass plus declaration reverts",
		counts.sidecar_source_reads,
		counts.total_sources
	);
	let result = result.expect("a result");
	let output = super::sidecar::output_text(&result, "out/main.luau");
	let prefix = output.find("prefix-stage");
	let native = output.find("defineMetadata");
	let suffix = output.find("suffix-after-native");
	assert!(
		prefix.is_some() && native > prefix && suffix > native,
		"stage order = prefix:{prefix:?} native:{native:?} suffix:{suffix:?}, want strict order:\n{output}"
	);
	assert_eq!(
		output.matches(r#""identifier""#).count(),
		1,
		"native Flamework identifier metadata count:\n{output}"
	);
	let declaration = std::fs::read_to_string(temp.join("out/main.d.ts")).expect("declaration");
	// Declarations are emitted natively from the untransformed program, so neither the plugins'
	// afterDeclarations markers nor their source-stage markers may appear in the published types.
	for marker in [
		"prefix-declaration",
		"suffix-declaration",
		"__PREFIX_STAGE__",
		"__SUFFIX_STAGE__",
	] {
		assert!(
			!declaration.contains(marker),
			"transformer output {marker:?} leaked into the declaration:\n{declaration}"
		);
	}
	assert!(
		declaration.contains("export declare class TestService"),
		"declaration does not describe the authored source:\n{declaration}"
	);
}

#[test]
fn flamework_pipeline_composes_diagnostic_trace_across_every_stage() {
	// Given: prefix, native, and suffix stages each reprint the source before a TypeScript
	// diagnostic is reported.
	let temp = write_project("@scope/flamework-trace", "");
	let dir = temp.dir();
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	symlink_transformer_fixture_modules(&temp);
	let plugins = r#"[{"transform":"./plugins/prefix.js","phase":"prefix"},{"transform":"./plugins/suffix.js","phase":"suffix"}]"#;
	write_sidecar_plugin_fixture(&dir, "", &sidecar_declaration_config(plugins));
	temp.write("rotor.toml", "[flamework]\nafter = \"./plugins/prefix.js\"\n");
	for name in ["prefix.js", "suffix.js"] {
		temp.write(&format!("plugins/{name}"), FLAMEWORK_STAGE_PLUGIN);
	}
	temp.write("src/main.ts", POSITION_FIXTURE_SOURCE);

	// When: the real build reaches its post-pipeline TypeScript diagnostics.
	let (result, _, error) = build(&dir, &ProjectOptions::default());

	// Then: TS2304 still indexes the authored token on disk.
	let Some(result) = result.filter(|_| error.is_some()) else {
		panic!("BuildProjectWithOptions() = ({error:?}), want located diagnostic failure");
	};
	let diagnostic = result
		.diagnostics
		.iter()
		.find(|diagnostic| diagnostic.code == "TS2304")
		.unwrap_or_else(|| panic!("TS2304 missing from diagnostics: {:?}", result.diagnostics));
	assert_locates_use_mount_effect(diagnostic);
}

#[test]
fn prepare_flamework_pipeline_accepts_root_dirs_when_root_dir_is_null() {
	// Given: native Flamework is enabled and tsconfig uses rootDirs with rootDir: null (the
	// test-project pattern: sources live under both src/ and test/).
	let temp = task7_flamework_project("[flamework]\n");
	temp.write("test/spec.ts", "export const spec = 1;\n");
	rewrite_flamework_root_dirs(&temp);

	// When: the real compile pipeline opens native Flamework.
	let (result, diagnostics, error) = build(&temp.dir(), &ProjectOptions::default());

	// Then: rootDirs stands in for rootDir; OpenProject does not assert.
	assert!(
		error.is_none() && diagnostics.is_empty(),
		"BuildProjectWithOptions = ({diagnostics:?}, {error:?})"
	);
	let result = result.expect("a result");
	let outputs: Vec<&String> = result.outputs.keys().collect();
	assert!(
		result.outputs.contains_key("out/src/main.luau"),
		"outputs = {outputs:?}, want out/src/main.luau"
	);
	assert!(
		result.outputs.contains_key("out/test/spec.luau"),
		"outputs = {outputs:?}, want out/test/spec.luau from rootDirs test/"
	);
}

/// `rewriteFlameworkRootDirs`
fn rewrite_flamework_root_dirs(temp: &Temp) {
	let data = std::fs::read_to_string(temp.join("tsconfig.json")).expect("read tsconfig");
	let text = data.replacen(
		r#""rootDir": "src""#,
		r#""rootDir": null, "rootDirs": ["src", "test"]"#,
		1,
	);
	let text = text.replacen(r#""include": ["src"]"#, r#""include": ["src", "test"]"#, 1);
	assert_ne!(
		text, data,
		"rewriteFlameworkRootDirs: tsconfig did not contain expected rootDir/include"
	);
	temp.write("tsconfig.json", text);
}

/// An incremental build that selected nothing has nothing to transform and nothing to emit, so
/// the pipeline must not reach the worker at all: the worker builds its whole `LanguageService`
/// program before it can discover that the compile list is empty. The plugin named here does not
/// exist, so any round trip fails loudly.
#[test]
fn run_compile_pipeline_skips_the_worker_when_nothing_is_selected() {
	let temp = write_project("@scope/empty-selection", "");
	let dir = temp.dir();
	large_stack(|| {
		let (project_dir, program) = new_project_program_with_options(&dir, "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {} (diags: {:?})", failure.error, failure.messages()));
		let missing = vec![plugin("rotor-transformer-that-does-not-exist")];
		// Go's pipeline has no project; the port's needs one, which nothing below reads.
		let project = rotor_flamework::open_project(&rotor_flamework::ProjectOptions {
			project_dir: project_dir.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			..rotor_flamework::ProjectOptions::default()
		})
		.expect("open project");
		let pipeline = FlameworkPipeline {
			config: rotor_config::config::FlameworkConfig::default(),
			project: Arc::new(Mutex::new(project)),
			program,
			plugins: missing.clone(),
			prefix: missing.clone(),
			suffix: missing,
		};

		let result = run_compile_pipeline_with_options(
			&project_dir,
			program,
			&[],
			&std::collections::BTreeMap::new(),
			Some(&pipeline),
			&ProjectOptions::default(),
		)
		.unwrap_or_else(|failure| {
			panic!(
				"runCompilePipeline: {} (diags: {:?})",
				failure.error,
				failure.messages()
			)
		});
		assert!(
			result.prepared.source_files.is_empty(),
			"prepared {} source files, want none",
			result.prepared.source_files.len()
		);
		assert_eq!(
			result.prepared.flamework.as_ref(),
			Some(&pipeline.config),
			"the empty pipeline dropped the Flamework config the rest of the build reads"
		);
	});
}
