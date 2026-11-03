use std::sync::Arc;
use tsrs_core::collections::OrderedMap;

use tsrs_core::compileroptions::*;

#[test]
fn paths_order_is_part_of_compiler_option_equality() {
    let mut left = CompilerOptions::default();
    let mut right = CompilerOptions::default();
    left.paths = Some(Arc::new(OrderedMap::from_iter([
        ("a".to_string(), Some(vec!["one".to_string()])),
        ("b".to_string(), Some(vec!["two".to_string()])),
    ])));
    right.paths = Some(Arc::new(OrderedMap::from_iter([
        ("b".to_string(), Some(vec!["two".to_string()])),
        ("a".to_string(), Some(vec!["one".to_string()])),
    ])));
    assert!(!left.equals(&right));
    right.paths = left.paths.clone();
    assert!(left.equals(&right));
}
