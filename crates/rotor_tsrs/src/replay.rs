//! Replays the spike's checker queries over a program, the way rotor will drive the checker.
//!
//! The basic walk asks for the type and symbol of every identifier and call. The rotor-API walk
//! exercises the entry points that `tools/tsrs-mirror/patches/0001-visibility.patch` makes public.
//! Both count what they saw; any panic inside a file is caught and counted, not propagated.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Mutex;

use tsrs_ast::{Kind, Node, SourceFile, SymbolFlags};
use tsrs_checker::{Checker, ContextFlags, ObjectFlags, SignatureKind, TypeFlags};
use tsrs_compiler::Program;
use tsrs_core::P;

/// Named counters, merged across checker threads.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Counts(BTreeMap<&'static str, usize>);

impl Counts {
	fn add(&mut self, name: &'static str, amount: usize) {
		*self.0.entry(name).or_default() += amount;
	}

	fn merge(&mut self, other: Self) {
		for (name, amount) in other.0 {
			self.add(name, amount);
		}
	}

	/// The counter called `name`, or zero if it never moved.
	#[must_use]
	pub fn get(&self, name: &str) -> usize {
		self.0.get(name).copied().unwrap_or(0)
	}

	/// Every counter that moved, sorted by name.
	pub fn iter(&self) -> impl Iterator<Item = (&'static str, usize)> {
		self.0.iter().map(|(name, amount)| (*name, *amount))
	}
}

/// The source files the spike walked: the ones rotor compiles ([`crate::project::project_source_files`]).
#[must_use]
pub fn project_files(program: &Program) -> Vec<P<SourceFile>> {
	crate::project::project_source_files(program)
}

fn walk(node: P<Node>, visit: &mut dyn FnMut(P<Node>)) {
	visit(node);
	node.for_each_child(&mut |child| {
		walk(child, &mut *visit);
		false
	});
}

/// Runs `body` over every file on the checker pool, merging each file's counts. A file whose walk
/// panics is recorded as `panicked_files` and its partial counts are dropped.
fn replay(
	program: &Program,
	files: &[P<SourceFile>],
	body: impl Fn(&mut Checker, P<SourceFile>, &mut Counts) + Sync,
) -> Option<Counts> {
	let merged = Mutex::new(Counts::default());
	let ran = program.for_each_checker_group(files, |checker, _index, file| {
		let mut counts = Counts::default();
		if catch_unwind(AssertUnwindSafe(|| body(checker, file, &mut counts))).is_err() {
			counts = Counts::default();
			counts.add("panicked_files", 1);
		}
		counts.add("files", 1);
		if let Ok(mut total) = merged.lock() {
			total.merge(counts);
		}
	});
	ran.then(|| merged.into_inner().unwrap_or_default())
}

/// Walks every identifier and call: `getTypeAtLocation`, flag reads, `getSymbolAtLocation`, and a
/// `typeToString` every thousandth query. Returns `None` if the program has no built-in checker pool.
#[must_use]
pub fn replay_basic(program: &Program, files: &[P<SourceFile>]) -> Option<Counts> {
	replay(program, files, |checker, file, counts| {
		let mut queried = 0_usize;
		walk(file.as_node(), &mut |node| {
			counts.add("nodes", 1);
			let kind = node.kind();
			if kind != Kind::Identifier && kind != Kind::CallExpression {
				return;
			}
			queried += 1;
			let node_type = checker.get_type_at_location(node);
			counts.add("type_queries", 1);
			let flags = node_type.flags();
			if flags.intersects(TypeFlags::Any) {
				counts.add("any_types", 1);
			}
			if flags.intersects(TypeFlags::Never | TypeFlags::Unknown) {
				counts.add("never_or_unknown", 1);
			}
			counts.add("symbol_queries", 1);
			if checker.get_symbol_at_location_exported(node).is_some() {
				counts.add("symbols_found", 1);
			}
			if queried.is_multiple_of(1000) {
				counts.add("type_to_string_bytes", checker.type_to_string_exported(node_type).len());
				counts.add("type_to_string", 1);
			}
		});
	})
}

