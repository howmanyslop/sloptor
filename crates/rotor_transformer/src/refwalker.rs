//! `refwalker.go`: the `ts.FindAllReferences.Core` internals the transformer consumes, scoped to
//! its single-file callers (`checkVariableHoist`, `transformForStatement`).

use rotor_tsrs::ast as tsast;
use rotor_tsrs::checker::{Checker, TypeChecker};

use crate::prelude::*;
use crate::util::is_identifier;

/// `ForEachSymbolReference`: walks `search_container` in document order, calling `on_ref` on each
/// identifier that references `definition`'s symbol. `on_ref` returning true ends the walk; the
/// result says whether one did.
///
/// The definition itself is skipped; a text prefilter runs before symbol resolution; matches
/// are symbol identity or the shorthand-property value symbol. Export specifiers resolved
/// through their local symbol are deliberately omitted (Go's documented divergence).
pub fn for_each_symbol_reference(
	checker: &mut Checker,
	definition: P<Node>,
	search_container: P<Node>,
	on_ref: &mut dyn FnMut(P<Node>) -> bool,
) -> bool {
	let mut checker = TypeChecker::new(checker);
	let Some(symbol) = checker.get_symbol_at_location(definition) else {
		return false;
	};
	let name = definition.text();
	visit(&mut checker, search_container, definition, name, symbol, on_ref)
}

fn visit(
	checker: &mut TypeChecker<'_>,
	node: P<Node>,
	definition: P<Node>,
	name: &str,
	symbol: P<Symbol>,
	on_ref: &mut dyn FnMut(P<Node>) -> bool,
) -> bool {
	if is_identifier(node) {
		if node == definition || node.text() != name {
			return false;
		}
		let mut matches = checker.get_symbol_at_location(node) == Some(symbol);
		if !matches
			&& let Some(parent) = node.parent()
			&& tsast::is_shorthand_property_assignment(parent)
		{
			matches = checker.get_shorthand_assignment_value_symbol(Some(parent)) == Some(symbol);
		}
		if matches {
			return on_ref(node);
		}
		return false;
	}
	node.for_each_child(&mut |child| visit(checker, child, definition, name, symbol, on_ref))
}

/// `IsSymbolReferenced`: `definition`'s symbol has a reference besides the definition inside
/// `search_container`.
pub fn is_symbol_referenced(checker: &mut Checker, definition: P<Node>, search_container: P<Node>) -> bool {
	for_each_symbol_reference(checker, definition, search_container, &mut |_| true)
}
