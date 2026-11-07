//! `flamework_discovery_lookup_test.go`

use std::sync::Arc;

use rotor_flamework::{BuildClass, BuildDecorator, BuildInfo, FLAMEWORK_VERSION};

use super::Temp;
use super::incremental_flamework_build::file_hashes;
use super::sidecar::{build, output_text};
use super::solution::write_solution_file;
use super::solution_drain::must_read_file;
use super::solution_flamework_integration::{
	link_flamework_fixture_dependencies, write_native_flamework_solution_project,
};
use crate::project::ProjectOptions;
use crate::timings::BuildTimings;

fn decorator(name: &str, internal_id: &str) -> BuildDecorator {
	BuildDecorator {
		name: name.to_owned(),
		internal_id: internal_id.to_owned(),
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test")]
fn flamework_discovery_lookup_emitted_artifacts_and_recovery() {
	let temp = Temp::new();
	let dir = temp.dir();
	write_native_flamework_solution_project(&dir, "@scope/discovery-lookup");
	link_flamework_fixture_dependencies(&dir);
	let config_path = format!("{dir}/tsconfig.json");
	let config = String::from_utf8(must_read_file(&config_path)).expect("utf-8");
	let config = config.replacen(
		r#""outDir":"out""#,
		r#""outDir":"out","incremental":true,"tsBuildInfoFile":"out/lookup.tsbuildinfo""#,
		1,
	);
	write_solution_file(&dir, "tsconfig.json", &config);
	let good_source = "/** @metadata identifier */ export class Persisted {}\n";
	write_solution_file(&dir, "src/index.ts", good_source);
	write_solution_file(
		&dir,
		"flamework.build",
		r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{"@scope/discovery-lookup:out/init@Persisted":"b"},"classes":[{"internalId":"b","filePath":"first.lua","decorators":[{"name":"First","internalId":"first"}]},{"internalId":"a","filePath":"middle.lua"},{"internalId":"b","filePath":"third.lua","decorators":[{"name":"Third","internalId":"third"},{"name":"Extra","internalId":"extra"}]}]}"#,
	);
	let (result, diagnostics, error) = build(&dir, &ProjectOptions::default());
	assert!(
		error.is_none() && diagnostics.is_empty(),
		"persisted fallback build: {error:?} ({diagnostics:?})"
	);
	let output = output_text(&result.expect("a result"), "out/init.luau");
	// Captured from baseline production, including the absence of source decorator calls.
	let want_output = "-- Compiled with sloptor v3.0.0-beta.4\nlocal TS = _G[script]\n--* @metadata identifier \n"
		.to_owned()
		+ r#"local Reflect = TS.import(script, TS.getModule(script, "@flamework", "core").out).Reflect
local Persisted
do
	Persisted = setmetatable({}, {
		__tostring = function()
			return "Persisted"
		end,
	})
	Persisted.__index = Persisted
	function Persisted.new(...)
		local self = setmetatable({}, Persisted)
		return self:constructor(...) or self
	end
	function Persisted:constructor()
	end
	do
		-- (Flamework) Persisted metadata
		Reflect.defineMetadata(Persisted, "identifier", "b")
	end
end
return {
	Persisted = Persisted,
}
"#;
	assert_eq!(
		output, want_output,
		"persisted identifier or decorator provenance changed:\n{output}\nwant:\n{want_output}"
	);
	let declaration = String::from_utf8(must_read_file(&format!("{dir}/out/index.d.ts"))).expect("utf-8");
	assert_eq!(
		declaration, "/** @metadata identifier */ export declare class Persisted {\n}\n",
		"declaration"
	);
	let info = BuildInfo::load(&format!("{dir}/flamework.build"), FLAMEWORK_VERSION).expect("load build info");
	let want_classes = vec![
		BuildClass {
			internal_id: "b".to_owned(),
			file_path: "first.lua".to_owned(),
			decorators: vec![decorator("First", "first")],
		},
		BuildClass {
			internal_id: "a".to_owned(),
			file_path: "middle.lua".to_owned(),
			decorators: Vec::new(),
		},
		BuildClass {
			internal_id: "b".to_owned(),
			file_path: "third.lua".to_owned(),
			decorators: vec![decorator("Third", "third"), decorator("Extra", "extra")],
		},
	];
	let got = info.snapshot().classes;
	assert_eq!(
		got.as_ref(),
		Some(&want_classes),
		"fallback rewrote persisted class payload/order: {got:?}"
	);
	let mapping = info.identifier("@scope/discovery-lookup:out/init@Persisted");
	assert_eq!(mapping.as_deref(), Some("b"), "persisted mapping = {mapping:?}");
	let paths = [
		format!("{dir}/out/init.luau"),
		format!("{dir}/out/index.d.ts"),
		format!("{dir}/out/lookup.rbxtsc.tsbuildinfo"),
		format!("{dir}/flamework.build"),
	];
	let before = file_hashes(&paths);
	for (name, source, message) in [
		(
			"constructor",
			"export class Persisted<T extends string> { constructor(value: T extends string ? 1 : 2) {} }",
			r#"Could not find UID for type "T extends string ? 1 : 2""#,
		),
		(
			"guard",
			"/** @metadata identifier */ export class Persisted {\n/** @metadata flamework:guard */ value!: `prefix-${string}`;\n}\n",
			"Flamework encountered a template literal which is unsupported: `prefix-${string}`",
		),
	] {
		write_solution_file(&dir, "src/index.ts", source);
		let (_, diagnostics, error) = build(&dir, &ProjectOptions::default());
		let text = format!(
			"{}{}",
			error.as_ref().map(ToString::to_string).unwrap_or_default(),
			diagnostics.join("\n")
		)
		.to_lowercase();
		assert!(
			error.is_some() && text.contains(&message.to_lowercase()),
			"{name}: failure = {error:?} ({diagnostics:?}), want {message:?}"
		);
		assert_eq!(
			file_hashes(&paths),
			before,
			"{name}: failed native transform published output, declarations, build metadata or incremental state"
		);
		write_solution_file(&dir, "src/index.ts", good_source);
		let timings = Arc::new(BuildTimings::new());
		let opts = ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		};
		let (_, diagnostics, error) = build(&dir, &opts);
		assert!(
			error.is_none() && diagnostics.is_empty(),
			"{name}: recovery = {error:?} ({diagnostics:?})"
		);
		let counts = timings.report().counts;
		assert!(
			file_hashes(&paths) == before && counts.selected_sources == 0 && counts.actual_writes == 0,
			"{name}: recovery changed last-success state: selected={} writes={}",
			counts.selected_sources,
			counts.actual_writes
		);
	}
	assert!(
		must_read_file(&format!("{dir}/out/init.luau")) == output.as_bytes()
			&& must_read_file(&format!("{dir}/out/index.d.ts")) == declaration.as_bytes(),
		"recovery changed emitted artifacts"
	);
}