/// Exercises the rotor-specific checker entry points.
///
/// That covers signatures, assignment patterns, index types, conditional and tuple types, alias
/// resolution, contextual types, base types and the emit resolver, so the visibility patch is
/// compiled and run, not just written.
#[must_use]
pub fn replay_rotor_apis(program: &Program, files: &[P<SourceFile>]) -> Option<Counts> {
	replay(program, files, |checker, file, counts| {
		let number = checker.get_number_type();
		let resolver = checker.get_emit_resolver();
		if Path::new(file.file_name())
			.extension()
			.is_some_and(|extension| extension.eq_ignore_ascii_case("tsx"))
		{
			let _ = resolver.get_jsx_factory_entity(checker, file.as_node());
			counts.add("jsx_factory", 1);
		}
		walk(file.as_node(), &mut |node| match node.kind() {
			Kind::CallExpression | Kind::NewExpression => {
				if let Some(callee) = node.expression() {
					let callee_type = checker.get_type_at_location(callee);
					counts.add(
						"construct_signatures",
						checker
							.get_signatures_of_type(callee_type, SignatureKind::Construct)
							.len(),
					);
					counts.add("call_signatures", checker.get_call_signatures(callee_type).len());
					let _ = checker.get_resolved_signature_exported(node);
					counts.add("resolved_signatures", 1);
				}
			}
			Kind::ArrayLiteralExpression | Kind::ObjectLiteralExpression => {
				if let Some(parent) = node.parent()
					&& parent.kind() == Kind::BinaryExpression
				{
					let binary = parent.as_binary_expression();
					if binary.operator_token.kind() == Kind::EqualsToken && binary.left == node {
						let _ = checker.get_type_of_assignment_pattern(node);
						counts.add("assignment_patterns", 1);
					}
				}
				if node.kind() == Kind::ObjectLiteralExpression {
					let _ = checker.get_contextual_type_exported(node, ContextFlags::None);
					counts.add("contextual_types", 1);
				}
			}
			Kind::ElementAccessExpression => {
				if let Some(expression) = node.expression() {
					let object_type = checker.get_type_at_location(expression);
					let _ = checker.get_index_type_of_type(object_type, number);
					counts.add("index_types", 1);
				}
			}
			Kind::ImportDeclaration | Kind::ImportSpecifier | Kind::ImportEqualsDeclaration | Kind::ImportClause => {
				counts.add("alias_declarations", 1);
				if resolver.is_referenced_alias_declaration(checker, node) {
					counts.add("alias_referenced", 1);
				}
			}
			Kind::Identifier => identifier_queries(checker, node, counts),
			_ => {}
		});
	})
}

fn identifier_queries(checker: &mut Checker, node: P<Node>, counts: &mut Counts) {
	let node_type = checker.get_type_at_location(node);
	let flags = node_type.flags();
	if flags.intersects(TypeFlags::Conditional)
		&& let Some(root) = node_type.as_conditional_type().root.get()
	{
		let _ = root.check_type.get();
		let _ = root.node.get();
		counts.add("conditional_roots", 1);
	}
	if tsrs_checker::is_tuple_type_exported(node_type) {
		let _ = node_type.target_tuple_type().fixed_length();
		counts.add("tuple_targets", 1);
	}
	if flags.intersects(TypeFlags::Object) {
		let apparent = checker.get_apparent_type_exported(node_type);
		counts.add("apparent_properties", checker.get_properties_of_type(apparent).len());
		if node_type.object_flags().intersects(ObjectFlags::ClassOrInterface) {
			counts.add("base_types", checker.get_base_types_exported(node_type).len());
		}
	}
	if flags.intersects(TypeFlags::Union) {
		let _ = checker.get_non_nullable_type(node_type);
		counts.add("non_nullable", 1);
	}
	if let Some(symbol) = checker.get_symbol_at_location_exported(node)
		&& symbol.flags().intersects(SymbolFlags::Alias)
	{
		let _ = checker.get_aliased_symbol(symbol);
		counts.add("aliased_symbols", 1);
	}
}
