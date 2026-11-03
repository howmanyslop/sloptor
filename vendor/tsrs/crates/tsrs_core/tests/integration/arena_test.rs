use tsrs_core::{
    P, alloc_slice_recycled, arena_checkpoint, arena_pin, arena_rewind, free, free_slice,
};

#[test]
fn free_list_reuses_blocks_of_the_same_size() {
    let a = P::new([1u64, 2]);
    let addr = a.addr();
    // SAFETY: `a` is not used again.
    unsafe { free!(a) };
    let b = P::new_recycled([3u64, 4]);
    assert_eq!(b.addr(), addr);
    assert_eq!(*b, [3, 4]);
    let s = alloc_slice_recycled(&[5u64, 6]);
    let s_addr = s.as_ptr() as usize;
    assert_ne!(s_addr, addr);
    // SAFETY: `s` is not used again (not even for its address: Miri).
    unsafe { free_slice!(s) };
    let t = alloc_slice_recycled(&[7u64, 8]);
    assert_eq!(t.as_ptr() as usize, s_addr);
    assert_eq!(t, &[7, 8]);
}

/// A callee that frees a slice its caller made takes it as `*const [T]` (`free_slice_ptr`): a `&[T]` parameter
/// is a protected borrow for the whole call, and the free-list link written into the block would be undefined
/// behaviour (Miri reports it; LLVM deleted that write once, notes/fix-arena-recycle-uaf.md). Run under Miri:
/// `cargo +nightly miri test -p tsrs_core --features plain-ptrs --lib arena::tests`.
#[test]
fn a_callee_frees_a_slice_passed_as_a_raw_pointer() {
    /// # Safety
    /// `s` is a whole arena slice that nothing uses afterwards.
    unsafe fn recycle_list(s: *const [u64]) {
        // SAFETY: this function's contract.
        unsafe { tsrs_core::free_slice_ptr(s) };
    }
    let s = alloc_slice_recycled(&[1u64, 2, 3]);
    let s_addr = s.as_ptr() as usize;
    // SAFETY: `s` is not used again.
    unsafe { recycle_list(s) };
    let t = alloc_slice_recycled(&[4u64, 5, 6]);
    assert_eq!(t.as_ptr() as usize, s_addr);
    assert_eq!(t, &[4, 5, 6]);
    // The link was written: the list is empty again, so the next block of that size is fresh.
    let u = alloc_slice_recycled(&[7u64, 8, 9]);
    assert_ne!(u.as_ptr() as usize, s_addr);
    assert_eq!((t, u), (&[4u64, 5, 6][..], &[7u64, 8, 9][..]));
}

#[test]
#[ignore = "a negative control for Miri: it must report undefined behaviour (a write through a protected borrow)"]
fn miri_rejects_freeing_a_reference_parameter() {
    fn recycle_list(s: &'static [u64]) {
        // SAFETY: deliberately wrong: `s` is a protected borrow of the block for this whole call.
        unsafe { free_slice!(s) }; // source.mjs: deliberate
    }
    recycle_list(alloc_slice_recycled(&[1u64, 2, 3]));
}

#[test]
fn rewind_discards_speculative_allocations_unless_pinned() {
    let before = P::new(1u64);
    let cp = arena_checkpoint();
    let spec = P::new(2u64).addr();
    arena_rewind(cp);
    let after = P::new(3u64);
    assert_eq!(after.addr(), spec);
    assert_eq!((*before, *after), (1, 3));

    let cp = arena_checkpoint();
    let kept = P::new(4u64);
    arena_pin();
    arena_rewind(cp);
    let next = P::new(5u64);
    assert_ne!(next.addr(), kept.addr());
    assert_eq!(*kept, 4);
}

