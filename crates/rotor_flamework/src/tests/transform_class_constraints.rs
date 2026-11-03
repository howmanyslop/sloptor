//! `transform_class_constraints_test.go`

use super::with_class_analysis_input;
use crate::transform_analysis::analyze_flamework_classes;
use crate::transform_class_diagnostics::add_class_transform_diagnostic;
use crate::transform_class_diagnostics_constraints::collect_node_metadata;
use crate::transform_diagnostics::text_pos;
use crate::transform_state::new_transform_state;
use crate::{Error, ErrorKind};

const PACKAGE_JSON: (&str, &str) = ("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#);
const DECORATOR_TSCONFIG: (&str, &str) = (
	"tsconfig.json",
	r#"{"compilerOptions":{"experimentalDecorators":true,"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
);
const PLAIN_TSCONFIG: (&str, &str) = (
	"tsconfig.json",
	r#"{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}"#,
);

/// Each diagnostic's message and its related information's messages and positions.
type DiagnosticSummary = Vec<(String, Vec<(String, i32)>)>;

fn analyze(
	tsconfig: (&str, &str),
	source: &[&str],
) -> Result<
	(
		Vec<crate::transform_analysis::DiscoveredFlameworkClass>,
		DiagnosticSummary,
	),
	Error,
> {
	let source = source.join("\n");
	with_class_analysis_input(
		&[PACKAGE_JSON, tsconfig, ("src/service.ts", &source)],
		"src/service.ts",
		|input| {
			let analysis = analyze_flamework_classes(input)?;
			let diagnostics = analysis
				.diagnostics()
				.iter()
				.map(|diagnostic| {
					let related = diagnostic
						.related_information
						.iter()
						.map(|related| (related.string().to_owned(), related.pos()))
						.collect();
					(diagnostic.string().to_owned(), related)
				})
				.collect();
			Ok((analysis.classes(), diagnostics))
		},
	)
}

#[test]
fn constraint_diagnostic_has_ordered_related_trace() {
	let (_, diagnostics) = analyze(
		DECORATOR_TSCONFIG,
		&[
			"interface Required { required(): void }",
			"/** @metadata reflect {@link Required constraint} */",
			"interface Marked {}",
			"export class Invalid implements Marked {}",
		],
	)
	.expect("AnalyzeFlameworkClasses");

	assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
	assert_eq!(
		diagnostics[0].0,
		"Type 'Invalid' does not satisfy the constraint 'Required'."
	);
	let related: Vec<&str> = diagnostics[0].1.iter().map(|(message, _)| message.as_str()).collect();
	assert_eq!(related, ["'The constraint' is declared here."]);
}

#[test]
fn constraint_diagnostics_preserve_file_position_order() {
	let (_, diagnostics) = analyze(
		PLAIN_TSCONFIG,
		&[
			"interface Required { required(): void }",
			"/** @metadata reflect {@link Required constraint} */ interface Marked {}",
			"export class Zed implements Marked {}",
			"export class Alpha implements Marked {}",
		],
	)
	.expect("AnalyzeFlameworkClasses");

	assert!(
		diagnostics.len() == 2 && diagnostics[0].0.contains("Zed") && diagnostics[1].0.contains("Alpha"),
		"ordered diagnostics = {diagnostics:?}"
	);
}

/// Go runs this on a zero `TransformState` over a bare parse; the Rust state needs a program, so
/// the class comes from a one-file fixture.
#[test]
fn constraint_diagnostic_capture_adds_located_decorator_error() {
	let (handled, located) = with_class_analysis_input(
		&[PACKAGE_JSON, PLAIN_TSCONFIG, ("src/service.ts", "class Service {}")],
		"src/service.ts",
		|input| {
			let file = input.files[0];
			let node = file.statements().nodes()[0];
			let mut state = new_transform_state(input.program, input.checker, input.project, None, &[]);
			let error = Error::sentinel(ErrorKind::InvalidDecorator)
				.reword(format!("{}\nbad signature", ErrorKind::InvalidDecorator.text()));
			let handled = add_class_transform_diagnostic(&mut state, node, &error);
			let located: Vec<(bool, i32, i32, String)> = state
				.diagnostics
				.iter()
				.map(|diagnostic| {
					(
						diagnostic.file == Some(file),
						diagnostic.pos(),
						diagnostic.end(),
						diagnostic.string().to_owned(),
					)
				})
				.collect();
			(handled, (located, text_pos(node.pos()), text_pos(node.end())))
		},
	);

	let (diagnostics, pos, end) = located;
	assert!(handled, "addClassTransformDiagnostic() did not handle the error");
	assert_eq!(
		diagnostics,
		[(true, pos, end, "Decorators are not valid here.".to_owned())]
	);
}

#[test]
fn circular_dependency_rejects_constructor_graph_deterministically() {
	let error = analyze(
		PLAIN_TSCONFIG,
		&[
			"/** @metadata reflect */ export class A { constructor(value: B) {} }",
			"/** @metadata reflect */ export class B { constructor(value: A) {} }",
		],
	)
	.expect_err("AnalyzeFlameworkClasses");

	assert_eq!(
		error.message(),
		"circular Flamework constructor dependency: fixture-game:out/service@A -> fixture-game:out/service@B -> fixture-game:out/service@A"
	);
}

#[test]
fn analyze_flamework_classes_inherits_member_metadata_from_implemented_interface() {
	let source = [
		"interface Contract {",
		"  /** @metadata reflect */",
		"  run(): void",
		"}",
		"class Implementation implements Contract {",
		"  run(): void {}",
		"}",
	]
	.join("\n");
	let requested = with_class_analysis_input(
		&[PACKAGE_JSON, PLAIN_TSCONFIG, ("src/service.ts", &source)],
		"src/service.ts",
		|input| {
			let class = input.files[0].statements().nodes()[1];
			let member = *class.members().first().expect("Implementation class has no members");
			let mut state = new_transform_state(input.program, input.checker, input.project, None, &[]);
			collect_node_metadata(&mut state, member).requested("reflect")
		},
	);

	assert!(requested, "interface member metadata did not request reflect");
}

#[test]
fn analyze_flamework_classes_discovers_metadata_and_rejects_constraint() {
	let (classes, diagnostics) = analyze(
		DECORATOR_TSCONFIG,
		&[
			"interface Required { required(): void }",
			r#"type FlameworkDecorator = ClassDecorator & { _flamework_Decorator: "Class" };"#,
			"/** @metadata reflect {@link Required constraint} */",
			"declare const Service: () => FlameworkDecorator;",
			"@Service() export class Invalid {}",
		],
	)
	.expect("AnalyzeFlameworkClasses");

	assert!(
		classes.len() == 1 && classes[0].decorator_ids.len() == 1 && diagnostics.len() == 1,
		"classes = {classes:?} diagnostics = {diagnostics:?}"
	);
	assert_eq!(
		diagnostics[0].0,
		"Type 'Invalid' does not satisfy the constraint 'Required'."
	);
	assert_eq!(diagnostics[0].1.len(), 1, "related = {:?}", diagnostics[0].1);
}
