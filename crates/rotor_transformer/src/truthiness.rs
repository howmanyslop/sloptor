//! `truthiness.go`: `util/createTruthinessChecks.ts`. TS treats `0`, `NaN` and `""` as falsy
//! where Luau only has `false` and `nil`, so conditions need guards whenever the type admits
//! them.

use rotor_luau::Origin;
use rotor_tsrs::jsnum::Number;
use rotor_tsrs::types::TypeCheck;

use crate::diagnostics;
use crate::prelude::*;
use crate::types::is_possibly_type;

/// `WillCreateTruthinessChecks`
pub fn will_create_truthiness_checks(state: &mut State<'_>, t: P<Type>) -> bool {
	is_possibly_type(state, t, &[TypeCheck::NumberLiteral(Number(0.0))])
		|| is_possibly_type(state, t, &[TypeCheck::NaN])
		|| is_possibly_type(state, t, &[TypeCheck::EmptyString])
}

/// `CreateTruthinessChecks` (L17-56). The check order is fixed: `exp ~= 0`, `exp == exp` (NaN),
/// `exp ~= ""`, then `exp`, joined with `and`. A type assignable to `0` also gets the NaN check
/// (microsoft/TypeScript#32778). Any check pins `exp` to a `value` temp. Upstream computes the
/// type itself; callers pass `state.get_type(node)`.
pub fn create_truthiness_checks(state: &mut State<'_>, mut expression: NodeId, node: P<Node>, t: P<Type>) -> NodeId {
	let is_assignable_to_zero = is_possibly_type(state, t, &[TypeCheck::NumberLiteral(Number(0.0))]);
	let is_assignable_to_nan = is_possibly_type(state, t, &[TypeCheck::NaN]);
	let is_assignable_to_empty_string = is_possibly_type(state, t, &[TypeCheck::EmptyString]);

	if is_assignable_to_zero || is_assignable_to_nan || is_assignable_to_empty_string {
		expression = state.push_to_var_if_complex(expression, "value");
	}

	let mut checks = Vec::new();

	if is_assignable_to_zero {
		let zero = state.ast.num(0.0);
		checks.push(state.ast.binary(expression, BinaryOperator::NotEqual, zero));
	}

	// workaround for https://github.com/microsoft/TypeScript/issues/32778
	if is_assignable_to_zero || is_assignable_to_nan {
		checks.push(state.ast.binary(expression, BinaryOperator::Equal, expression));
	}

	if is_assignable_to_empty_string {
		let empty = state.ast.str("");
		checks.push(state.ast.binary(expression, BinaryOperator::NotEqual, empty));
	}

	checks.push(expression);

	if state.log_truthy_changes && (is_assignable_to_zero || is_assignable_to_nan || is_assignable_to_empty_string) {
		let mut check_texts = Vec::new();
		if is_assignable_to_zero {
			check_texts.push("0");
		}
		if is_assignable_to_zero || is_assignable_to_nan {
			check_texts.push("NaN");
		}
		if is_assignable_to_empty_string {
			check_texts.push("\"\"");
		}
		state
			.diags
			.add(diagnostics::truthy_change(node, &check_texts.join(", ")));
	}

	let start = if state.source_map_enabled && checks.len() > 1 {
		state.original_source_position(Some(node), false)
	} else {
		None
	};
	let origins = start.map_or_else(Vec::new, |start| {
		let origin = state.ast.add_origin(Origin {
			start,
			..Origin::default()
		});
		for &check in &checks[..checks.len() - 1] {
			state.ast.set_origin(check, origin);
		}
		vec![origin; checks.len() - 1]
	});
	binary_expression_chain(&mut state.ast, &checks, BinaryOperator::And, &origins)
}

/// `binaryExpressionChain` (`util/expressionChain.ts`): `[a, b, c]` with `and` is
/// `a and b and c` (a left fold). `origins[i]` anchors the i-th created binary.
pub fn binary_expression_chain(
	ast: &mut Ast,
	expressions: &[NodeId],
	operator: BinaryOperator,
	origins: &[rotor_luau::OriginId],
) -> NodeId {
	let Some((&first, rest)) = expressions.split_first() else {
		return ast.none();
	};
	let mut result = first;
	for (index, &expression) in rest.iter().enumerate() {
		result = ast.binary(result, operator, expression);
		if let Some(&origin) = origins.get(index) {
			ast.set_origin(result, origin);
		}
	}
	result
}
