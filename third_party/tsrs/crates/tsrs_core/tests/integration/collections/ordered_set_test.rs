use tsrs_core::collections::ordered_set::*;

#[test]
fn test_ordered_set() {
    let mut s: OrderedSet<i32> = OrderedSet::default();
    s.extend(0..10);
    s.insert(3);
    assert_eq!(s.len(), 10);
    assert!(s.shift_remove(&3));
    assert!(!s.shift_remove(&3));
    assert!(!s.contains(&3));
    let v: Vec<i32> = s.iter().copied().collect();
    assert_eq!(v, vec![0, 1, 2, 4, 5, 6, 7, 8, 9]);
}
