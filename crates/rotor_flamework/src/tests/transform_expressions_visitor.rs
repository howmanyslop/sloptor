//! `transform_expressions_visitor_test.go`
//!
//! Go's guard builders are closures; Rust's are function pointers, so the callback log, the
//! fixture file and the nested results live in thread locals. Go's frame-stack checks
//! (`assertExpressionVisitorIdle`) and `testing.AllocsPerRun` count have no counterpart: the Rust
//! visitor keeps no frame stack between calls ([`crate::transform_expressions::map_children`]
//! records into locals), so the warmed-traversal test keeps its semantic assertions only.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, NodeFlags, SourceFile};
use rotor_tsrs::checker::Type;
use rotor_tsrs::diagnostics::Category;

use super::{find_expression_kind, find_node, with_expression_fixture};
use crate::Error;
use crate::project::FLAMEWORK_DIAGNOSTIC_CODE;
use crate::transform_diagnostics::Diagnostic;
use crate::transform_expressions::{ExpressionTransformResult, transform_flamework_expression_with_runtime};
use crate::transform_imports::FLAMEWORK_CORE_MODULE;
use crate::transform_macro_types::{GuardBuildResult, MacroError, MacroImport, MacroRelatedInformation, MacroRuntime};
use crate::transform_state::TransformState;

const EXPRESSION_VISITOR_MACROS: &str = r#"
type Generic<T, M extends string> = { _flamework_macro_generic: [T, M] };
declare function guardMacro<T>(value?: Generic<T, "guard">): unknown;
declare namespace Guards {
    function _use(value: unknown): unknown;
    /** @metadata macro {@link _use intrinsic-flamework-rewrite} */
    function use<T>(value?: Generic<T, "guard">): unknown;
}
declare namespace Other {
    function _use(value: unknown): unknown;
    /** @metadata macro {@link _use intrinsic-flamework-rewrite} */
    function use<T>(value?: Generic<T, "guard">): unknown;
}
"#;

thread_local! {
	static CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
	static FILE: Cell<Option<P<SourceFile>>> = const { Cell::new(None) };
	static INNER: RefCell<Option<ExpressionTransformResult>> = const { RefCell::new(None) };
	static HEALTHY: RefCell<Option<ExpressionTransformResult>> = const { RefCell::new(None) };
}

fn log_call(entry: String) {
	CALLS.with_borrow_mut(|calls| calls.push(entry));
}

fn take_calls() -> Vec<String> {
	CALLS.with_borrow_mut(std::mem::take)
}

fn fixture_file() -> P<SourceFile> {
	FILE.get().expect("fixture file")
}

/// `nestVisitorExpression`
fn nest_visitor_expression(expression: &str, depth: usize) -> String {
	let mut expression = expression.to_owned();
	for index in 0..depth {
		expression = if index % 2 == 0 {
			format!("[{expression}]")
		} else {
			format!("{{ nested: {expression} }}")
		};
	}
	expression
}

/// `expressionVisitorInitializer`
fn expression_visitor_initializer(file: P<SourceFile>, name: &str) -> P<Node> {
	find_node(file.as_node(), &|node| {
		node.kind() == Kind::VariableDeclaration && node.name().is_some_and(|declared| declared.text() == name)
	})
	.and_then(|declaration| declaration.initializer())
	.unwrap_or_else(|| panic!("initializer {name:?} not found"))
}

/// `expressionVisitorGuard`
fn expression_visitor_guard(state: &TransformState<'_>, label: &str) -> GuardBuildResult {
	let factory = &state.factory;
	let alloc = rotor_tsrs::tscore::alloc_str;
	GuardBuildResult {
		expression: Some(factory.new_property_access_expression(
			factory.new_identifier("t"),
			None,
			factory.new_identifier(alloc(&format!("g_{label}"))),
			NodeFlags::None,
		)),
		statements: vec![factory.new_expression_statement(factory.new_identifier(alloc(&format!("prepare_{label}"))))],
	}
}

fn label_of(state: &mut TransformState<'_>, target: P<Type>) -> String {
	state.ck().type_to_string(target).trim_matches('"').to_owned()
}

