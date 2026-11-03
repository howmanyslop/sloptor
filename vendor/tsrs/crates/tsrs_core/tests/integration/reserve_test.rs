use std::collections::BTreeMap;

use tsrs_core::reserve::*;

#[test]
fn released_ranges_are_reused_and_coalesced() {
    let mut c = Chunks { next: GRANULE, free: BTreeMap::new(), live: 0, peak: GRANULE };
    let a = c.take(GRANULE, GRANULE).unwrap();
    let b = c.take(3 * GRANULE, GRANULE).unwrap();
    let d = c.take(GRANULE, GRANULE).unwrap();
    assert_eq!((a, b, d), (GRANULE, 2 * GRANULE, 5 * GRANULE));
    c.put(a, GRANULE);
    c.put(b, 3 * GRANULE);
    assert_eq!(c.free.len(), 1);
    assert_eq!(c.take(4 * GRANULE, GRANULE), Some(a));
    c.put(d, GRANULE);
    assert_eq!(c.next, d);
    assert_eq!(c.live, 4 * GRANULE);
    assert_eq!(c.take(RESERVE, GRANULE), None);
}

#[test]
fn aligned_takes_leave_the_skipped_range_free() {
    const A: usize = 32 * GRANULE;
    let mut c = Chunks { next: GRANULE, free: BTreeMap::new(), live: 0, peak: GRANULE };
    let small = c.take(GRANULE, GRANULE).unwrap();
    let big = c.take(A, A).unwrap();
    assert_eq!((small, big), (GRANULE, A));
    // The gap between them serves unaligned takes first, then aligned ones fit only past `big`.
    assert_eq!(
        c.free.iter().map(|(&o, &l)| (o, l)).collect::<Vec<_>>(),
        [(2 * GRANULE, A - 2 * GRANULE)]
    );
    assert_eq!(c.take(3 * GRANULE, GRANULE), Some(2 * GRANULE));
    assert_eq!(c.take(GRANULE, A), Some(2 * A));
    assert_eq!(c.live, 5 * GRANULE + A);
    // Everything back: the free ranges coalesce and `next` returns to the start.
    c.put(big, A);
    c.put(2 * A, GRANULE);
    c.put(2 * GRANULE, 3 * GRANULE);
    c.put(small, GRANULE);
    assert_eq!((c.next, c.live, c.free.len()), (GRANULE, 0, 0));
    // A released range is reused at its first aligned offset, its unaligned head stays free.
    let x = c.take(A, GRANULE).unwrap();
    let y = c.take(GRANULE, GRANULE).unwrap();
    c.put(x, A);
    assert_eq!(c.take(A / 2, A / 2), Some(A / 2));
    assert_eq!(
        c.free.iter().map(|(&o, &l)| (o, l)).collect::<Vec<_>>(),
        [(GRANULE, A / 2 - GRANULE), (A, GRANULE)]
    );
    assert!(y > x);
}

#[test]
fn chunks_are_committed_and_zeroed() {
    let p = alloc_chunk(1, false);
    // SAFETY: fresh, committed.
    unsafe {
        assert_eq!(*p.add(GRANULE - 1), 0);
        p.write(7);
        release_chunk(p, 1);
    }
    assert!(p.addr() - base().addr() >= GRANULE);
    let h = alloc_chunk(HUGE_CHUNK, true);
    assert_eq!((h.addr() - base().addr()) % HUGE_CHUNK, 0);
    // SAFETY: fresh, committed.
    unsafe {
        assert_eq!(*h.add(HUGE_CHUNK - 1), 0);
        h.write(7);
    }
}
