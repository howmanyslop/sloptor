//! `types.go`: the type predicates, over [`rotor_tsrs::types`] (the port of the same file),
//! with [`State`] as their context so they share the pass's `MacroManager` registry.

use rotor_tsrs::checker::TypeFlags;
use rotor_tsrs::types::{self as tstypes, TypeCheck};

use crate::prelude::*;

/// `IsDefinitelyType`
pub fn is_definitely_type(state: &mut State<'_>, t: P<Type>, callbacks: &[TypeCheck]) -> bool {
	tstypes::is_definitely_type(state, t, callbacks)
}

/// `IsPossiblyType`
pub fn is_possibly_type(state: &mut State<'_>, t: P<Type>, callbacks: &[TypeCheck]) -> bool {
	tstypes::is_possibly_type(state, t, callbacks)
}

/// `TypeCheck.Check`
pub fn check(state: &mut State<'_>, callback: TypeCheck, t: P<Type>) -> bool {
	callback.check(state, t)
}

/// `WalkTypes`: union and intersection members recursively, through each constraint that
/// differs from the type itself; the callback runs on each leaf as the walk reaches it, so the
/// checker sees Go's query order.
pub fn walk_types(state: &mut State<'_>, t: P<Type>, callback: &mut dyn FnMut(&mut State<'_>, P<Type>)) {
	if t.flags().intersects(TypeFlags::UnionOrIntersection) {
		for &member in t.types() {
			walk_types(state, member, callback);
		}
		return;
	}
	// type.getConstraint() == Checker.GetBaseConstraintOfType
	match state.ck().get_base_constraint_of_type(t) {
		Some(constraint) if constraint != t => walk_types(state, constraint, callback),
		_ => callback(state, t),
	}
}

/// `getTypeArguments`
pub fn get_type_arguments(state: &mut State<'_>, t: P<Type>) -> &'static [P<Type>] {
	tstypes::get_type_arguments(state, t)
}

/// `getFirstConstructSymbol`: the construct symbol of `expression`'s type, through
/// [`State::get_type`].
pub fn get_first_construct_symbol(state: &mut State<'_>, expression: P<Node>) -> Option<P<Symbol>> {
	let t = state.get_type(expression);
	tstypes::get_first_construct_symbol(t)
}

/// `GetFirstDefinedSymbol`
pub fn get_first_defined_symbol(state: &mut State<'_>, t: P<Type>) -> Option<P<Symbol>> {
	tstypes::get_first_defined_symbol(state, t)
}

/// `IsSharedTableType`: the ambient `SharedTable` type, iterated like a `Map`.
pub fn is_shared_table_type(state: &mut State<'_>, t: P<Type>) -> bool {
	tstypes::is_shared_table_type(state, t)
}

/// `getRecursiveBaseTypes`: the base types of a class or interface type, transitively.
pub fn get_recursive_base_types(state: &mut State<'_>, t: P<Type>) -> Vec<P<Type>> {
	tstypes::get_recursive_base_types(state, t)
}

/// `isUndefinedTypeCheck`: `undefined` or `void`.
#[must_use]
pub fn is_undefined_type_check(t: P<Type>) -> bool {
	t.flags().intersects(TypeFlags::Undefined | TypeFlags::Void)
}

/// `symbolIsAmbient`: whether `t`'s symbol is one of the named global ambient symbols. A name
/// that resolves to nothing never matches (Go guards `nil == nil` the same way).
pub fn symbol_is_ambient(state: &mut State<'_>, t: P<Type>, names: &[&str]) -> bool {
	let Some(symbol) = t.symbol() else {
		return false;
	};
	names.iter().any(|name| state.ambient_symbol(name) == Some(symbol))
}
