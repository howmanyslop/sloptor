use tsrs_core::frozen::*;

#[test]
fn borrow_and_mutate() {
    let c = FrozenCell::new(vec![1]);
    c.borrow_mut().push(2);
    assert_eq!(&*c.borrow(), &[1, 2]);
    let a = c.borrow();
    let b = c.borrow();
    assert_eq!(a.len() + b.len(), 4);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "FrozenCell already borrowed")]
fn aliasing_is_detected() {
    let c = FrozenCell::new(vec![1]);
    let _r = c.borrow();
    c.borrow_mut().push(2);
}
