//! `transform_paths.go`: the `path` and `pathglob` intrinsics.

use rotor_rojo::RbxPath;
use rotor_support::gopath;
use rotor_support::gostr::quote;
use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, SourceFile, TokenFlags, get_source_file_of_node};
use rotor_tsrs::checker::Type;

use crate::Error;
use crate::path::project_relative_path;
use crate::transform_intrinsics::hash_text;
use crate::transform_macro_types::{alloc, invalid_macro, string_literal_value};
use crate::transform_state::TransformState;

/// `buildPathGlobIntrinsic`
pub(crate) fn build_path_glob_intrinsic(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	path_type: P<Type>,
) -> Result<P<Node>, Error> {
	if !path_type.is_string_literal() {
		let text = state.ck().type_to_string(path_type);
		return Err(invalid_macro(
			trace,
			format!("Path is invalid, expected string literal and got: {text}"),
		));
	}
	let glob = string_literal_value(path_type);
	let Some(file) = get_source_file_of_node(trace) else {
		return Err(invalid_macro(
			trace,
			format!("Could not resolve path glob {}", quote(&glob)),
		));
	};
	let Some(absolute_glob) = project_glob(state, file, &glob) else {
		return Err(invalid_macro(
			trace,
			format!("Could not resolve path glob {}", quote(&glob)),
		));
	};
	let file_id = project_relative_path(&state.project.root_directory, file.file_name())?;
	state.add_glob(&absolute_glob, &file_id);
	let value = hash_text(state, &absolute_glob, "addPaths", false)?;
	Ok(state.factory.new_string_literal(alloc(&value), TokenFlags::None))
}

/// `projectGlob`: a file-relative (`.`-prefixed) glob made project-relative.
pub(crate) fn project_glob(state: &TransformState<'_>, file: P<SourceFile>, glob: &str) -> Option<String> {
	if !glob.starts_with('.') {
		return Some(glob.to_owned());
	}
	let absolute = gopath::join(&[&gopath::dir(file.file_name()), &gopath::from_slash(glob)]);
	let relative = gopath::rel(&state.project.root_directory, &absolute).ok()?;
	Some(gopath::to_slash(&relative).into_owned())
}

/// `buildPathIntrinsic`
pub(crate) fn build_path_intrinsic(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	path_type: P<Type>,
) -> Result<P<Node>, Error> {
	if !path_type.is_string_literal() {
		let text = state.ck().type_to_string(path_type);
		return Err(invalid_macro(
			trace,
			format!("Path is invalid, expected string literal and got: {text}"),
		));
	}
	let value = string_literal_value(path_type);
	let Some(rbx_path) = path_intrinsic_rbx_path(state, &project_path_input(state, &value)) else {
		return Err(invalid_macro(trace, format!("Could not find Rojo data for '{value}'")));
	};
	let factory = &state.factory;
	let parts = rbx_path
		.iter()
		.map(|part| factory.new_string_literal(alloc(part), TokenFlags::None))
		.collect();
	let path_expression = factory.new_array_literal_expression(factory.new_node_list(parts), true);
	Ok(factory.new_array_literal_expression(factory.new_node_list(vec![path_expression]), true))
}

/// `projectPathInput`: a path intrinsic's input resolved against the project root.
pub(crate) fn project_path_input(state: &TransformState<'_>, input: &str) -> String {
	if gopath::is_abs(input) {
		return input.to_owned();
	}
	gopath::join(&[&state.project.root_directory, &gopath::from_slash(input)])
}

/// `pathIntrinsicRbxPath`
pub(crate) fn path_intrinsic_rbx_path(state: &TransformState<'_>, input: &str) -> Option<RbxPath> {
	state
		.project
		.resolver
		.get_rbx_path_from_file_path(&state.project.translator.get_output_path(input))
}
