//! `flamework_nosemantic_test.go`

use super::sidecar::{build, output_text};
use super::task7_acceptance::task7_flamework_project;
use super::{large_stack, write_project};
use crate::compile::pre_emit_project_file_diagnostics_with_options;
use crate::project::{ProjectOptions, new_project_program_with_options};

#[test]
fn pre_emit_project_file_diagnostics_skips_semantic_when_no_semantic_diagnostics_configured() {
	// Given: a project source with a stable TypeScript semantic error.
	let temp = write_project("@scope/no-semantic-preemit", "");
	temp.write("src/main.ts", "export const value: number = \"not-a-number\";\n");
	let dir = temp.dir();
	large_stack(|| {
		let (abs_dir, program) = new_project_program_with_options(&dir, "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {} ({:?})", failure.error, failure.messages()));
		let source_file = program
			.get_source_file(&format!("{abs_dir}/src/main.ts"))
			.expect("source missing");
		let context = rotor_tsrs::Context::default();

		// When: pre-emit runs with semantic checks enabled.
		let with_semantic =
			pre_emit_project_file_diagnostics_with_options(&context, program, source_file, &ProjectOptions::default());
		// Then: the semantic assignment error is reported.
		assert!(
			!with_semantic.is_empty(),
			"expected semantic diagnostics when SkipSemanticDiagnostics is false"
		);

		// When: pre-emit honors noSemanticDiagnostics.
		let skipped = pre_emit_project_file_diagnostics_with_options(
			&context,
			program,
			source_file,
			&ProjectOptions {
				skip_semantic_diagnostics: true,
				..ProjectOptions::default()
			},
		);
		// Then: only syntactic diagnostics remain (none for this fixture).
		assert!(
			skipped.is_empty(),
			"SkipSemanticDiagnostics pre-emit = {} diagnostics, want none",
			skipped.len()
		);
	});
}

#[test]
fn skip_semantic_diagnostics_emits_unresolved_identifier_instead_of_ice() {
	// Given: noSemanticDiagnostics skipped the TS2304 that would have aborted transform, so an
	// unresolved identifier reaches the roblox-ts emitter.
	let temp = write_project("@scope/no-semantic-identifier", "");
	temp.write("src/main.ts", "export const x = neverDeclared;\n");

	// When: the real compile path honors SkipSemanticDiagnostics.
	let opts = ProjectOptions {
		skip_semantic_diagnostics: true,
		..ProjectOptions::default()
	};
	let (result, diags, error) = build(&temp.dir(), &opts);

	// Then: emit the identifier text instead of the upstream symbol assert.
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions = ({diags:?}, {error:?})"
	);
	let got = output_text(&result.expect("a result"), "out/main.luau");
	assert!(
		got.contains("neverDeclared"),
		"output missing unresolved identifier:\n{got}"
	);
}

#[test]
fn flamework_no_semantic_diagnostics_emits_unresolved_identifier_instead_of_ice() {
	// Given: native Flamework is on and the profile disables semantic diagnostics.
	let temp = task7_flamework_project("[flamework]\nnoSemanticDiagnostics = true\n");
	temp.write("src/main.ts", "export const x = neverDeclared;\n");

	// When: the build reads noSemanticDiagnostics from rotor.toml.
	let (result, diags, error) = build(&temp.dir(), &ProjectOptions::default());

	// Then: the Flamework flag reaches the transformer and does not ICE.
	assert!(
		error.is_none() && diags.is_empty(),
		"BuildProjectWithOptions = ({diags:?}, {error:?})"
	);
	let got = output_text(&result.expect("a result"), "out/main.luau");
	assert!(
		got.contains("neverDeclared"),
		"output missing unresolved identifier:\n{got}"
	);
}
