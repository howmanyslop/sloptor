//! `validateoptions_test.go`, through `rotor_tsrs::project::parse_project_config`, which runs
//! `validateCompilerOptions` over the sanitized config the way a build does.

use std::path::Path;

use rotor_tsrs::LoadError;

use super::{Temp, fixture_project_dir};

const VALIDATE_HEADER: &str =
	"Invalid \"tsconfig.json\" configuration!\nhttps://roblox-ts.com/docs/quick-start#project-folder-setup\n";

/// `validateConfigText`: the validation message for `tsconfig` (empty when it passes) and the
/// project directory.
fn validate_config_text(tsconfig: &str, extra: &[(&str, &str)]) -> (String, Temp) {
	let temp = Temp::new();
	temp.write("tsconfig.json", tsconfig);
	temp.write("src/main.ts", "export {};\n");
	for (name, contents) in extra {
		temp.write(name, contents);
	}
	(validate(&temp.join("tsconfig.json")), temp)
}

fn validate(config: &Path) -> String {
	rotor_tsrs::with_large_stack(|| {
		match rotor_tsrs::project::parse_project_config(config, &rotor_tsrs::ProjectOptions::default()) {
			Ok(_) => String::new(),
			Err(LoadError::Invalid(message)) => message,
			Err(error) => panic!("config parse diagnostics: {error}"),
		}
	})
}

/// `validConfig`: the canonical config with `overrides` replacing (or, when empty, dropping)
/// whole lines.
fn valid_config(overrides: &[(&str, &str)]) -> String {
	let lines = [
		(
			"allowSyntheticDefaultImports",
			"\t\t\"allowSyntheticDefaultImports\": true,",
		),
		("downlevelIteration", "\t\t\"downlevelIteration\": true,"),
		("module", "\t\t\"module\": \"commonjs\","),
		("moduleResolution", "\t\t\"moduleResolution\": \"Node\","),
		("noLib", "\t\t\"noLib\": true,"),
		("moduleDetection", "\t\t\"moduleDetection\": \"force\","),
		("strict", "\t\t\"strict\": true,"),
		("target", "\t\t\"target\": \"ESNext\","),
		("typeRoots", "\t\t\"typeRoots\": [\"node_modules/@rbxts\"],"),
		("types", ""),
		("importsNotUsedAsValues", ""),
		("rootDir", "\t\t\"rootDir\": \"src\","),
		("outDir", "\t\t\"outDir\": \"out\","),
	];
	let kept: Vec<&str> = lines
		.iter()
		.map(|(key, line)| {
			overrides
				.iter()
				.find(|(override_key, _)| override_key == key)
				.map_or(*line, |(_, replacement)| *replacement)
		})
		.filter(|line| !line.is_empty())
		.collect();
	let joined = kept.join("\n");
	let joined = joined.strip_suffix(',').unwrap_or(&joined);
	format!("{{\n\t\"compilerOptions\": {{\n{joined}\n\t}},\n\t\"include\": [\"src\"]\n}}\n")
}

struct Case {
	name: &'static str,
	overrides: &'static [(&'static str, &'static str)],
	extra: &'static [(&'static str, &'static str)],
	want: Option<&'static [&'static str]>,
}