#[test]
fn region_scopes_route_allocations_and_free_drops_values() {
    use std::rc::Rc;
    use tsrs_core::arena::{Region, enter_owner, enter_thread_arena};
    let outside = P::new(1u64);
    let counter = Rc::new(());
    let region = Region::new(4096);
    let (inside, nested_outside, inner) = {
        let _scope = region.enter();
        let inside = P::new(Rc::clone(&counter));
        let v = tsrs_core::alloc_vec(vec![Rc::clone(&counter), Rc::clone(&counter)]);
        assert_eq!(v.len(), 2);
        let nested_outside = {
            let _global = enter_thread_arena();
            P::new(2u64)
        };
        let inner = Region::new(4096);
        {
            let _s = inner.enter();
            let x = P::new(3u64);
            assert!(Region::containing(x.addr()).unwrap().ptr_eq(&inner));
            // Owner routing back into the outer region from inside another one.
            let _o = enter_owner(inside.addr());
            let y = P::new(4u64);
            assert!(Region::containing(y.addr()).unwrap().ptr_eq(&region));
        }
        (inside.addr(), nested_outside, P::new(5u64))
    };
    assert!(Region::containing(outside.addr()).is_none());
    assert!(Region::containing(nested_outside.addr()).is_none());
    assert!(Region::containing(inner.addr()).unwrap().ptr_eq(&region));
    assert!(Region::containing(inside).unwrap().ptr_eq(&region));
    assert_eq!(Rc::strong_count(&counter), 4);
    drop(region);
    assert!(Region::containing(inside).is_none());
    assert_eq!(Rc::strong_count(&counter), 1);
    // Back on the thread arena.
    assert!(Region::containing(P::new(6u64).addr()).is_none());
    assert_eq!((*outside, *nested_outside), (1, 2));
}

#[test]
fn region_free_lists_and_rewinds_stay_in_the_region() {
    use std::rc::Rc;
    use tsrs_core::arena::Region;
    let counter = Rc::new(());
    let region = Region::new(4096);
    {
        let _scope = region.enter();
        let a = P::new([1u64, 2]);
        let a_addr = a.addr();
        // SAFETY: a is no longer accessed after this call, so its region block can be recycled.
        unsafe { free!(a) };
        let b = P::new_recycled([3u64, 4]);
        assert_eq!(b.addr(), a_addr);
        // A value that needs drop is not recycled in a region: its drop entry stays.
        let c = P::new(Rc::clone(&counter));
        let c_addr = c.addr();
        // SAFETY: c is no longer accessed; its region retains the drop entry for the Rc payload.
        unsafe { free!(c) };
        let d = P::new_recycled(Rc::clone(&counter));
        assert_ne!(d.addr(), c_addr);
        let cp = arena_checkpoint();
        let _spec = P::new(Rc::clone(&counter));
        arena_rewind(cp);
        assert_eq!(Rc::strong_count(&counter), 4);
    }
    drop(region);
    // The rewound value lost its entry (not dropped, like a thread arena); `c` and `d` were dropped.
    assert_eq!(Rc::strong_count(&counter), 2);
}

#[test]
fn nested_scratch_regions_escape_past_all_of_them() {
    use tsrs_core::arena::{Region, escape_scratch, escape_scratch_one_level};
    // A checker in a scratch region of its own emits a file in a nested scratch region: the checker's data
    // escapes the file's region into the checker's, diagnostics escape both.
    let outer = Region::new(4096);
    let checker = Region::new(4096);
    let file = Region::new(4096);
    let _o = outer.enter();
    let _c = checker.enter_scratch();
    {
        let _f = file.enter_scratch();
        assert!(Region::containing(P::new(1u64).addr()).unwrap().ptr_eq(&file));
        {
            let _e = escape_scratch_one_level().unwrap();
            assert!(Region::containing(P::new(2u64).addr()).unwrap().ptr_eq(&checker));
            let _d = escape_scratch().unwrap();
            assert!(Region::containing(P::new(3u64).addr()).unwrap().ptr_eq(&outer));
        }
        let _d = escape_scratch().unwrap();
        assert!(Region::containing(P::new(4u64).addr()).unwrap().ptr_eq(&outer));
    }
    // Back in the checker's region alone: both escapes lead to the same target.
    let _e = escape_scratch().unwrap();
    assert!(escape_scratch_one_level().is_none());
    assert!(Region::containing(P::new(5u64).addr()).unwrap().ptr_eq(&outer));
}

