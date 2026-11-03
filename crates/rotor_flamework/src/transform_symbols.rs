//! `transform_symbols.go`: declaration UIDs, the `symbol-id` and `declaration-uid` intrinsics.

use rotor_support::gopath;
use rotor_support::gostr::quote;
use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Node, Symbol, SymbolFlags, TokenFlags, find_ancestor, get_name_of_declaration, get_source_file_of_node,
	is_declaration, is_identifier, is_source_file, is_type_query_node, is_type_reference_node,
};
use rotor_tsrs::checker::{Type, TypeFlags};

use crate::Error;
use crate::build_info::BuildInfo;
use crate::project_discovery::{find_package_json, package_identity};
use crate::state::{DeclarationIdentity, FLAMEWORK_VERSION};
use crate::transform::lua_file_name;
use crate::transform_macro_types::{alloc, invalid_macro, string_literal_value};
use crate::transform_state::TransformState;

/// `buildSymbolIDIntrinsic`
pub(crate) fn build_symbol_id_intrinsic(
	state: &mut TransformState<'_>,
	call: P<Node>,
	target: P<Type>,
) -> Result<P<Node>, Error> {
	let id = match call.type_arguments().first() {
		Some(first) => node_uid(state, *first)?,
		None => type_uid(state, target, call)?,
	};
	Ok(state.factory.new_string_literal(alloc(&id), TokenFlags::None))
}

/// `buildDeclarationUIDIntrinsic`
pub(crate) fn build_declaration_uid_intrinsic(
	state: &mut TransformState<'_>,
	trace: P<Node>,
) -> Result<P<Node>, Error> {
	let declaration = find_ancestor(Some(trace), |node| {
		is_declaration(node) && get_name_of_declaration(node).is_some()
	});
	let Some(declaration) = declaration else {
		return Err(invalid_macro(
			trace,
			"This function must be under a variable declaration.",
		));
	};
	let id = node_uid(state, declaration)?;
	Ok(state.factory.new_string_literal(alloc(&id), TokenFlags::None))
}

/// `nodeUID`
pub(crate) fn node_uid(state: &mut TransformState<'_>, node: P<Node>) -> Result<String, Error> {
	if is_type_reference_node(node) {
		return node_uid(state, node.as_type_reference_node().type_name);
	}
	if is_type_query_node(node) {
		return node_uid(state, node.as_type_query_node().expr_name);
	}
	if let Some(declaration) = declaration_from_node(state, node) {
		return declaration_uid(state, declaration);
	}
	let target = state.ck().get_type_at_location(node);
	type_uid(state, target, node)
}

/// `typeUID`
pub(crate) fn type_uid(state: &mut TransformState<'_>, target: P<Type>, trace: P<Node>) -> Result<String, Error> {
	if let Some(symbol) = target.symbol() {
		return symbol_uid(state, symbol, trace);
	}
	if target.is_string_literal() {
		return Ok(format!("$ps:{}", string_literal_value(target)));
	}
	if target.is_number_literal() {
		return Ok(format!("$pn:{}", literal_type_string(target)));
	}
	if target.flags().intersects(TypeFlags::Intrinsic) {
		return Ok(format!("$p:{}", target.as_intrinsic_type().intrinsic_name()));
	}
	if target
		.flags()
		.intersects(TypeFlags::Object | TypeFlags::Intersection | TypeFlags::Union)
	{
		return Ok("$p:defined".to_owned());
	}
	let text = state.ck().type_to_string(target);
	Err(invalid_macro(
		trace,
		format!("Could not find UID for type {}", quote(&text)),
	))
}

/// `LiteralType.String()`
pub(crate) fn literal_type_string(target: P<Type>) -> String {
	match target.as_literal_type().value() {
		Some(rotor_tsrs::checker::LiteralValue::Number(number)) => number.to_string(),
		Some(rotor_tsrs::checker::LiteralValue::String(text)) => text.to_owned(),
		Some(rotor_tsrs::checker::LiteralValue::Boolean(value)) => value.to_string(),
		Some(rotor_tsrs::checker::LiteralValue::BigInt(big)) => {
			format!("{}{}n", if big.negative { "-" } else { "" }, big.base10_value)
		}
		None => String::new(),
	}
}

fn symbol_uid(state: &mut TransformState<'_>, mut symbol: P<Symbol>, trace: P<Node>) -> Result<String, Error> {
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	let declaration = symbol
		.value_declaration()
		.or_else(|| symbol.declarations().first().copied());
	let Some(declaration) = declaration else {
		return Err(invalid_macro(
			trace,
			format!("Could not find UID for symbol {}", quote(symbol.name())),
		));
	};
	declaration_uid(state, declaration)
}

