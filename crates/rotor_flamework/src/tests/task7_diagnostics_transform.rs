//! `task7_diagnostics_transform_test.go`: the table and two real-compiler adapters are one
//! differential contract.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use rotor_config::config::FlameworkConfig;
use rotor_tsrs::P;
use rotor_tsrs::ast::Node;

use super::oracle_corpus::flamework_testdata;
use super::task7_differential::{
	TASK7_ANSI, copy_task7_tree, link_task7_modules, run_combined, task7_fixture, task7_install, task7_source_files,
	task7_ts_file_config,
};
use super::{new_transform_program, temp_dir, write_transform_fixture};
use crate::ErrorDetail;
use crate::project::{ProjectOptions, open_project};
use crate::transform_diagnostics::Diagnostic;
use crate::transform_state::TransformInput;

/// `task7TransformDiagnosticCase`
struct Case {
	row: &'static str,
	fixture: &'static str,
	want_failure: bool,
	baseline: bool,
	unreachable: bool,
	want_upstream_exception_tuple: &'static str,
	want_native_exception_tuples: &'static [&'static str],
}

const CASE: Case = Case {
	row: "",
	fixture: "",
	want_failure: false,
	baseline: false,
	unreachable: false,
	want_upstream_exception_tuple: "",
	want_native_exception_tuples: &[],
};

#[expect(clippy::too_many_lines, reason = "one Go test, ported whole")]
#[test]
fn task7_diagnostics_transform_matches_pinned_upstream_ordered_tuples() {
	// The exact v1.3.2 transformer and canonical diagnostic branch fixtures.
	let base = task7_fixture();
	let fixtures = flamework_testdata("task7-diagnostics-transform");
	let (_install_dir, install) = task7_install(&base);
	let cases = [
		Case {
			row: "DIAG-013",
			fixture: "constant-nonliteral.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "baseline",
			fixture: "template-literal-guard.ts",
			want_failure: true,
			baseline: true,
			..CASE
		},
		Case {
			row: "DIAG-008",
			fixture: "dynamic-obfuscated-access.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-008",
			fixture: "direct-attribute-assignment.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-008",
			fixture: "direct-attribute-delete.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-008",
			fixture: "direct-attribute-unary.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-012",
			fixture: "invalid-intrinsic-dispatch.ts",
			want_failure: true,
			want_upstream_exception_tuple: "error|@flamework/core|case.ts|3|1|Flamework failure occurred here\nError: Invalid intrinsic usage",
			want_native_exception_tuples: &["error|@flamework/core|case.ts|3|1|Invalid path intrinsic usage"],
			..CASE
		},
		Case {
			row: "DIAG-012",
			fixture: "unknown-intrinsic.ts",
			want_failure: true,
			want_upstream_exception_tuple: "error|@flamework/core|case.ts|3|1|Flamework failure occurred here\nTypeError: Cannot use 'in' operator to search for 'diagnostic' in Unexpected intrinsic ID 'task7-unknown' with 0 inputs",
			want_native_exception_tuples: &[
				"error|@flamework/core|case.ts|3|1|Unexpected intrinsic ID \"task7-unknown\" with 0 inputs",
			],
			..CASE
		},
		Case {
			row: "DIAG-012",
			fixture: "missing-inner-unknown.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-012",
			fixture: "missing-inner-macro.ts",
			unreachable: true,
			..CASE
		},
		Case {
			row: "DIAG-012",
			fixture: "unknown-macro-type.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-013",
			fixture: "constant-spread.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-013",
			fixture: "constant-linked-mismatch.ts",
			..CASE
		},
		Case {
			row: "DIAG-014",
			fixture: "unsupported-guard.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-014",
			fixture: "recursive-guard.ts",
			want_failure: true,
			want_upstream_exception_tuple: "error|@flamework/core|case.ts|7|1|Flamework failure occurred here\nRangeError: Maximum call stack size exceeded",
			want_native_exception_tuples: &[
				"error|@flamework/core|case.ts|7|1|generate guard for Recursive: recursive types cannot be represented without a previously emitted guard (type path: [<project>/invalid/case.ts:Recursive])",
				"error|@flamework/core|case.ts|3|10|Type was defined here: Recursive",
			],
			..CASE
		},
		Case {
			row: "DIAG-014",
			fixture: "type-parameter-guard.ts",
			want_failure: true,
			..CASE
		},
		Case {
			row: "DIAG-014",
			fixture: "nominal-intersection-guard.ts",
			want_failure: true,
			..CASE
		},
	];

	for case in &cases {
		let name = format!("{}/{}", case.row, case.fixture.trim_end_matches(".ts"));
		let (_oracle_dir, oracle_root) = temp_dir();
		let (_native_dir, native_root) = temp_dir();
		let data = std::fs::read_to_string(fixtures.join(case.fixture)).expect("read fixture");
		for root in [&oracle_root, &native_root] {
			copy_task7_tree(&base, Path::new(root));
			link_task7_modules(&install, Path::new(root));
			write_transform_fixture(root, "invalid/case.ts", &data);
		}
		let plugin = r#""noSemanticDiagnostics":true,"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full""#;
		write_transform_fixture(
			&oracle_root,
			"tsconfig.json",
			&task7_ts_file_config(plugin, "invalid/case.ts"),
		);
		write_transform_fixture(
			&native_root,
			"tsconfig.json",
			&task7_ts_file_config("", "invalid/case.ts"),
		);

		// Both public compiler/transformer entrypoints process the same source.
		let (oracle_tuples, oracle_failed, raw_oracle) =
			task7_run_upstream_transform_diagnostics(&oracle_root, &fixtures);
		let (native_tuples, native_failed) = task7_run_native_transform_diagnostics(&native_root);

		// Exit state and every ordered location/category/source/message/related tuple are exact.
		if case.unreachable {
			assert!(
				!oracle_failed && oracle_tuples.is_empty() && !native_failed && native_tuples.is_empty(),
				"{name} public-unreachable contract changed\nupstream failed={oracle_failed} tuples={oracle_tuples:?}\nnative failed={native_failed} tuples={native_tuples:?}\nraw upstream:\n{raw_oracle}"
			);
			continue;
		}
		if !case.want_upstream_exception_tuple.is_empty() {
			let normalized_oracle = task7_normalize_exception_tuples(&oracle_tuples, &oracle_root);
			let normalized_native = task7_normalize_exception_tuples(&native_tuples, &native_root);
			assert!(
				oracle_failed
					&& native_failed
					&& normalized_oracle == [case.want_upstream_exception_tuple]
					&& normalized_native == case.want_native_exception_tuples,
				"{name} exception mismatch\nupstream failed={oracle_failed} tuples={normalized_oracle:?}\nnative failed={native_failed} tuples={normalized_native:?}"
			);
			for root in [&oracle_root, &native_root] {
				assert!(
					!Path::new(root).join("out").exists(),
					"{name} wrote compiler output after failure"
				);
			}
			continue;
		}
		if case.baseline {
			assert!(
				oracle_failed == case.want_failure
					&& native_failed == case.want_failure
					&& task7_primary_transform_message(&oracle_tuples)
						== task7_primary_transform_message(&native_tuples),
				"baseline {name} message mismatch\nupstream failed={oracle_failed} tuples={oracle_tuples:?}\nnative failed={native_failed} tuples={native_tuples:?}\nraw upstream:\n{raw_oracle}"
			);
			continue;
		}
		assert!(
			oracle_failed == case.want_failure && native_failed == case.want_failure && oracle_tuples == native_tuples,
			"{name} differential mismatch\nupstream failed={oracle_failed} tuples={oracle_tuples:?}\nnative failed={native_failed} tuples={native_tuples:?}\nraw upstream:\n{raw_oracle}"
		);
	}
}

