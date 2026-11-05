use tsrs_core::P;

use std::cell::Cell;
use tsrs_core::linkstore::*;

#[derive(Default)]
struct Links {
    n: Cell<i32>,
}

#[test]
fn link_store() {
    let store: LinkStore<i32, Links> = LinkStore::default();
    let k = P::new(1);
    assert!(!store.has(k));
    store.get(k).n.set(5);
    assert_eq!(store.try_get(k).unwrap().n.get(), 5);
    assert_eq!(store.get(k), store.get(k));

    let paged: PagedLinkStore<Links> = PagedLinkStore::default();
    assert!(!paged.has(3));
    let first = paged.get(3);
    first.n.set(7);
    assert!(!paged.has(4)); // A page does not create its other links.
    assert_eq!(paged.get(3).n.get(), 7);
    paged.get(4).n.set(8);
    assert_eq!(paged.try_get(4).unwrap().n.get(), 8);
    paged.get(LINK_PAGE_SIZE as u64 + 3).n.set(10);
    assert_eq!(first, paged.get(3)); // Growing the page index does not move existing links.
    paged.get(1 << 40).n.set(9);
    assert_eq!(paged.try_get(1 << 40).unwrap().n.get(), 9);
}
