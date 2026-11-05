//! `transform_decorators_imports.go`: the guard library import decorators need.

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, SourceFile, is_identifier};

use crate::transform_guard_library::flamework_guard_library;
use crate::transform_macro_types::MacroImport;
use crate::transform_state::TransformState;

pub(crate) const FLAMEWORK_PRELUDE_MODULE: &str = "@flamework/core/out/prelude";

/// `decoratorRuntimeImports`: `imports`, plus `t` when any expression or prerequisite uses it.
pub(crate) fn decorator_runtime_imports(
	state: &mut TransformState<'_>,
	file: Option<P<SourceFile>>,
	expressions: &[P<Node>],
	prerequisites: &[P<Node>],
	mut imports: Vec<MacroImport>,
) -> Vec<MacroImport> {
	let uses_t = expressions
		.iter()
		.chain(prerequisites)
		.any(|node| node_contains_identifier(Some(*node), "t"));
	if uses_t && let Some(file) = file {
		let module = flamework_guard_library(state, file);
		imports.push(MacroImport {
			module,
			export: "t".to_owned(),
			local: "t".to_owned(),
		});
	}
	imports
}

/// `nodeContainsIdentifier`
pub(crate) fn node_contains_identifier(node: Option<P<Node>>, name: &str) -> bool {
	let Some(node) = node else {
		return false;
	};
	if is_identifier(node) && node.text() == name {
		return true;
	}
	node.iter_children()
		.into_iter()
		.any(|child| node_contains_identifier(Some(child), name))
}
