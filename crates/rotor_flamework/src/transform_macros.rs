//! `transform_macros.go`: rewriting a call to a Flamework macro.

use std::collections::BTreeMap;

use rotor_tsrs::P;
use rotor_tsrs::ast::{Node, NodeFlags, Symbol, TokenFlags, is_call_expression, is_new_expression};

use crate::Error;
use crate::ErrorKind;
use crate::transform_imports::FLAMEWORK_CORE_MODULE;
use crate::transform_inlining::inline_macro_intrinsic;
use crate::transform_intrinsics::{build_intrinsic_macro, build_many_macro};
use crate::transform_macro_resolution::user_macro_of_union;
use crate::transform_macro_types::build_caller_macro as transform_macro_types_build_caller;
use crate::transform_macro_types::{
	MacroImport, MacroKind, MacroMetadata, MacroRuntime, MacroTransformResult, UserMacro, alloc, as_never,
	invalid_macro, is_undefined_expression, literal_expression, read_macro_metadata,
};
use crate::transform_network::transform_networking_middleware_intrinsic;
use crate::transform_parameters::{macro_parameter_count, validate_parameter_const_intrinsic};
use crate::transform_state::TransformState;
use crate::transform_symbols::type_uid;

/// `transformFlameworkCall`
pub(crate) fn transform_flamework_call(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<MacroTransformResult, Error> {
	if !is_call_expression(node) && !is_new_expression(node) {
		return Err(invalid_macro(
			node,
			"Flamework macro target must be a call or new expression",
		));
	}
	let signature = state.ck().get_resolved_signature(node);
	let mut arguments: Vec<P<Node>> = node.arguments().to_vec();
	let mut highest: Option<usize> = None;
	let mut macros: BTreeMap<usize, UserMacro> = BTreeMap::new();
	for index in 0..macro_parameter_count(state, signature) {
		if index < arguments.len() && !is_undefined_expression(arguments[index]) {
			continue;
		}
		let position_type = state.ck().get_type_at_position(signature, index);
		let target = state.ck().get_non_nullable_type(position_type);
		if let Some(found) = user_macro_of_union(state, node, target)? {
			macros.insert(index, found);
			highest = Some(index);
		}
	}

	let mut prerequisites = Vec::new();
	if let Some(highest) = highest {
		for index in 0..=highest {
			let Some(found) = macros.get(&index).cloned() else {
				if index >= arguments.len() {
					arguments.push(state.factory.new_identifier("undefined"));
				}
				continue;
			};
			let (expression, statements) = build_user_macro(state, node, &found, runtime)?;
			while arguments.len() <= index {
				arguments.push(state.factory.new_identifier("undefined"));
			}
			arguments[index] = expression;
			prerequisites.extend(statements);
		}
	}

	let metadata = read_macro_metadata(state, signature.declaration());
	transform_networking_middleware_intrinsic(
		state,
		signature,
		&mut arguments,
		metadata.symbols("intrinsic-middleware"),
		runtime,
	)?;
	if let [symbol] = metadata.symbols("intrinsic-inline") {
		let expression = inline_macro_intrinsic(state, signature, &arguments, *symbol)?;
		return Ok(MacroTransformResult {
			expression: Some(expression),
			prerequisites,
			imports: Vec::new(),
		});
	}
	validate_parameter_const_intrinsic(node, signature, metadata.symbols("intrinsic-const"))?;
	if metadata.requested("intrinsic-arg-shift") && !arguments.is_empty() {
		arguments.remove(0);
	}

	let mut callee = node.expression().unwrap_or(node);
	let mut imports = Vec::with_capacity(1);
	if let Some(target) = flamework_rewrite_symbol(&metadata)
		&& let Some(parent) = target.parent()
	{
		let namespace = parent.name();
		callee = state.factory.new_element_access_expression(
			state.factory.new_identifier(alloc(namespace)),
			None,
			state.factory.new_string_literal(alloc(target.name()), TokenFlags::None),
			NodeFlags::None,
		);
		imports.push(MacroImport {
			module: FLAMEWORK_CORE_MODULE.to_owned(),
			export: namespace.to_owned(),
			local: namespace.to_owned(),
		});
	}
	let argument_list = state.factory.new_node_list(arguments);
	let updated = if is_call_expression(node) {
		let call = node.as_call_expression();
		state.factory.update_call_expression(
			node,
			callee,
			call.question_dot_token(),
			call.type_arguments(),
			argument_list,
			node.flags(),
		)
	} else {
		let constructor = node.as_new_expression();
		state
			.factory
			.update_new_expression(node, callee, constructor.type_arguments(), Some(argument_list))
	};
	Ok(MacroTransformResult {
		expression: Some(updated),
		prerequisites,
		imports,
	})
}

/// `flameworkRewriteSymbol`: the namespace member `intrinsic-flamework-rewrite` redirects to.
pub(crate) fn flamework_rewrite_symbol(metadata: &MacroMetadata) -> Option<P<Symbol>> {
	metadata
		.symbols("intrinsic-flamework-rewrite")
		.first()
		.copied()
		.filter(|symbol| symbol.parent().is_some())
}

/// `buildUserMacro`: the macro's expression, as `never`, and its hoisted statements.
pub(crate) fn build_user_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	user_macro: &UserMacro,
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let (expression, prerequisites) = match user_macro.kind {
		MacroKind::Generic => build_generic_macro(state, trace, user_macro, runtime)?,
		MacroKind::Caller => (
			transform_macro_types_build_caller(state, trace, user_macro, runtime)?,
			Vec::new(),
		),
		MacroKind::Many => build_many_macro(state, trace, user_macro, runtime)?,
		MacroKind::Literal => (literal_expression(&state.factory, &user_macro.literal), Vec::new()),
		MacroKind::Intrinsic => build_intrinsic_macro(state, trace, user_macro, runtime)?,
	};
	Ok((as_never(&state.factory, expression), prerequisites))
}

pub(crate) fn build_generic_macro(
	state: &mut TransformState<'_>,
	trace: P<Node>,
	user_macro: &UserMacro,
	runtime: &MacroRuntime,
) -> Result<(P<Node>, Vec<P<Node>>), Error> {
	let Some(target) = user_macro.target else {
		return Ok((state.factory.new_identifier("undefined"), Vec::new()));
	};
	match user_macro.metadata.as_str() {
		"id" => {
			let id = type_uid(state, target, trace)?;
			Ok((
				state.factory.new_string_literal(alloc(&id), TokenFlags::None),
				Vec::new(),
			))
		}
		"text" => {
			let text = state.ck().type_to_string(target);
			Ok((
				state.factory.new_string_literal(alloc(&text), TokenFlags::None),
				Vec::new(),
			))
		}
		"guard" => {
			let Some(build_guard) = runtime.build_guard else {
				return Err(Error::sentinel(ErrorKind::GuardBuilderAbsent));
			};
			let result = build_guard(state, trace, target)?;
			let expression = result
				.expression
				.unwrap_or_else(|| state.factory.new_identifier("undefined"));
			Ok((expression, result.statements))
		}
		_ => Ok((state.factory.new_identifier("undefined"), Vec::new())),
	}
}
