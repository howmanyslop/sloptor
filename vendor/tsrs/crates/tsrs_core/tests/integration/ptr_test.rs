use std::cell::Cell;
use tsrs_core::ptr::*;

struct Thing {
    n: Cell<i32>,
    next: Cell<Option<P<Thing>>>,
}

#[test]
fn identity_and_mutation() {
    let a = P::new(Thing { n: Cell::new(1), next: Cell::new(None) });
    let b = P::new(Thing { n: Cell::new(1), next: Cell::new(None) });
    assert_ne!(a, b);
    assert_eq!(a, a);
    a.next.set(Some(b));
    b.next.set(Some(a));
    a.next.get().unwrap().n.set(5);
    assert_eq!(b.n.get(), 5);
    assert_eq!(
        std::mem::size_of::<Option<P<Thing>>>(),
        if tsrs_core::COMPRESSED_PTRS { 4 } else { std::mem::size_of::<usize>() }
    );
}

#[test]
fn slices_and_strings() {
    let s = alloc_slice(&[1, 2, 3]);
    assert_eq!(s, &[1, 2, 3]);
    let v = alloc_vec(vec![String::from("a"), String::from("b")]);
    assert_eq!(v.len(), 2);
    assert_eq!(alloc_str("hello"), "hello");
    let mut builder = ArenaStringBuilder::with_capacity(2);
    builder.push_str("hello");
    builder.push(' ');
    builder.push_str("🦀");
    assert_eq!(builder.into_str(), "hello 🦀");
}

#[test]
fn option_slice_cell() {
    let c: OptionSliceCell<i32> = OptionSliceCell::default();
    assert_eq!(c.get(), None);
    c.set(Some(&[]));
    assert_eq!(c.get(), Some(&[][..]));
    c.set(Some(alloc_slice(&[1, 2])));
    assert_eq!(c.get(), Some(&[1, 2][..]));
}

#[test]
fn thin_slices_keep_identity() {
    for len in
        [0usize, 1, 3, u16::MAX as usize - 1, u16::MAX as usize, u16::MAX as usize + 1, 200_000]
    {
        let s: &'static [u32] = Box::leak((0..len as u32).collect::<Vec<_>>().into_boxed_slice());
        let c = SliceCell::new(s);
        assert!(std::ptr::eq(c.get(), s), "len {len}");
        let o: OptionSliceCell<u32> = OptionSliceCell::default();
        o.set(Some(s));
        assert!(std::ptr::eq(o.get().unwrap(), s), "len {len}");
        o.set(None);
        assert_eq!(o.get(), None);
    }
    let empty: &'static [u64] = &[];
    assert!(std::ptr::eq(SliceCell::new(empty).get(), empty));
}

#[test]
fn packed_str() {
    let short: &'static str = "identifier";
    assert!(std::ptr::eq(PackedStr::new(short).as_str(), short));
    assert_eq!(PackedStr::new("").as_str(), "");
    let long: &'static str = Box::leak("x".repeat(u16::MAX as usize + 3).into_boxed_str());
    assert_eq!(PackedStr::new(long).as_str(), long);
    let exact: &'static str = Box::leak("é".repeat(u16::MAX as usize / 2 + 1).into_boxed_str());
    assert_eq!(PackedStr::new(exact).as_str(), exact);
}