/// `task7NormalizeExceptionTuples`
fn task7_normalize_exception_tuples(tuples: &[String], root: &str) -> Vec<String> {
	let root = rotor_support::gopath::to_slash(root);
	tuples
		.iter()
		.map(|tuple| {
			let tuple = tuple.replace(root.as_ref(), "<project>");
			match tuple.find("\n    at ") {
				Some(stack) => tuple[..stack].to_owned(),
				None => tuple,
			}
		})
		.collect()
}

/// `task7PrimaryTransformMessage`
fn task7_primary_transform_message(tuples: &[String]) -> String {
	let Some(first) = tuples.first() else {
		return String::new();
	};
	let parts: Vec<&str> = first.splitn(6, '|').collect();
	if parts.len() != 6 {
		return first.clone();
	}
	parts[5].to_owned()
}

/// `task7RunUpstreamTransformDiagnostics`: the tuples, whether any diagnostic was reported, and
/// the raw output.
fn task7_run_upstream_transform_diagnostics(root: &str, fixtures: &Path) -> (Vec<String>, bool, String) {
	let driver = fixtures.join("upstream-driver.cjs");
	let mut command = Command::new("node");
	command
		.arg(&driver)
		.arg(root)
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("NODE_OPTIONS", "--require=./deterministic-random.cjs");
	let (success, output, timed_out) = run_combined(command, Duration::from_secs(45));
	assert!(!timed_out, "upstream compiler timed out");
	let raw = TASK7_ANSI.replace_all(&output, "").into_owned();
	let marker = raw.rfind("TASK7_JSON:");
	let (true, Some(marker)) = (success, marker) else {
		panic!("upstream diagnostic driver success={success} output:\n{raw}");
	};
	let diagnostics: Vec<serde_json::Value> = serde_json::from_str(raw[marker + "TASK7_JSON:".len()..].trim())
		.unwrap_or_else(|error| panic!("decode upstream diagnostics: {error}\n{raw}"));
	let tuples = diagnostics
		.iter()
		.flat_map(task7_upstream_transform_diagnostic_tuples)
		.collect();
	(tuples, !diagnostics.is_empty(), raw)
}

