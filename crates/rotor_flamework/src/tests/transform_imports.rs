//! `transform_imports_test.go`

use rotor_tsrs::ast::{NodeFactory, NodeFactoryHooks, new_node_factory};

use super::{parse_ts, print_source_file};
use crate::transform_decorators_imports::FLAMEWORK_PRELUDE_MODULE;
use crate::transform_expressions::macro_import_statements;
use crate::transform_imports::prepend_flamework_reflect_import;
use crate::transform_macro_types::MacroImport;

fn plain_factory() -> NodeFactory {
	new_node_factory(NodeFactoryHooks::default())
}

fn macro_import(module: &str, name: &str) -> MacroImport {
	MacroImport {
		module: module.to_owned(),
		export: name.to_owned(),
		local: name.to_owned(),
	}
}

#[test]
fn prepend_flamework_reflect_import_deduplicates_alias_and_preserves_first_comment() {
	let file = parse_ts(
		"/input.ts",
		"// @directive\nimport { Reflect as RuntimeReflect } from \"@flamework/core\";\nclass Counter {}\n",
	);

	let printed = print_source_file(prepend_flamework_reflect_import(&plain_factory(), file));

	assert_eq!(
		printed.matches(r#"from "@flamework/core""#).count(),
		1,
		"Reflect import count mismatch:\n{printed}"
	);
	assert!(
		printed.starts_with("// @directive\nimport"),
		"first-statement comment was not preserved:\n{printed}"
	);
}

#[test]
fn prepend_flamework_reflect_import_places_new_import_after_first_statement_comment() {
	let file = parse_ts("/input.ts", "// @directive\nclass Counter {}\n");

	let printed = print_source_file(prepend_flamework_reflect_import(&plain_factory(), file));

	assert!(
		printed.starts_with("// @directive\nimport { Reflect }"),
		"comment/import order mismatch:\n{printed}"
	);
	assert_eq!(
		printed.matches("// @directive").count(),
		1,
		"first comment duplicated:\n{printed}"
	);
}

#[test]
fn prepend_flamework_reflect_import_hoists_and_deduplicates_generated_prerequisite_imports() {
	let file = parse_ts("/input.ts", "// @directive\nconst before = true;\n");
	let factory = plain_factory();
	let generated = macro_import_statements(
		&factory,
		&[
			macro_import("@flamework/core", "Flamework"),
			macro_import("@flamework/core", "Flamework"),
		],
	);
	let mut statements = file.statements().nodes().to_vec();
	statements.extend(generated);
	let file = factory
		.update_source_file(
			file.as_node(),
			factory.new_node_list(statements),
			file.end_of_file_token,
		)
		.as_source_file_p();

	let printed = print_source_file(prepend_flamework_reflect_import(&factory, file));

	let reflect_index = printed.find("import { Reflect }");
	let flamework_index = printed.find("import { Flamework }");
	let statement_index = printed.find("const before");
	assert!(
		reflect_index.is_some()
			&& flamework_index.is_some()
			&& statement_index.is_some()
			&& reflect_index <= statement_index
			&& flamework_index <= statement_index,
		"generated import prerequisite order mismatch:\n{printed}"
	);
	assert!(
		printed.matches("import { Flamework }").count() == 1 && printed.matches("// @directive").count() == 1,
		"generated import dedupe/comment mismatch:\n{printed}"
	);
}

#[test]
fn prepend_flamework_reflect_import_moves_first_import_comment_to_hoisted_generated_import() {
	let file = parse_ts(
		"/input.ts",
		"// @directive\nimport { Reflect as RuntimeReflect } from \"@flamework/core\";\nclass Counter {}\n",
	);
	let factory = plain_factory();
	let mut statements = macro_import_statements(&factory, &[macro_import(FLAMEWORK_PRELUDE_MODULE, "t")]);
	statements.extend_from_slice(file.statements().nodes());
	let file = factory
		.update_source_file(
			file.as_node(),
			factory.new_node_list(statements),
			file.end_of_file_token,
		)
		.as_source_file_p();

	let printed = print_source_file(prepend_flamework_reflect_import(&factory, file));

	assert!(
		printed.starts_with("// @directive\nimport { t } from \"@flamework/core/out/prelude\";")
			&& printed.matches("// @directive").count() == 1,
		"hoisted import comment transfer mismatch:\n{printed}"
	);
}