/// `declarationUID`
pub(crate) fn declaration_uid(state: &mut TransformState<'_>, declaration: P<Node>) -> Result<String, Error> {
	let name = declaration_full_name(declaration);
	if name.is_empty() {
		return Err(invalid_macro(declaration, "Could not determine declaration name"));
	}
	let file_name = get_source_file_of_node(declaration).map_or_else(String::new, |file| file.file_name().to_owned());
	if !path_within(&state.project.translator.root_dir, &file_name) {
		return package_declaration_uid(state, declaration, &name);
	}
	let output = state.project.translator.get_output_path(&file_name);
	let relative = gopath::rel(&state.project.root_directory, &output)
		.map_err(|error| Error::msg(format!("resolve Flamework declaration path: {error}")))?;
	let relative = trim_extension(&gopath::to_slash(&relative), &relative);
	let internal_id = format!("{}:{relative}@{name}", state.project.package_name);
	state.identifier(&DeclarationIdentity {
		internal_id,
		declaration_name: name,
		lua_file_name: lua_file_name(&file_name),
		..DeclarationIdentity::default()
	})
}

/// `strings.TrimSuffix(slashed, filepath.Ext(native))`
pub(crate) fn trim_extension(slashed: &str, native: &str) -> String {
	let base = gopath::base(native);
	let extension = base.rfind('.').map_or("", |dot| &base[dot..]);
	slashed.strip_suffix(extension).unwrap_or(slashed).to_owned()
}

fn package_declaration_uid(state: &mut TransformState<'_>, declaration: P<Node>, name: &str) -> Result<String, Error> {
	let file_name = get_source_file_of_node(declaration).map_or_else(String::new, |file| file.file_name().to_owned());
	let package_json = find_package_json(&gopath::dir(&file_name));
	if package_json.is_empty() {
		return Err(invalid_macro(
			declaration,
			format!("Could not find package for declaration {}", quote(name)),
		));
	}
	let data = std::fs::read(&package_json)
		.map_err(|error| Error::io("open", &package_json, &error).wrap("read Flamework declaration package"))?;
	let Some((package_name, _)) = package_identity(&data).filter(|(package_name, _)| !package_name.is_empty()) else {
		return Err(invalid_macro(
			declaration,
			format!("Could not read package identity for declaration {}", quote(name)),
		));
	};
	let internal_id = format!(
		"{package_name}:{}@{name}",
		package_relative_module(&package_json, &file_name)?
	);
	let mut package_prefix = String::new();
	let build_path = gopath::join(&[&gopath::dir(&package_json), "flamework.build"]);
	if std::fs::metadata(&build_path).is_ok() {
		let info = BuildInfo::load(&build_path, FLAMEWORK_VERSION)
			.map_err(|error| error.wrap("load packaged Flamework declaration metadata"))?;
		info.snapshot()
			.identifier_prefix()
			.unwrap_or_default()
			.clone_into(&mut package_prefix);
	}
	state.identifier(&DeclarationIdentity {
		internal_id,
		declaration_name: name.to_owned(),
		lua_file_name: lua_file_name(&file_name),
		package_prefix,
		is_package: true,
	})
}

/// A packaged file's module path relative to its `package.json`: no `.ts`/`.d`, `index` as
/// `init`.
pub(crate) fn package_relative_module(package_json: &str, file_name: &str) -> Result<String, Error> {
	let relative = gopath::rel(&gopath::dir(package_json), file_name)
		.map_err(|error| Error::msg(format!("resolve packaged Flamework declaration path: {error}")))?;
	let slashed = gopath::to_slash(&relative);
	let without_ts = slashed.strip_suffix(".ts").unwrap_or(&slashed);
	let mut relative = without_ts.strip_suffix(".d").unwrap_or(without_ts).to_owned();
	if let Some(stripped) = relative.strip_suffix("/index") {
		relative = format!("{stripped}/init");
	}
	Ok(relative)
}

/// `pathWithin`
pub(crate) fn path_within(root: &str, candidate: &str) -> bool {
	gopath::rel(root, candidate).is_ok_and(|relative| {
		relative != ".."
			&& !relative.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR))
			&& !gopath::is_abs(&relative)
	})
}

/// `declarationFromNode`: the node itself when it is a named declaration, else its symbol's
/// (aliases resolved) value or first declaration.
pub(crate) fn declaration_from_node(state: &mut TransformState<'_>, node: P<Node>) -> Option<P<Node>> {
	if is_declaration(node) && get_name_of_declaration(node).is_some() {
		return Some(node);
	}
	let mut symbol = state.ck().get_symbol_at_location(node)?;
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	symbol
		.value_declaration()
		.or_else(|| symbol.declarations().first().copied())
}

/// `declarationFullName`: the dotted names of the declaration and its named ancestors.
pub(crate) fn declaration_full_name(declaration: P<Node>) -> String {
	let mut parts = Vec::with_capacity(2);
	let mut current = Some(declaration);
	while let Some(node) = current {
		if is_source_file(node) {
			break;
		}
		if let Some(name) = get_name_of_declaration(node)
			&& is_identifier(name)
		{
			parts.push(name.text());
		}
		current = node.parent();
	}
	parts.reverse();
	parts.join(".")
}
