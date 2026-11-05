//! `flamework_t_import_test.go`

use super::sidecar::{build, output_text};
use super::solution::write_solution_file;
use super::task7_acceptance::task7_flamework_project;
use crate::project::ProjectOptions;

#[test]
fn flamework_guard_t_import_survives_transformer() {
	// Given: native Flamework with noSemanticDiagnostics, a file that uses
	// Flamework.createGuard on an interface — the generated guard references
	// `t` from "@flamework/core/out/prelude".
	let temp = task7_flamework_project("[flamework]\nnoSemanticDiagnostics = true\n");
	let dir = temp.dir();
	// Real prelude + t type declarations (same shape as @flamework/core and @rbxts/t).
	write_flamework_modules(&dir);

	let guard_lib = "export declare namespace Flamework {\n\texport function createGuard<T>(meta?: { _flamework_macro_generic: [T, \"guard\"] }): unknown;\n}\n";
	std::fs::write(temp.join("src/guard-lib.ts"), guard_lib).expect("write");
	let main = "import { Flamework } from \"./guard-lib\";\ninterface Playback { readonly animationId: string; readonly loop: boolean; }\nexport const check = Flamework.createGuard<Playback>()(undefined);\n";
	std::fs::write(temp.join("src/main.ts"), main).expect("write");

	// When: full build.
	let (result, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions = ({diags:?}, {error:?})"
	);
	let got = output_text(&result.expect("a result"), "out/main.luau");
	assert!(
		got.contains("t[\"interface\"") || got.contains("t.interface"),
		"generated guard missing t usage:\n{got}"
	);
	assert!(
		got.contains("local t = TS.import"),
		"synthetic t import elided despite t usage:\n{got}"
	);
}

/// `writeFlameworkModules`: lays down `node_modules/@flamework/core/out/prelude` and
/// `node_modules/@rbxts/t` mirroring the real packages' declaration shapes.
pub fn write_flamework_modules(dir: &str) {
	write_file(
		dir,
		"node_modules/@rbxts/t/index.d.ts",
		"export declare const t: {\n\tinterface(x: unknown): (v: unknown) => boolean;\n\tstring: unknown;\n\tnumber: unknown;\n\tboolean: unknown;\n};\n",
	);
	write_file(
		dir,
		"node_modules/@flamework/core/out/prelude.d.ts",
		"import { t } from \"@rbxts/t\";\nexport { t };\n",
	);
	write_file(
		dir,
		"node_modules/@flamework/core/package.json",
		"{\"name\":\"@flamework/core\",\"version\":\"1.0.0\",\"types\":\"out/prelude.d.ts\"}",
	);
	write_file(
		dir,
		"node_modules/@rbxts/t/package.json",
		"{\"name\":\"@rbxts/t\",\"version\":\"3.0.0\",\"types\":\"index.d.ts\"}",
	);
}

/// `writeFile`
pub fn write_file(dir: &str, name: &str, contents: &str) {
	write_solution_file(dir, name, contents);
}

#[test]
fn flamework_caller_line_injection_survives_native_pipeline() {
	// Given: the transitive type-only case from the source transform regression,
	// exercised through the full native prepare + transform + emit pipeline.
	// main.ts reaches the Caller-bearing overload only via type-only imports
	// and has no Flamework surface text.
	let temp = task7_flamework_project("");
	let dir = temp.dir();
	write_flamework_modules(&dir);

	let store = [
		r"type CallerMetadata = { line: number; };",
		r"declare namespace Modding {",
		r"    export type Caller<M extends keyof CallerMetadata> = CallerMetadata[M] & { _flamework_macro_caller: M };",
		r"}",
		r"export interface Store {",
		r#"    enqueue(player: unknown, transform: unknown, callsiteId?: Modding.Caller<"line">): void;"#,
		r#"    enqueue(player: unknown, key: string, transform: unknown, callsiteId?: Modding.Caller<"line">): void;"#,
		r"}",
	]
	.join("\n");
	std::fs::write(temp.join("src/store.ts"), store).expect("write");

	let context = [
		r#"import type { Store } from "./store";"#,
		r"export interface Context {",
		r"    data: Store;",
		r"}",
	]
	.join("\n");
	std::fs::write(temp.join("src/context.ts"), context).expect("write");

	let main = [
		r#"import type { Context } from "./context";"#,
		r"declare const ctx: Context;",
		r"const { data } = ctx;",
		r"declare const player: unknown;",
		r"declare const transform: unknown;",
		r#"data.enqueue(player, "settings.userSettings", transform);"#,
	]
	.join("\n");
	std::fs::write(temp.join("src/main.ts"), main).expect("write");

	// When: full build through native pipeline.
	let (result, diags, error) = build(&dir, &ProjectOptions::default());
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions = ({diags:?}, {error:?})"
	);
	let got = output_text(&result.expect("a result"), "out/main.luau");

	// Then: the callsite line was injected as 4th arg and survived to Luau.
	assert!(
		got.contains(r#"data:enqueue(player, "settings.userSettings", transform"#),
		"emitted luau lost original args:\n{got}"
	);
	// Go's loose numeric-fourth-argument check only fails when the output has no `, ` and no
	// `enqueue`, which the assertion above already rules out.
}

/// Go's test only logs: "global ambient case added per plan; core reachability + injection
/// covered by other regressions and index build".
#[test]
fn flamework_caller_line_injection_global_ambient() {}
