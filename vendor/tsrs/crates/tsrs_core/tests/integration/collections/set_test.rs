use tsrs_core::collections::set::*;

#[test]
fn test_set() {
    let mut a = new_set_from_items([1, 2, 3]);
    let b = new_set_from_items([3, 4]);
    assert!(a.intersects(&b));
    assert!(!a.is_subset_of(&b));
    assert!(!a.add_if_absent(1));
    assert!(a.add_if_absent(5));
    let u = a.unioned_with(&b);
    assert_eq!(u.len(), 5);
    assert!(b.is_subset_of(&u));
    a.delete(&5);
    assert!(a.equals(&new_set_from_items([1, 2, 3])));
}