#[test]
fn scratch_regions_route_escapes_to_the_outer_target() {
    use tsrs_core::arena::{Region, enter_thread_arena, escape_scratch, scratch_active};
    let outer = Region::new(4096);
    let scratch = Region::new(4096);
    let _o = outer.enter();
    assert!(!scratch_active() && escape_scratch().is_none());
    {
        let _s = scratch.enter_scratch();
        assert!(scratch_active());
        assert!(Region::containing(P::new(1u64).addr()).unwrap().ptr_eq(&scratch));
        {
            let _e = escape_scratch().unwrap();
            // Already escaped: no second scope.
            assert!(escape_scratch().is_none());
            assert!(Region::containing(P::new(2u64).addr()).unwrap().ptr_eq(&outer));
            // Scratch allocations while escaped.
            assert!(Region::containing(P::new_scratch(3u64).addr()).unwrap().ptr_eq(&scratch));
            assert!(
                Region::containing(tsrs_core::alloc_str_scratch("abc").as_ptr() as usize)
                    .unwrap()
                    .ptr_eq(&scratch)
            );
            // A scope pushed and popped inside the escape restores the escape's target, not the scratch region.
            drop(enter_thread_arena());
            assert!(Region::containing(P::new(4u64).addr()).unwrap().ptr_eq(&outer));
        }
        assert!(Region::containing(P::new(5u64).addr()).unwrap().ptr_eq(&scratch));
    }
    assert!(!scratch_active());
    assert!(Region::containing(P::new(6u64).addr()).unwrap().ptr_eq(&outer));
    // Without a scratch region, `new_scratch` is `new`.
    assert!(Region::containing(P::new_scratch(7u64).addr()).unwrap().ptr_eq(&outer));
}

/// A retired region's range is never handed out again (fileregions.rs in `tsrs_compiler` frees a checked
/// file's tree while caches keyed by the address of a node in it may keep stale entries): no later chunk,
/// of any arena or region on any thread, overlaps it.
#[cfg(compressed_ptrs)]
#[test]
fn retired_regions_are_never_reused() {
    use tsrs_core::arena::Region;
    // More than a quarter of a slab: the region's chunk is a slab of its own, which a plain free would give back
    // to the reservation (or the slab cache) for reuse.
    const SIZE: usize = 1 << 20;
    let retired = Region::new(SIZE);
    let first = {
        let _s = retired.enter();
        P::new([1u64; 8]).addr()
    };
    let range = first..first + retired.allocated_bytes();
    retired.retire_on_free();
    drop(retired);
    let mut later = Vec::new();
    for i in 0..32u64 {
        let r = Region::new(SIZE);
        {
            let _s = r.enter();
            let p = P::new([i; 8]);
            assert!(!range.contains(&p.addr()), "a retired region's range was reused");
        }
        later.push(r);
    }
    let big = tsrs_core::alloc_slice(&vec![0u8; 4 * SIZE]);
    assert!(
        !range.contains(&(big.as_ptr() as usize))
            && !range.contains(&(big.as_ptr() as usize + big.len() - 1))
    );
    drop(later);
}

#[test]
fn small_regions_share_slabs_and_trim() {
    use tsrs_core::arena::Region;
    let a = Region::new(64 << 10);
    let x = {
        let _s = a.enter();
        P::new([7u64; 4])
    };
    a.trim();
    assert!(a.allocated_bytes() < 1024);
    let b = Region::new(64 << 10);
    let y = {
        let _s = b.enter();
        P::new([8u64; 4])
    };
    // Carved right after `a`'s trimmed chunk.
    assert!(y.addr() > x.addr() && y.addr() - x.addr() < 1024);
    assert_eq!((*x, *y), ([7; 4], [8; 4]));
    drop(a);
    assert!(Region::containing(y.addr()).unwrap().ptr_eq(&b));
    assert_eq!(*y, [8; 4]);
}
