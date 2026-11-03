use std::collections::BTreeMap;
use tsrs_core::arena::retired::{State, spans_of, take_pages};

fn batch(s: &mut State, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    s.pending.extend(ranges.iter().map(|&(start, end)| (start, end)));
    take_pages(s)
}

/// A page two neighbouring retired chunks share goes once both are retired; pages given back are not given
/// back again when a slab around them is retired.
#[test]
fn neighbours_share_their_boundary_page() {
    let p = tsrs_core::reserve::page_size();
    let mut s = State { spans: BTreeMap::new(), pending: Vec::new(), pending_bytes: 0 };
    let a = (10 * p + 100, 12 * p + 50);
    let b = (12 * p + 50, 14 * p);
    assert_eq!(batch(&mut s, &[a]), [(11 * p, 12 * p)]);
    // The page at 12p (part a, part b) goes with b.
    assert_eq!(batch(&mut s, &[b]), [(12 * p, 14 * p)]);
    // Their slab: only what is outside them.
    assert_eq!(batch(&mut s, &[(8 * p, 16 * p)]), [(8 * p, 11 * p), (14 * p, 16 * p)]);
    // In one batch, in any order: the same pages, merged.
    let mut t = State { spans: BTreeMap::new(), pending: Vec::new(), pending_bytes: 0 };
    assert_eq!(batch(&mut t, &[b, a]), [(11 * p, 14 * p)]);
    // A chunk not touching them: its own whole pages only.
    assert_eq!(batch(&mut t, &[(20 * p + 1, 23 * p - 1)]), [(21 * p, 22 * p)]);
}

/// Ranges retired on both sides of a span given back earlier go back in one call over the whole span.
#[test]
fn a_batch_gives_back_whole_spans() {
    let p = tsrs_core::reserve::page_size();
    let mut s = State { spans: BTreeMap::new(), pending: Vec::new(), pending_bytes: 0 };
    assert_eq!(batch(&mut s, &[(10 * p, 12 * p)]), [(10 * p, 12 * p)]);
    let pages = batch(&mut s, &[(8 * p, 10 * p), (12 * p, 14 * p), (30 * p, 31 * p)]);
    assert_eq!(pages, [(8 * p, 10 * p), (12 * p, 14 * p), (30 * p, 31 * p)]);
    assert_eq!(spans_of(&s, &pages), [(8 * p, 14 * p), (30 * p, 31 * p)]);
}
