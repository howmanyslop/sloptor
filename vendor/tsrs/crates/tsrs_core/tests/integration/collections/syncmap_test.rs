use tsrs_core::collections::syncmap::*;

#[test]
fn test_sync_map() {
    let m: SyncMap<&str, i32> = SyncMap::default();
    assert_eq!(m.load_or_store("a", 1), (1, false));
    assert_eq!(m.load_or_store("a", 2), (1, true));
    assert_eq!(m.load(&"a"), Some(1));
    m.store("b", 3);
    m.store("b", 4);
    assert_eq!(m.load(&"b"), Some(4));
    assert_eq!(m.size(), 2);
    let c = m.clone_map();
    m.delete(&"a");
    assert_eq!(m.load(&"a"), None);
    assert_eq!(c.load(&"a"), Some(1));
}

#[test]
fn borrowed_lookup_hashes_like_the_key() {
    let m: SyncMap<String, u32> = SyncMap::default();
    for i in 0..1000 {
        m.store(format!("/dir/{i}/file.ts"), i);
    }
    assert!((0..1000).all(|i| m.load(format!("/dir/{i}/file.ts").as_str()) == Some(i)));
    assert_eq!(m.load("/dir/none"), None);
}

#[test]
fn shards_cover_every_key() {
    let m: SyncMap<u64, u64> = SyncMap::default();
    for i in 0..10_000 {
        m.store(i, i * 2);
    }
    assert_eq!(m.size(), 10_000);
    assert!((0..10_000).all(|i| m.load(&i) == Some(i * 2)));
    let mut keys = m.keys();
    keys.sort_unstable();
    assert_eq!(keys, (0..10_000).collect::<Vec<_>>());
    assert_eq!(m.to_map().len(), 10_000);
    let mut seen = 0;
    m.range(|_, _| {
        seen += 1;
        true
    });
    assert_eq!(seen, 10_000);
    m.clear();
    assert_eq!(m.size(), 0);
}

#[test]
fn racing_insertion_keeps_one_value() {
    let m = SyncMap::default();
    let barrier = std::sync::Barrier::new(16);
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..16)
            .map(|i| {
                let m = &m;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    m.load_or_store("shared", i)
                })
            })
            .collect();
        handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
    });
    let winner = m.load(&"shared").unwrap();
    assert!(results.iter().all(|(value, _)| *value == winner));
    assert_eq!(results.iter().filter(|(_, loaded)| !loaded).count(), 1);
}

#[test]
fn range_callbacks_can_mutate_the_map() {
    let m = SyncMap::default();
    for i in 0..128 {
        m.store(i, i);
    }
    let mut visited = Vec::new();
    m.range(|key, value| {
        assert_eq!(key, value);
        visited.push(*key);
        m.delete(key);
        m.store(*key + 128, *value);
        true
    });
    visited.sort_unstable();
    assert_eq!(visited, (0..128).collect::<Vec<_>>());
    assert_eq!(m.size(), 128);
    assert!((0..128).all(|i| m.load(&i).is_none() && m.load(&(i + 128)) == Some(i)));
}

#[test]
fn empty_maps_and_clones_remain_independent() {
    let m = SyncMap::<u64, u64>::default();
    assert_eq!(m.load(&1), None);
    m.delete(&1);
    m.clear();
    assert_eq!(m.keys(), []);
    assert!(m.to_map().is_empty());
    let empty_clone = m.clone_map();
    m.store(1, 2);
    assert_eq!(empty_clone.load(&1), None);
    let populated_clone = m.clone_map();
    m.clear();
    assert_eq!(populated_clone.load(&1), Some(2));
    m.store(3, 4);
    assert_eq!(m.load(&3), Some(4));
    assert_eq!(populated_clone.load(&3), None);
}

#[test]
fn range_stops_after_the_callback_returns_false() {
    let m = SyncMap::default();
    for i in 0..128 {
        m.store(i, i);
    }
    let mut visited = 0;
    m.range(|key, _| {
        visited += 1;
        m.delete(key);
        false
    });
    assert_eq!(visited, 1);
    assert_eq!(m.size(), 127);
}

#[test]
fn a_panicking_write_poisons_its_shard() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct PanicOnClone(Arc<AtomicBool>);

    impl Clone for PanicOnClone {
        fn clone(&self) -> Self {
            assert!(!self.0.load(Ordering::Relaxed), "intentional clone panic");
            Self(Arc::clone(&self.0))
        }
    }

    let panic_on_clone = Arc::new(AtomicBool::new(true));
    let m = SyncMap::default();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            m.load_or_store(42_u64, PanicOnClone(Arc::clone(&panic_on_clone)))
        }))
        .is_err()
    );
    panic_on_clone.store(false, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| m.load(&42))).is_err());
}

#[test]
fn a_write_during_unwinding_does_not_poison() {
    struct StoreOnDrop<'a>(&'a SyncMap<u64, u64>);

    impl Drop for StoreOnDrop<'_> {
        fn drop(&mut self) {
            self.0.store(1, 2);
        }
    }

    let m = SyncMap::default();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _store = StoreOnDrop(&m);
            panic!("intentional unwind");
        }))
        .is_err()
    );
    assert_eq!(m.load(&1), Some(2));
}
