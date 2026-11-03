//! `enums.go`: `statements/transformEnumDeclaration.ts` and `util/hasMultipleDefinitions.ts`.
//! `needsInverseEntry` lives in [`crate::shared`].

use rotor_tsrs::ast::{self as tsast, ModifierFlags};
use rotor_tsrs::checker::LiteralValue;

use crate::call::expression_might_mutate;
use crate::diagnostics;
use crate::dispatch::transform_expression;
use crate::identifier::{transform_identifier_defined, validate_identifier};
use crate::literals::transform_property_name;
use crate::luau_types::{luau_declaration_statements, luau_enum_binding, luau_enum_table_cast};
use crate::prelude::*;
use crate::shared::needs_inverse_entry;

/// `hasMultipleDefinitions`: `util/hasMultipleDefinitions.ts` (L3-14). Counts the symbol's
/// declarations passing `filter`; true when more than one. Shared by the enum and namespace
/// merging bans.
pub fn has_multiple_definitions(symbol: P<Symbol>, filter: impl Fn(P<Node>) -> bool) -> bool {
	let mut amt_value_definitions = 0;
	for &declaration in symbol.declarations() {
		if filter(declaration) {
			amt_value_definitions += 1;
			if amt_value_definitions > 1 {
				return true;
			}
		}
	}
	false
}

/// `noMultipleDefinitions` reporting: `AddDiagnosticWithCache` keyed by the symbol in the
/// pass-wide `IsReportedByMultipleDefinitionsCache`.
pub fn add_multiple_definitions_diagnostic(state: &mut State<'_>, symbol: P<Symbol>, diagnostic: Diagnostic) {
	let mut cache = std::mem::take(&mut state.multi.is_reported_by_multiple_definitions_cache);
	state.diags.add_with_cache(symbol, diagnostic, &mut cache);
	state.multi.is_reported_by_multiple_definitions_cache = cache;
}

/// `transformEnumDeclaration`: `statements/transformEnumDeclaration.ts` (L18-128). String-only
/// enums collapse to a plain map; everything else gets the `local E` header and a do-block with
/// the `_inverse` table behind `setmetatable({}, { __index = _inverse })`, member and inverse
/// assignments interleaved per member. Const enums emit nothing (member accesses are inlined);
/// `declare enum` never reaches here.
///
/// In luauTypes mode the enum's `type` statement comes first, a const enum emits that alone, and
/// the object local is annotated with the member table.
pub fn transform_enum_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	// Go's luauDeclarationStatements returns an empty list without luauTypes.
	let result = if state.is_luau_typed {
		luau_declaration_statements(state, node)
	} else {
		state.ast.new_list()
	};

	// const enum: no emit unless preserveConstEnums
	if tsast::has_syntactic_modifier(node, ModifierFlags::Const)
		&& !state.program.options().preserve_const_enums.is_true()
	{
		return result;
	}

	let Some(name_node) = node.name() else {
		state.ice("transformer: EnumDeclaration has no name");
		return result;
	};

	let symbol = state.ck().get_symbol_at_location(name_node);
	if let Some(symbol) = symbol
		&& has_multiple_definitions(symbol, |declaration| {
			tsast::is_enum_declaration(declaration) && !tsast::has_syntactic_modifier(declaration, ModifierFlags::Const)
		}) {
		add_multiple_definitions_diagnostic(state, symbol, diagnostics::no_enum_merging(node));
		return result;
	}

	validate_identifier(state, name_node);

	let left = transform_identifier_defined(state, name_node);
	let is_hoisted = symbol.is_some_and(|symbol| state.is_hoisted.get(&symbol).copied().unwrap_or(false));

	let members = node.members();

	// FAST PATH: all members string-valued -> plain map, no inverse table
	let mut all_strings = true;
	for &member in members {
		if needs_inverse_entry(state, member) {
			all_strings = false;
			break;
		}
	}
	if all_strings {
		let fields = state.ast.new_list::<element::MapField>();
		for &member in members {
			let Some(member_name) = member.name() else {
				state.ice("transformer: EnumMember has no name");
				continue;
			};
			let key = transform_property_name(state, member_name);
			let key = state.push_to_var_if_complex(key, "");
			let value = if let Some(LiteralValue::String(value)) = state.ck().get_constant_value(member) {
				value
			} else {
				state.ice("transformer: string enum member has no string value");
				""
			};
			let value = state.ast.str(value);
			let field = state.ast.map_field(key, value);
			state.ast.list_push(fields, field);
		}
		let right = state.ast.map(fields);
		let statement = if is_hoisted {
			state.ast.assignment(left, AssignmentOperator::Assign, right)
		} else {
			let binding = luau_enum_binding(state, node, left);
			state.ast.variable_declaration(binding, Some(right.into()))
		};
		state.ast.list_push(result, statement);
		return result;
	}

	// GENERAL PATH: setmetatable + inverse, inside a do-block
	let statements = state.capture_statements(|state| transform_general_enum_members(state, node, left, members));

	if !is_hoisted {
		let binding = luau_enum_binding(state, node, left);
		let declaration = state.ast.variable_declaration(binding, None);
		state.ast.list_push(result, declaration);
	}
	let block = state.ast.do_statement(statements);
	state.ast.list_push(result, block);
	result
}