/// `assertExpressionVisitorText`: equal ignoring whitespace.
fn assert_text(state: &TransformState<'_>, file: P<SourceFile>, node: P<Node>, want: &str) {
	let got = rotor_tsrs::printer::new_printer(
		rotor_tsrs::printer::PrinterOptions::default(),
		rotor_tsrs::printer::PrintHandlers::default(),
		Some(state.emit_context),
	)
	.emit(node, Some(file));
	let squash = |text: &str| text.split_whitespace().collect::<String>();
	assert_eq!(squash(&got), squash(want), "printed expression:\n{got}\nwant:\n{want}");
}

/// `assertExpressionVisitorPrerequisites`
fn assert_prerequisites(
	state: &TransformState<'_>,
	file: P<SourceFile>,
	result: &ExpressionTransformResult,
	labels: &[&str],
) {
	assert_eq!(
		result.prerequisites.len(),
		labels.len(),
		"prerequisites, want labels {labels:?}"
	);
	for (prerequisite, label) in result.prerequisites.iter().zip(labels) {
		assert_text(state, file, *prerequisite, &format!("prepare_{label};"));
	}
}

/// `expressionVisitorParents`
fn expression_visitor_parents(root: P<Node>) -> Vec<(P<Node>, Option<P<Node>>)> {
	fn visit(node: P<Node>, parents: &mut Vec<(P<Node>, Option<P<Node>>)>) {
		parents.push((node, node.parent()));
		for child in node.iter_children() {
			visit(child, parents);
		}
	}
	let mut parents = Vec::new();
	visit(root, &mut parents);
	parents
}

/// `assertExpressionVisitorParents`
fn assert_parents(parents: &[(P<Node>, Option<P<Node>>)]) {
	for (node, parent) in parents {
		assert_eq!(node.parent(), *parent, "original {:?} parent changed", node.kind());
	}
}

/// `assertExpressionVisitorDiagnostic`
fn assert_diagnostic(
	diagnostic: &Diagnostic,
	file: P<SourceFile>,
	start: usize,
	end: usize,
	category: Category,
	message: &str,
) {
	let got = (
		diagnostic.file.map(|file| file.file_name().to_owned()),
		diagnostic.pos(),
		diagnostic.end(),
		diagnostic.category,
		diagnostic.string_code.clone(),
		diagnostic.string().to_owned(),
	);
	let want = (
		Some(file.file_name().to_owned()),
		i32::try_from(start).expect("start"),
		i32::try_from(end).expect("end"),
		category,
		Some(FLAMEWORK_DIAGNOSTIC_CODE.to_owned()),
		message.to_owned(),
	);
	assert_eq!(got, want, "diagnostic tuple");
}

#[expect(
	clippy::unnecessary_wraps,
	reason = "a GuardBuilder, whose signature returns a Result"
)]
fn recording_guard(state: &mut TransformState<'_>, _: P<Node>, target: P<Type>) -> Result<GuardBuildResult, Error> {
	let label = label_of(state, target);
	log_call(label.clone());
	Ok(expression_visitor_guard(state, &label))
}

fn runtime(build_guard: crate::transform_macro_types::GuardBuilder) -> MacroRuntime {
	MacroRuntime {
		build_guard: Some(build_guard),
		..MacroRuntime::default()
	}
}

fn macro_import(module: &str, name: &str) -> MacroImport {
	MacroImport {
		module: module.to_owned(),
		export: name.to_owned(),
		local: name.to_owned(),
	}
}