const TYPES_NOT_FOUND: &str = "\"types\" services were not found. Make sure the path is relative to `typeRoots`";

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test's table")]
fn validate_compiler_options() {
	let cases = [
		Case {
			name: "canonical valid config",
			overrides: &[],
			extra: &[],
			want: None,
		},
		Case {
			name: "preserve bundler config",
			overrides: &[
				("module", "\t\t\"module\": \"preserve\","),
				("moduleResolution", "\t\t\"moduleResolution\": \"bundler\","),
			],
			extra: &[],
			want: None,
		},
		Case {
			name: "target not enforced",
			overrides: &[("target", "\t\t\"target\": \"ES2015\",")],
			extra: &[],
			want: None,
		},
		Case {
			name: "noLib missing",
			overrides: &[("noLib", "")],
			extra: &[],
			want: Some(&[r#""noLib" must be true"#]),
		},
		Case {
			name: "noLib false",
			overrides: &[("noLib", "\t\t\"noLib\": false,")],
			extra: &[],
			want: Some(&[r#""noLib" must be true"#]),
		},
		Case {
			name: "strict missing",
			overrides: &[("strict", "")],
			extra: &[],
			want: Some(&[r#""strict" must be true"#]),
		},
		Case {
			name: "module esnext",
			overrides: &[("module", "\t\t\"module\": \"esnext\",")],
			extra: &[],
			want: Some(&[r#""module" must be commonjs"#]),
		},
		Case {
			name: "moduleDetection missing",
			overrides: &[("moduleDetection", "")],
			extra: &[],
			want: Some(&[r#""moduleDetection" must be "force""#]),
		},
		Case {
			name: "moduleResolution bundler",
			overrides: &[("moduleResolution", "\t\t\"moduleResolution\": \"bundler\",")],
			extra: &[],
			want: Some(&[r#""moduleResolution" must be "Node""#]),
		},
		Case {
			name: "moduleResolution missing",
			overrides: &[("moduleResolution", "")],
			extra: &[],
			want: Some(&[r#""moduleResolution" must be "Node""#]),
		},
		Case {
			name: "moduleResolution Node10 spelling",
			overrides: &[("moduleResolution", "\t\t\"moduleResolution\": \"Node10\",")],
			extra: &[],
			want: None,
		},
		Case {
			name: "allowSyntheticDefaultImports missing",
			overrides: &[("allowSyntheticDefaultImports", "")],
			extra: &[],
			want: Some(&[r#""allowSyntheticDefaultImports" must be true"#]),
		},
		Case {
			name: "typeRoots missing",
			overrides: &[("typeRoots", "")],
			extra: &[],
			want: Some(&[r#""typeRoots" must contain <rbxts>"#]),
		},
		Case {
			name: "typeRoots without the rbxts scope",
			overrides: &[("typeRoots", "\t\t\"typeRoots\": [\"node_modules/@types\"],")],
			extra: &[],
			want: Some(&[r#""typeRoots" must contain <rbxts>"#]),
		},
		Case {
			name: "typeRoots extra entries ok",
			overrides: &[(
				"typeRoots",
				"\t\t\"typeRoots\": [\"node_modules/@types\", \"node_modules/@rbxts\"],",
			)],
			extra: &[],
			want: None,
		},
		Case {
			name: "types entry not found",
			overrides: &[("types", "\t\t\"types\": [\"services\"],")],
			extra: &[],
			want: Some(&[TYPES_NOT_FOUND]),
		},
		Case {
			name: "types entry as directory",
			overrides: &[("types", "\t\t\"types\": [\"services\"],")],
			extra: &[("node_modules/@rbxts/services/index.d.ts", "export {};\n")],
			want: None,
		},
		Case {
			name: "types entry as dts file",
			overrides: &[("types", "\t\t\"types\": [\"globals\"],")],
			extra: &[("node_modules/@rbxts/globals.d.ts", "declare const g: number;\n")],
			want: None,
		},
		Case {
			name: "rootDir missing",
			overrides: &[("rootDir", "")],
			extra: &[],
			want: Some(&[r#""rootDir" or "rootDirs" must be defined"#]),
		},
		Case {
			name: "rootDirs stands in for rootDir",
			overrides: &[("rootDir", "\t\t\"rootDirs\": [\"src\"],")],
			extra: &[],
			want: None,
		},
		Case {
			name: "outDir missing",
			overrides: &[("outDir", "")],
			extra: &[],
			want: Some(&[r#""outDir" must be defined"#]),
		},
		Case {
			name: "importsNotUsedAsValues error",
			overrides: &[("importsNotUsedAsValues", "\t\t\"importsNotUsedAsValues\": \"error\",")],
			extra: &[],
			want: Some(&[
				r#""importsNotUsedAsValues" is no longer supported, use "verbatimModuleSyntax": false instead"#,
			]),
		},
		Case {
			name: "importsNotUsedAsValues preserve",
			overrides: &[(
				"importsNotUsedAsValues",
				"\t\t\"importsNotUsedAsValues\": \"preserve\",",
			)],
			extra: &[],
			want: Some(&[
				r#""importsNotUsedAsValues" is no longer supported, use "verbatimModuleSyntax": true instead"#,
			]),
		},
		Case {
			name: "all violations in upstream order",
			overrides: &[
				("noLib", ""),
				("strict", ""),
				("module", ""),
				("moduleDetection", ""),
				("moduleResolution", ""),
				("allowSyntheticDefaultImports", ""),
				("typeRoots", ""),
				("types", "\t\t\"types\": [\"services\"],"),
				("rootDir", ""),
				("outDir", ""),
				("importsNotUsedAsValues", "\t\t\"importsNotUsedAsValues\": \"remove\","),
			],
			extra: &[],
			want: Some(&[
				r#""noLib" must be true"#,
				r#""strict" must be true"#,
				r#""module" must be commonjs"#,
				r#""moduleDetection" must be "force""#,
				r#""moduleResolution" must be "Node""#,
				r#""allowSyntheticDefaultImports" must be true"#,
				r#""typeRoots" must contain <rbxts>"#,
				TYPES_NOT_FOUND,
				r#""rootDir" or "rootDirs" must be defined"#,
				r#""outDir" must be defined"#,
				r#""importsNotUsedAsValues" is no longer supported, use "verbatimModuleSyntax": false instead"#,
			]),
		},
	];
	for case in cases {
		let (got, temp) = validate_config_text(&valid_config(case.overrides), case.extra);
		let Some(want_lines) = case.want else {
			assert_eq!(got, "", "{}: validateCompilerOptions, want clean", case.name);
			continue;
		};
		let rbxts = temp.join("node_modules/@rbxts").to_string_lossy().into_owned();
		let mut want = VALIDATE_HEADER.to_owned();
		for line in want_lines {
			want.push_str("- ");
			want.push_str(&line.replace("<rbxts>", &rbxts));
			want.push('\n');
		}
		assert_eq!(got, want, "{}: validateCompilerOptions", case.name);
	}
}

#[test]
fn validate_compiler_options_accepts_preserve_bundler_from_extended_config() {
	let base = valid_config(&[
		("module", "\t\t\"module\": \"preserve\","),
		("moduleResolution", "\t\t\"moduleResolution\": \"bundler\","),
	]);
	let (got, _temp) = validate_config_text(
		r#"{"extends":"./tsconfig.base.json"}"#,
		&[("tsconfig.base.json", &base)],
	);
	assert_eq!(got, "", "validateCompilerOptions, want clean");
}

#[test]
fn validate_compiler_options_fixture_config() {
	let got = validate(&Path::new(&fixture_project_dir()).join("tsconfig.json"));
	assert_eq!(got, "", "fixture config rejected");
}
