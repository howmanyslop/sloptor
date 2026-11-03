use tsrs_ast::Symbol;
use tsrs_ast::SymbolFlags;
use tsrs_checker::checker_11::*;
use tsrs_core::alloc_slice;

#[test]
fn ordered_counts_reuse_without_leaking_nested_state() {
    let first = Symbol::new(SymbolFlags::Property, "first");
    let second = Symbol::new(SymbolFlags::Property, "second");
    let third = Symbol::new(SymbolFlags::Property, "third");

    let mut outer = NameCounts::default();
    outer.lists.push(alloc_slice(&[second, first]));
    outer.lists.push(alloc_slice(&[first, third, second]));
    outer.count_lists(5);
    assert_eq!(
        outer.entries.iter().map(|&(name, _, count)| (name, count)).collect::<Vec<_>>(),
        [("second", 2), ("first", 2), ("third", 1)]
    );

    let mut inner = NameCounts::default();
    inner.lists.push(alloc_slice(&[third, first, third]));
    inner.count_lists(3);
    assert_eq!(
        inner.entries.iter().map(|&(name, _, count)| (name, count)).collect::<Vec<_>>(),
        [("third", 2), ("first", 1)]
    );
    assert_eq!(
        outer.entries.iter().map(|&(name, _, count)| (name, count)).collect::<Vec<_>>(),
        [("second", 2), ("first", 2), ("third", 1)]
    );

    let entries = outer.entries.as_ptr();
    let table = outer.table.as_ptr();
    outer.lists.clear();
    outer.lists.push(alloc_slice(&[first, third]));
    outer.count_lists(2);
    assert_eq!(outer.entries.as_ptr(), entries);
    assert_eq!(outer.table.as_ptr(), table);
    assert_eq!(
        outer.entries.iter().map(|&(name, _, count)| (name, count)).collect::<Vec<_>>(),
        [("first", 1), ("third", 1)]
    );
}