#[test]
fn flamework_expression_visitor_preserves_nested_results_across_growth_and_later_calls() {
	let deep = nest_visitor_expression(r#"Guards.use<"deep">()"#, 48);
	let expression = format!(r#"{{ left: Guards.use<"left">(), middle: {deep}, right: Other.use<"right">() }}"#);
	let source = format!(
		"{EXPRESSION_VISITOR_MACROS}const first = {expression};\n{}",
		r#"const later = [Other.use<"later">(), Other.use<"later">(), Other.use<"later">(), Other.use<"later">()];"#
	);
	with_expression_fixture(&source, |state, file| {
		state.guard_library = "@rbxts/t".to_owned();
		let parents = expression_visitor_parents(file.as_node());
		let runtime = runtime(recording_guard);
		let result =
			transform_flamework_expression_with_runtime(state, expression_visitor_initializer(file, "first"), &runtime)
				.expect("first");
		let want_expression = expression
			.replace(
				r#"Guards.use<"left">()"#,
				r#"Guards["_use"]<"left">(t.g_left as never)"#,
			)
			.replace(
				r#"Guards.use<"deep">()"#,
				r#"Guards["_use"]<"deep">(t.g_deep as never)"#,
			)
			.replace(
				r#"Other.use<"right">()"#,
				r#"Other["_use"]<"right">(t.g_right as never)"#,
			);
		assert_text(state, file, result.expression, &want_expression);
		assert_prerequisites(state, file, &result, &["left", "deep", "right"]);
		let want_imports = [
			macro_import(FLAMEWORK_CORE_MODULE, "Guards"),
			macro_import("@rbxts/t", "t"),
			macro_import(FLAMEWORK_CORE_MODULE, "Guards"),
			macro_import("@rbxts/t", "t"),
			macro_import(FLAMEWORK_CORE_MODULE, "Other"),
			macro_import("@rbxts/t", "t"),
		];
		assert_eq!(result.imports, want_imports);
		assert_eq!(take_calls(), ["left", "deep", "right"]);
		let held_prerequisites = result.prerequisites.clone();
		let held_imports = result.imports.clone();
		for _ in 0..3 {
			let later = transform_flamework_expression_with_runtime(
				state,
				expression_visitor_initializer(file, "later"),
				&runtime,
			)
			.expect("later");
			assert!(
				later.prerequisites.len() == 4 && later.imports.len() == 8,
				"later prerequisites={} imports={}",
				later.prerequisites.len(),
				later.imports.len()
			);
		}
		assert!(
			result.prerequisites == held_prerequisites && result.imports == held_imports,
			"later invocation overwrote retained result slices"
		);
		assert_text(state, file, result.expression, &want_expression);
		assert_prerequisites(state, file, &result, &["left", "deep", "right"]);
		assert_parents(&parents);
		assert!(state.diagnostics.is_empty(), "diagnostics={:?}", state.diagnostics);
	});
}

fn failing_default_guard(_: &mut TransformState<'_>, _: P<Node>, _: P<Type>) -> Result<GuardBuildResult, Error> {
	panic!("replaced the supplied runtime with the state's default");
}

fn fatal_guard(state: &mut TransformState<'_>, _: P<Node>, target: P<Type>) -> Result<GuardBuildResult, Error> {
	let label = label_of(state, target);
	log_call(format!("fatal:{label}"));
	if label == "fatal" {
		return Err(Error::msg("visitor fatal"));
	}
	Ok(expression_visitor_guard(state, &label))
}

fn inner_guard(state: &mut TransformState<'_>, trace: P<Node>, target: P<Type>) -> Result<GuardBuildResult, Error> {
	let label = label_of(state, target);
	log_call(format!("inner:{label}"));
	if label == "captured" {
		return Err(Error::with_detail(
			"captured visitor diagnostic",
			crate::ErrorDetail::Macro(MacroError {
				node: Some(trace),
				message: "captured visitor diagnostic".to_owned(),
				related_information: vec![MacroRelatedInformation {
					node: Some(trace),
					message: "related visitor diagnostic".to_owned(),
				}],
			}),
		)
		.also(crate::ErrorKind::MacroDiagnostic));
	}
	if label == "inner" {
		let file = fixture_file();
		let failed = transform_flamework_expression_with_runtime(
			state,
			expression_visitor_initializer(file, "failing"),
			&runtime(fatal_guard),
		);
		assert!(
			failed.as_ref().is_err_and(|error| error.message() == "visitor fatal"),
			"fatal result is_ok={}",
			failed.is_ok()
		);
		let healthy = transform_flamework_expression_with_runtime(
			state,
			expression_visitor_initializer(file, "healthy"),
			&runtime(inner_guard),
		)
		.expect("healthy");
		HEALTHY.with_borrow_mut(|slot| *slot = Some(healthy));
	}
	Ok(expression_visitor_guard(state, &format!("inner_{label}")))
}

#[expect(
	clippy::unnecessary_wraps,
	reason = "a GuardBuilder, whose signature returns a Result"
)]
fn outer_guard(state: &mut TransformState<'_>, _: P<Node>, target: P<Type>) -> Result<GuardBuildResult, Error> {
	let label = label_of(state, target);
	log_call(format!("outer:{label}"));
	if label == "reenter" {
		let inner = transform_flamework_expression_with_runtime(
			state,
			expression_visitor_initializer(fixture_file(), "inner"),
			&runtime(inner_guard),
		)
		.expect("inner");
		INNER.with_borrow_mut(|slot| *slot = Some(inner));
	}
	Ok(expression_visitor_guard(state, &format!("outer_{label}")))
}

