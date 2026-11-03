//! `flamework_workspace_import_test.go`

use std::path::Path;

use super::flamework_t_import::write_file;
use super::sidecar::{build, output_text};
use super::solution_flamework_integration::symlink;
use super::{NO_LIB_GLOBAL_STUBS, Temp, write_project};
use crate::project::ProjectOptions;

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test")]
fn native_flamework_guard_preserves_scoped_workspace_import() {
	for local_guard in [false, true] {
		let name = if local_guard {
			"local_guard_dependency"
		} else {
			"hoisted_guard_dependency"
		};
		let temp = Temp::new();
		let workspace = temp.dir();
		let dir = format!("{workspace}/packages/consumer");
		write_file(&dir, "package.json", r#"{"name":"@workspace/consumer"}"#);
		write_file(&dir, "rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n");
		write_file(
			&dir,
			"tsconfig.json",
			r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@workspace", "node_modules/@rbxts", "node_modules/@flamework"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}"#,
		);
		write_file(
			&dir,
			"src/globals.d.ts",
			&NO_LIB_GLOBAL_STUBS.replacen(
				"interface Array<T> {}",
				"interface Array<T> { readonly length: number; readonly [index: number]: T; }",
				1,
			),
		);

		let core = format!("{workspace}/node_modules/.pnpm/@flamework+core@1.0.0/node_modules/@flamework/core");
		let guard = format!("{workspace}/node_modules/.pnpm/@rbxts+t@1.0.0/node_modules/@rbxts/t");
		let provider = format!("{workspace}/packages/provider");
		write_file(
			&core,
			"package.json",
			r#"{"name":"@flamework/core","version":"1.0.0","types":"out/index.d.ts","main":"out/init.luau"}"#,
		);
		write_file(
			&core,
			"out/index.d.ts",
			r#"import { t } from "@rbxts/t";
export declare namespace Flamework {
	/** @metadata macro */
	function createGuard<T>(meta?: { _flamework_macro_generic: [T, "guard"] }): t.check<T>;
}"#,
		);
		write_file(
			&core,
			"out/init.luau",
			"return { Flamework = { createGuard = function(guard) return guard end } }\n",
		);
		write_file(&core, "out/prelude.d.ts", "export { t } from \"@rbxts/t\";\n");
		write_file(
			&core,
			"out/prelude.luau",
			"local TS = _G[script]\nreturn { t = TS.import(script, TS.getModule(script, \"@rbxts\", \"t\").lib).t }\n",
		);
		write_file(
			&guard,
			"package.json",
			r#"{"name":"@rbxts/t","version":"1.0.0","types":"lib/index.d.ts","main":"lib/init.luau"}"#,
		);
		write_file(
			&guard,
			"lib/index.d.ts",
			"export declare namespace t {\n\ttype check<T> = (value: unknown) => value is T;\n\tfunction string(value: unknown): value is string;\n}\n",
		);
		write_file(
			&guard,
			"lib/init.luau",
			"return { t = { string = function(value) return type(value) == \"string\" end } }\n",
		);
		write_file(
			&provider,
			"package.json",
			r#"{"name":"@workspace/provider","version":"1.0.0","types":"src/index.d.ts","main":"src/init.luau","exports":{".":{"types":"./src/index.d.ts","default":"./src/init.luau"}}}"#,
		);
		write_file(&provider, "src/index.d.ts", "export declare const value: number;\n");
		write_file(&provider, "src/init.luau", "return { value = 1 }\n");
		let mut links = vec![
			(format!("{dir}/node_modules/@workspace/provider"), provider.clone()),
			(format!("{dir}/node_modules/@flamework/core"), core.clone()),
			(format!("{workspace}/node_modules/@rbxts/t"), guard.clone()),
			(
				format!("{workspace}/node_modules/.pnpm/@flamework+core@1.0.0/node_modules/@rbxts/t"),
				guard.clone(),
			),
		];
		if local_guard {
			links.push((format!("{dir}/node_modules/@rbxts/t"), guard.clone()));
		}
		for (link, target) in &links {
			let link = Path::new(link);
			std::fs::create_dir_all(link.parent().expect("parent")).expect("mkdir");
			symlink(Path::new(target), link);
		}

		for with_guard in [false, true] {
			let mut main = "import { value } from \"@workspace/provider\";\nimport { Flamework } from \"@flamework/core\";\nexport const result = value;\n".to_owned();
			if with_guard {
				main.push_str(
					"export const isString: (value: unknown) => value is string = Flamework.createGuard<string>();\n",
				);
			}
			let case = if with_guard { "with_guard" } else { "without_guard" };
			write_file(&dir, "src/main.ts", &main);
			let (result, diagnostics, error) = build(&dir, &ProjectOptions::default());
			assert!(
				error.is_none() && diagnostics.is_empty(),
				"{name}/{case}: BuildProjectWithOptions: {error:?} ({diagnostics:?})"
			);
			let output = output_text(&result.expect("a result"), "out/main.luau");
			assert!(
				output.contains(r#"TS.import(script, TS.getModule(script, "@workspace", "provider").src).value"#),
				"{name}/{case}: workspace runtime import missing:\n{output}"
			);
			if with_guard {
				let guard_import = if local_guard {
					r#"TS.import(script, TS.getModule(script, "@rbxts", "t").lib).t"#
				} else {
					r#"TS.import(script, TS.getModule(script, "@flamework", "core").out.prelude).t"#
				};
				assert!(
					output.contains(guard_import) && output.contains("t.string"),
					"{name}/{case}: generated guard runtime import missing:\n{output}"
				);
			}
		}
	}
}

#[test]
fn native_flamework_rejects_unsupported_runtime_imports() {
	for (module, message) in [
		("unscoped", "You cannot use modules directly under node_modules."),
		(
			"@unconfigured/provider",
			"You can only use npm scopes that are listed in your typeRoots.",
		),
	] {
		let temp = write_project("@workspace/consumer", "");
		let dir = temp.dir();
		write_file(&dir, "rotor.toml", "[flamework]\nnoSemanticDiagnostics = true\n");
		let module_dir = format!("{dir}/node_modules/{module}");
		write_file(
			&module_dir,
			"package.json",
			r#"{"types":"index.d.ts","main":"init.luau"}"#,
		);
		write_file(&module_dir, "index.d.ts", "export declare const value: number;\n");
		write_file(&module_dir, "init.luau", "return { value = 1 }\n");
		write_file(
			&dir,
			"src/main.ts",
			&format!("import {{ value }} from \"{module}\";\nexport const result = value;\n"),
		);
		let (_, diagnostics, error) = build(&dir, &ProjectOptions::default());
		assert!(
			error.is_some() && diagnostics.len() == 1 && diagnostics[0] == message,
			"{module}: BuildProjectWithOptions: {error:?} ({diagnostics:?}), want {message:?}"
		);
	}
}