/// `task7UpstreamTransformDiagnosticTuples`
fn task7_upstream_transform_diagnostic_tuples(diagnostic: &serde_json::Value) -> Vec<String> {
	let text = |key: &str| {
		diagnostic
			.get(key)
			.and_then(serde_json::Value::as_str)
			.unwrap_or_default()
	};
	let position = diagnostic
		.get("start")
		.and_then(serde_json::Value::as_i64)
		.unwrap_or(-1);
	let category = match text("category") {
		"message" | "suggestion" => "message",
		category => category,
	};
	let mut tuples = vec![task7_position_transform_tuple(
		category,
		text("file"),
		position,
		text("message"),
	)];
	if let Some(related) = diagnostic.get("related").and_then(serde_json::Value::as_array) {
		for related in related {
			tuples.extend(task7_upstream_transform_diagnostic_tuples(related));
		}
	}
	tuples
}

/// `task7RunNativeTransformDiagnostics`
fn task7_run_native_transform_diagnostics(root: &str) -> (Vec<String>, bool) {
	rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(root);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let files = task7_source_files(program, Path::new(root));
		let mut project = open_project(&ProjectOptions {
			project_dir: root.to_owned(),
			root_dir: "invalid".to_owned(),
			out_dir: "out".to_owned(),
			config: FlameworkConfig {
				no_semantic_diagnostics: true,
				salt: "task7-controlled-salt".to_owned(),
				hash_prefix: "task7".to_owned(),
				id_generation_mode: "full".to_owned(),
				..FlameworkConfig::default()
			},
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let result = crate::transform::transform(TransformInput {
			program,
			checker: &mut checker,
			files,
			project: &mut project,
			analyses: Vec::new(),
			macro_runtime: None,
			print: None,
		});
		match result {
			Err(error) => (task7_native_error_tuples(&error), true),
			Ok(result) => {
				let tuples: Vec<String> = result
					.diagnostics
					.iter()
					.flat_map(task7_ast_diagnostic_tuples)
					.collect();
				let failed = !result.diagnostics.is_empty();
				(tuples, failed)
			}
		}
	})
}

/// `task7NativeErrorTuples`
fn task7_native_error_tuples(error: &crate::Error) -> Vec<String> {
	match error.detail() {
		Some(ErrorDetail::Macro(macro_error)) if macro_error.node.is_some() => {
			let mut tuples = Vec::new();
			if let Some(node) = macro_error.node {
				tuples.push(task7_node_transform_tuple("error", node, &macro_error.message));
			}
			for related in &macro_error.related_information {
				if let Some(node) = related.node {
					tuples.push(task7_node_transform_tuple("message", node, &related.message));
				}
			}
			tuples
		}
		Some(ErrorDetail::Guard(guard_error)) => {
			let mut tuples = vec![task7_position_transform_tuple(
				"error",
				&guard_error.file_name,
				i64::from(guard_error.start),
				&guard_error.message(),
			)];
			for related in &guard_error.related_information {
				tuples.push(task7_position_transform_tuple(
					"error",
					&related.file_name,
					i64::from(related.start),
					&format!("Type was defined here: {}", related.type_name),
				));
			}
			tuples
		}
		_ => vec![format!("fatal|@flamework/core||||{}", error.message())],
	}
}

/// `task7ASTDiagnosticTuples`
fn task7_ast_diagnostic_tuples(diagnostic: &Diagnostic) -> Vec<String> {
	let file_name = diagnostic
		.file
		.map(|file| file.file_name().to_owned())
		.unwrap_or_default();
	let mut tuples = vec![task7_position_transform_tuple(
		diagnostic.category.name(),
		&file_name,
		i64::from(diagnostic.pos()),
		diagnostic.string(),
	)];
	for related in &diagnostic.related_information {
		tuples.extend(task7_ast_diagnostic_tuples(related));
	}
	tuples
}

/// `task7NodeTransformTuple`
fn task7_node_transform_tuple(category: &str, node: P<Node>, message: &str) -> String {
	let file = rotor_tsrs::ast::get_source_file_of_node(node).expect("node file");
	let position = rotor_tsrs::scanner::get_token_pos_of_node(node, file, false);
	task7_position_transform_tuple(category, file.file_name(), i64::from(position), message)
}

/// `task7PositionTransformTuple`: `category|@flamework/core|file|line|column|message`.
fn task7_position_transform_tuple(category: &str, file_name: &str, position: i64, message: &str) -> String {
	let file = if file_name.is_empty() {
		String::new()
	} else {
		rotor_support::gopath::base(file_name)
	};
	let data = std::fs::read(file_name);
	let (Ok(data), Ok(position)) = (data, usize::try_from(position)) else {
		return format!("{category}|@flamework/core|{file}|||{message}");
	};
	let prefix = &data[..position.min(data.len())];
	let line = prefix.split(|byte| *byte == b'\n').count();
	let column = prefix.len()
		- prefix
			.iter()
			.rposition(|byte| *byte == b'\n')
			.map_or(0, |index| index + 1)
		+ 1;
	format!("{category}|@flamework/core|{file}|{line}|{column}|{message}")
}
