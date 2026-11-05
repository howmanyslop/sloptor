//! `plan_provenance_test.go`

use crate::plan::{ClassPlan, DecoratorPlan};

#[test]
fn class_plan_clone_preserves_legacy_decorator_provenance_and_decorator_isolation() {
	let original = ClassPlan {
		internal_id: "fixture:out/service@Service".to_owned(),
		contains_legacy_decorator: true,
		decorators: vec![DecoratorPlan {
			name: "Service".to_owned(),
			internal_id: "fixture:out/service@Service".to_owned(),
		}],
		..ClassPlan::default()
	};

	// Go's cloneClass: a copy with its own decorator slice.
	let mut cloned = original.clone();

	assert!(
		cloned.contains_legacy_decorator,
		"clone lost legacy-decorator provenance"
	);
	cloned.decorators[0].name = "mutated".to_owned();
	assert_eq!(original.decorators[0].name, "Service", "clone shared decorator storage");
}