#[test]
fn flamework_expression_visitor_isolates_reentrant_runtimes_and_errors() {
	let inner_expression = nest_visitor_expression(r#"[guardMacro<"captured">(), guardMacro<"inner">()]"#, 64);
	let source = format!(
		r#"{EXPRESSION_VISITOR_MACROS}
const outer = [guardMacro<"before">(), guardMacro<"reenter">(), guardMacro<"after">()];
const inner = {inner_expression};
const failing = [guardMacro<"fatal_before">(), guardMacro<"fatal">(), guardMacro<"skipped">()];
const healthy = [guardMacro<"healthy">()];
"#
	);
	with_expression_fixture(&source, |state, file| {
		FILE.set(Some(file));
		state.guard_library = "@rbxts/t".to_owned();
		state.macro_runtime = runtime(failing_default_guard);
		let parents = expression_visitor_parents(file.as_node());
		let outer = transform_flamework_expression_with_runtime(
			state,
			expression_visitor_initializer(file, "outer"),
			&runtime(outer_guard),
		)
		.expect("outer");
		let inner = INNER.with_borrow_mut(Option::take).expect("inner result");
		let healthy = HEALTHY.with_borrow_mut(Option::take).expect("healthy result");

		assert_eq!(
			take_calls(),
			[
				"outer:before",
				"outer:reenter",
				"inner:captured",
				"inner:inner",
				"fatal:fatal_before",
				"fatal:fatal",
				"inner:healthy",
				"outer:after"
			]
		);
		assert_text(
			state,
			file,
			outer.expression,
			r#"[guardMacro<"before">(t.g_outer_before as never), guardMacro<"reenter">(t.g_outer_reenter as never), guardMacro<"after">(t.g_outer_after as never)]"#,
		);
		assert_text(
			state,
			file,
			inner.expression,
			&inner_expression.replacen(
				r#"guardMacro<"inner">()"#,
				r#"guardMacro<"inner">(t.g_inner_inner as never)"#,
				1,
			),
		);
		assert_text(
			state,
			file,
			healthy.expression,
			r#"[guardMacro<"healthy">(t.g_inner_healthy as never)]"#,
		);
		assert_prerequisites(state, file, &outer, &["outer_before", "outer_reenter", "outer_after"]);
		assert_prerequisites(state, file, &inner, &["inner_inner"]);
		assert_prerequisites(state, file, &healthy, &["inner_healthy"]);
		for result in [&outer, &inner, &healthy] {
			assert_eq!(
				result.imports.len(),
				result.prerequisites.len(),
				"imports leaked across invocations"
			);
			for imported in &result.imports {
				assert_eq!(*imported, macro_import("@rbxts/t", "t"), "unexpected import");
			}
		}
		let ordered = state.ordered_diagnostics();
		assert!(
			ordered.len() == 1 && ordered[0].related_information.len() == 1,
			"diagnostics={ordered:?}"
		);
		let call = r#"guardMacro<"captured">()"#;
		let start = source.find(call).expect("captured call");
		assert_diagnostic(
			&ordered[0],
			file,
			start,
			start + call.len(),
			Category::Error,
			"captured visitor diagnostic",
		);
		assert_diagnostic(
			&ordered[0].related_information[0],
			file,
			start,
			start + call.len(),
			Category::Message,
			"related visitor diagnostic",
		);
		assert_parents(&parents);
	});
}

#[derive(Debug)]
struct VisitorPanic;

fn panic_guard(_: &mut TransformState<'_>, _: P<Node>, _: P<Type>) -> Result<GuardBuildResult, Error> {
	std::panic::panic_any(VisitorPanic);
}

/// Runs the panicking transform and checks the original panic surfaced.
fn invoke_panic(state: &mut TransformState<'_>, panicking: P<Node>) {
	let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		let _ = transform_flamework_expression_with_runtime(state, panicking, &runtime(panic_guard));
	}));
	let payload = outcome.expect_err("transform swallowed the runtime panic");
	assert!(payload.is::<VisitorPanic>(), "panic payload is not the original panic");
}

