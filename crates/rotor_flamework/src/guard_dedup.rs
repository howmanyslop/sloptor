//! `guard_dedup.go`: the types a guard should hoist into a shared variable.

use std::collections::{HashMap, HashSet};

use rotor_tsrs::P;
use rotor_tsrs::checker::{Type, TypeChecker, TypeFlags};

/// `guardTypesRequiringDedup`: types reached at least `limit` times, discounting the reach
/// through types already selected, in first-reached order.
pub(crate) fn guard_types_requiring_dedup(
	type_value: P<Type>,
	checker: &mut TypeChecker<'_>,
	limit: i64,
) -> HashSet<P<Type>> {
	struct Counter<'c, 'k> {
		checker: &'c mut TypeChecker<'k>,
		counts: HashMap<P<Type>, i64>,
		order: Vec<P<Type>>,
		visiting: HashSet<P<Type>>,
	}
	impl Counter<'_, '_> {
		fn count(&mut self, current: Option<P<Type>>, modifier: i64) {
			let Some(current) = current else {
				return;
			};
			if current
				.flags()
				.intersects(TypeFlags::Object | TypeFlags::UnionOrIntersection)
			{
				if !self.counts.contains_key(&current) {
					self.order.push(current);
				}
				*self.counts.entry(current).or_insert(0) += modifier;
			}
			if !self.visiting.insert(current) {
				return;
			}
			if current.flags().intersects(TypeFlags::UnionOrIntersection) {
				for child in current.types() {
					self.count(Some(*child), modifier);
				}
				self.visiting.remove(&current);
				return;
			}
			if !current.flags().intersects(TypeFlags::Object)
				|| self
					.checker
					.get_property_of_type(current, "_nominal_Instance")
					.is_some()
			{
				self.visiting.remove(&current);
				return;
			}
			for property in self.checker.get_properties_of_type(current) {
				let property_type = self.checker.get_type_of_property_of_type(current, property.name());
				self.count(property_type, modifier);
			}
			for index_info in self.checker.get_index_infos_of_type(current) {
				self.count(Some(index_info.key_type()), modifier);
				self.count(Some(index_info.value_type()), modifier);
			}
			self.visiting.remove(&current);
		}
	}
	let mut counter = Counter {
		checker,
		counts: HashMap::new(),
		order: Vec::new(),
		visiting: HashSet::new(),
	};
	counter.count(Some(type_value), 1);
	let mut required = HashSet::new();
	loop {
		let selected = counter.order.iter().copied().find(|current| {
			let seen = counter.counts.get(current).copied().unwrap_or(0);
			seen >= limit && !required.contains(current)
		});
		let Some(selected) = selected else {
			return required;
		};
		required.insert(selected);
		let seen = counter.counts.get(&selected).copied().unwrap_or(0);
		counter.count(Some(selected), -(seen - 1));
	}
}