/// The general path's do-block body: the `_inverse` table, the metatable assignment, then the
/// member and inverse assignments per member.
fn transform_general_enum_members(state: &mut State<'_>, node: P<Node>, left: NodeId, members: &[P<Node>]) {
	let inverse_fields = state.ast.new_list();
	let inverse_map = state.ast.map(inverse_fields);
	let inverse_id = state.push_to_var(Some(inverse_map), "inverse");
	let setmetatable = state.ast.global_id("setmetatable");
	let empty_fields = state.ast.new_list();
	let empty = state.ast.map(empty_fields);
	let index_key = state.ast.str("__index");
	let index_field = state.ast.map_field(index_key, inverse_id);
	let metatable_fields = state.ast.list_from([index_field]);
	let metatable = state.ast.map(metatable_fields);
	let args = state.ast.list_from([empty, metatable]);
	let call = state.ast.call(setmetatable, args);
	let table = luau_enum_table_cast(state, node, call);
	let assignment = state.ast.assignment(left, AssignmentOperator::Assign, table);
	state.prereq(assignment);

	for &member in members {
		let Some(member_name) = member.name() else {
			state.ice("transformer: EnumMember has no name");
			continue;
		};
		let name = transform_property_name(state, member_name);
		let mutate_check_node = if tsast::is_computed_property_name(member_name) {
			member_name.expression().unwrap_or(member_name)
		} else {
			member_name
		};
		let index = if expression_might_mutate(state, name, Some(mutate_check_node)) {
			// note: we don't use pushToVarIfComplex here because identifier also needs to be
			// pushed since the value calculation might reassign the variable
			state.push_to_var(Some(name), "")
		} else {
			name
		};

		let value_exp = match state.ck().get_constant_value(member) {
			Some(LiteralValue::String(value)) => state.ast.str(value),
			Some(LiteralValue::Number(value)) => state.ast.num(value.0),
			_ => {
				// constantValue is always number without initializer, so assert is safe
				let Some(initializer) = member.initializer() else {
					state.ice("transformer: transformEnumDeclaration: computed member without initializer"); // upstream assert
					continue;
				};
				let value = transform_expression(state, initializer);
				state.push_to_var_if_complex(value, "value")
			}
		};

		let target = state.ast.computed_index(left, index);
		let assignment = state.ast.assignment(target, AssignmentOperator::Assign, value_exp);
		state.prereq(assignment);

		if needs_inverse_entry(state, member) {
			let target = state.ast.computed_index(inverse_id, value_exp);
			let assignment = state.ast.assignment(target, AssignmentOperator::Assign, index);
			state.prereq(assignment);
		}
	}
}