thread_local! {
	static PANICKING: Cell<Option<P<Node>>> = const { Cell::new(None) };
}

#[expect(
	clippy::unnecessary_wraps,
	reason = "a GuardBuilder, whose signature returns a Result"
)]
fn reentering_panic_guard(
	state: &mut TransformState<'_>,
	_: P<Node>,
	target: P<Type>,
) -> Result<GuardBuildResult, Error> {
	let label = label_of(state, target);
	log_call(label.clone());
	if label == "reenter" {
		invoke_panic(state, PANICKING.get().expect("panicking node"));
	}
	Ok(expression_visitor_guard(state, &label))
}

#[test]
fn flamework_expression_visitor_restores_suspended_and_top_level_frames_after_panic() {
	let source = format!(
		"{EXPRESSION_VISITOR_MACROS}\nconst outer = [guardMacro<\"reenter\">(), guardMacro<\"after\">()];\nconst panicking = {};\n",
		nest_visitor_expression(r#"guardMacro<"panic">()"#, 48)
	);
	let hook = std::panic::take_hook();
	std::panic::set_hook(Box::new(|_| {}));
	with_expression_fixture(&source, |state, file| {
		state.guard_library = "@rbxts/t".to_owned();
		let parents = expression_visitor_parents(file.as_node());
		let panicking = expression_visitor_initializer(file, "panicking");
		PANICKING.set(Some(panicking));
		for _ in 0..2 {
			invoke_panic(state, panicking);
			let result = transform_flamework_expression_with_runtime(
				state,
				expression_visitor_initializer(file, "outer"),
				&runtime(reentering_panic_guard),
			)
			.expect("outer");
			assert_text(
				state,
				file,
				result.expression,
				r#"[guardMacro<"reenter">(t.g_reenter as never), guardMacro<"after">(t.g_after as never)]"#,
			);
			assert_prerequisites(state, file, &result, &["reenter", "after"]);
			assert!(
				result.imports.len() == 2 && state.diagnostics.is_empty(),
				"panic contaminated imports={:?} diagnostics={:?}",
				result.imports,
				state.diagnostics
			);
		}
		assert_eq!(take_calls(), ["reenter", "after", "reenter", "after"]);
		assert_parents(&parents);
	});
	std::panic::set_hook(hook);
}

#[test]
fn flamework_expression_visitor_warmed_unchanged_traversal_allocations() {
	let shapes = [
		("small", "[1]".to_owned()),
		(
			"wide",
			format!("[{}]", "{ value: [1, 2] },".repeat(64).trim_end_matches(',')),
		),
		("deep", nest_visitor_expression("1", 64)),
	];
	let source = shapes.iter().fold(String::new(), |mut source, (name, expression)| {
		let _ = writeln!(source, "const {name} = {expression};");
		source
	});
	with_expression_fixture(&source, |state, file| {
		let leaf = find_expression_kind(file.as_node(), Kind::NumericLiteral);
		let result = transform_flamework_expression_with_runtime(state, leaf, &MacroRuntime::default()).expect("leaf");
		assert_eq!(result.expression, leaf, "leaf changed");
		for (name, shape) in &shapes {
			let expression = expression_visitor_initializer(file, name);
			let parents = expression_visitor_parents(expression);
			for _ in 0..20 {
				let result = transform_flamework_expression_with_runtime(state, expression, &MacroRuntime::default())
					.expect(name);
				assert!(
					result.expression == expression && result.prerequisites.is_empty() && result.imports.is_empty(),
					"{name}: unchanged traversal changed the expression"
				);
			}
			assert_text(state, file, expression, shape);
			assert_parents(&parents);
		}
		assert!(
			state.diagnostics.is_empty(),
			"unchanged traversal diagnostics={:?}",
			state.diagnostics
		);
	});
}
