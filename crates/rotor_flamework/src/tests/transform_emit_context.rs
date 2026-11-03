//! `transform_emit_context_test.go`

use rotor_tsrs::ast::Kind;

use super::with_expression_fixture;
use crate::transform_state::{SourceMetadata, TransformInput, new_transform_state};

fn print_with_context(metadata: &SourceMetadata) -> String {
	rotor_tsrs::printer::new_printer(
		rotor_tsrs::printer::PrinterOptions::default(),
		rotor_tsrs::printer::PrintHandlers::default(),
		Some(metadata.emit_context()),
	)
	.emit_source_file(metadata.transformed())
}

#[test]
fn new_transform_state_preserves_synthetic_comment_when_node_is_updated_and_printed() {
	let printed = with_expression_fixture("export {};\n", |base, source_file| {
		let state = new_transform_state(base.program, base.checker, base.project, None, &[]);
		let factory = state.factory();
		let statement = factory.new_expression_statement(factory.new_identifier("before"));
		let updated = factory.update_expression_statement(statement, factory.new_identifier("after"));
		assert_eq!(
			state.emit_context().original(updated),
			Some(statement),
			"updated node lost its source mapping"
		);
		state.emit_context().add_synthetic_leading_comment(
			updated,
			Kind::SingleLineCommentTrivia,
			" @flamework metadata",
			true,
		);
		let transformed = factory
			.update_source_file(
				source_file.as_node(),
				factory.new_node_list(vec![updated]),
				source_file.end_of_file_token,
			)
			.as_source_file_p();
		print_with_context(&SourceMetadata::new(source_file, transformed, state.emit_context()))
	});

	assert!(
		printed.contains("// @flamework metadata\nafter;"),
		"printed synthetic metadata =\n{printed}"
	);
}

#[test]
fn transform_exposes_emit_context_for_caller_printing() {
	let printed = with_expression_fixture("export {};\n", |base, source_file| {
		let result = crate::transform::transform(TransformInput {
			program: base.program,
			checker: base.checker,
			files: vec![source_file],
			project: base.project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		})
		.expect("Transform");
		let metadata = &result.sources[0];
		let statement = metadata.transformed().statements().nodes()[0];
		metadata.emit_context().add_synthetic_leading_comment(
			statement,
			Kind::SingleLineCommentTrivia,
			" caller metadata",
			true,
		);
		print_with_context(metadata)
	});

	assert!(
		printed.contains("// caller metadata\nexport {};"),
		"caller-printed synthetic metadata =\n{printed}"
	);
}
