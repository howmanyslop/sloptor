use tsrs_core::collections::cow::*;

#[test]
fn scopes() {
    let mut s: CopyOnWriteSet<i32> = CopyOnWriteSet::default();
    s.add(1);
    let scope = s.enter_scope();
    s.add(2);
    assert!(s.has(&1) && s.has(&2));
    s.exit_scope(scope);
    assert!(s.has(&1) && !s.has(&2));
}

#[test]
fn scopes_of_an_empty_map() {
    let mut m: CopyOnWriteMap<i32, i32> = CopyOnWriteMap::default();
    assert!(!m.has(&1) && m.get(&1).is_none());
    let scope = m.enter_scope();
    m.set(1, 10);
    assert_eq!(m.get(&1), Some(&10));
    m.exit_scope(scope);
    assert!(!m.has(&1) && m.get(&1).is_none());
    m.set(2, 20);
    assert_eq!(m.get(&2), Some(&20));
}
