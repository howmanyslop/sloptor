//! `transform_class_diagnostics.go`: class transform diagnostics and dependency cycles.

use std::collections::{BTreeMap, HashMap, HashSet};

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, get_source_file_of_node};
use rotor_tsrs::diagnostics as messages;

use crate::transform_diagnostics::{new_message_diagnostic, text_pos};
use crate::transform_state::TransformState;
use crate::{Error, ErrorDetail, ErrorKind};

/// `dependencyClass`
pub(crate) struct DependencyClass {
	pub(crate) internal_id: String,
	pub(crate) dependencies: Vec<String>,
}

/// `addClassTransformDiagnostic`: an invalid decorator becomes TS1206 on the class.
pub(crate) fn add_class_transform_diagnostic(state: &mut TransformState<'_>, node: P<Node>, error: &Error) -> bool {
	if !error.is(ErrorKind::InvalidDecorator) {
		return false;
	}
	state.add_diagnostic(new_message_diagnostic(
		get_source_file_of_node(node),
		text_pos(node.pos()),
		text_pos(node.end()),
		&messages::DECORATORS_ARE_NOT_VALID_HERE,
		&[],
	));
	true
}

/// `validateDependencyCycles`: the first constructor dependency cycle among the classes, in
/// internal-ID order.
pub(crate) fn validate_dependency_cycles(classes: &[DependencyClass]) -> Result<(), Error> {
	let mut graph: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
	for class in classes {
		let mut dependencies: Vec<&str> = class.dependencies.iter().map(String::as_str).collect();
		dependencies.sort_unstable();
		graph.insert(&class.internal_id, dependencies);
	}
	let mut ids: Vec<&str> = classes.iter().map(|class| class.internal_id.as_str()).collect();
	ids.sort_unstable();
	let mut visiting: HashMap<&str, usize> = HashMap::new();
	let mut visited: HashSet<&str> = HashSet::new();
	let mut path: Vec<&str> = Vec::new();
	for id in ids {
		visit(id, &graph, &mut visiting, &mut visited, &mut path)?;
	}
	Ok(())
}

fn visit<'g>(
	internal_id: &'g str,
	graph: &BTreeMap<&'g str, Vec<&'g str>>,
	visiting: &mut HashMap<&'g str, usize>,
	visited: &mut HashSet<&'g str>,
	path: &mut Vec<&'g str>,
) -> Result<(), Error> {
	if let Some(&index) = visiting.get(internal_id) {
		let mut cycle: Vec<String> = path[index..].iter().map(|id| (*id).to_owned()).collect();
		cycle.push(internal_id.to_owned());
		let message = format!("{}: {}", ErrorKind::CircularDependency.text(), cycle.join(" -> "));
		return Err(
			Error::with_detail(message, ErrorDetail::CircularDependency(cycle)).also(ErrorKind::CircularDependency)
		);
	}
	if visited.contains(internal_id) {
		return Ok(());
	}
	visiting.insert(internal_id, path.len());
	path.push(internal_id);
	if let Some(dependencies) = graph.get(internal_id) {
		for dependency in dependencies {
			if !graph.contains_key(dependency) {
				continue;
			}
			visit(dependency, graph, visiting, visited, path)?;
		}
	}
	path.pop();
	visiting.remove(internal_id);
	visited.insert(internal_id);
	Ok(())
}
