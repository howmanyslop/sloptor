//! `transform_class_diagnostics_test.go`

use crate::transform_class_diagnostics::{DependencyClass, validate_dependency_cycles};
use crate::{ErrorDetail, ErrorKind};

#[test]
fn validate_dependency_cycles_returns_typed_error_when_constructor_graph_is_circular() {
	let classes = [
		DependencyClass {
			internal_id: "game:out/a@A".to_owned(),
			dependencies: vec!["game:out/b@B".to_owned()],
		},
		DependencyClass {
			internal_id: "game:out/b@B".to_owned(),
			dependencies: vec!["game:out/a@A".to_owned()],
		},
	];

	let error = validate_dependency_cycles(&classes).expect_err("circular graph");

	assert!(error.is(ErrorKind::CircularDependency), "{error}");
	let Some(ErrorDetail::CircularDependency(path)) = error.detail() else {
		panic!("error detail = {:?}, want CircularDependency", error.detail());
	};
	assert_eq!(path, &["game:out/a@A", "game:out/b@B", "game:out/a@A"]);
}
