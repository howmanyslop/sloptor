use tsrs_core::json::Value;
use tsrs_core::json::map::{JsonMap, Key, LINEAR_MAX};

fn n(i: usize) -> Value {
    Value::Number(i as f64)
}

/// Lookups, replacement, removal and order agree with `IndexMap` on both sides of `LINEAR_MAX`.
#[test]
fn matches_indexmap_small_and_large() {
    for size in [0, 1, LINEAR_MAX - 1, LINEAR_MAX, LINEAR_MAX + 1, 3 * LINEAR_MAX] {
        let mut ours = JsonMap::new();
        let mut theirs = tsrs_core::collections::OrderedMap::<String, Value>::default();
        for i in 0..size {
            let key = format!("k{}", (i * 7) % (size + 3));
            assert_eq!(ours.insert(key.clone(), n(i)), theirs.insert(key, n(i)));
        }
        for i in 0..size + 3 {
            let key = format!("k{i}");
            assert_eq!(ours.get(key.as_str()), theirs.get(key.as_str()), "{size} {key}");
            assert_eq!(ours.get_index_of(key.as_str()), theirs.get_index_of(key.as_str()));
        }
        for i in (0..size + 3).step_by(3) {
            let key = format!("k{i}");
            assert_eq!(ours.shift_remove(key.as_str()), theirs.shift_remove(key.as_str()));
        }
        let order: Vec<_> = ours.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let want: Vec<_> = theirs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        assert_eq!(order, want, "{size}");
        for (k, v) in &theirs {
            assert_eq!(ours.get(k.as_str()), Some(v), "{size} {k}");
        }
        ours.reverse();
        theirs.reverse();
        ours.sort_keys();
        theirs.sort_keys();
        assert!(ours.iter().eq(theirs.iter().map(|(k, v)| (k.as_str(), v))), "{size}");
        for (k, v) in &theirs {
            assert_eq!(ours.get(k.as_str()), Some(v), "{size} {k}");
        }
    }
}

/// `from_distinct` builds the same map as inserting, and rejects a repeated key at any size and position.
#[test]
fn from_distinct_rejects_duplicates() {
    for size in [1, 2, LINEAR_MAX, LINEAR_MAX + 1, 3 * LINEAR_MAX] {
        let entries: Vec<(Key, Value)> =
            (0..size).map(|i| (format!("k{i}").into(), n(i))).collect();
        let map = JsonMap::from_distinct(entries.clone()).expect("distinct keys");
        assert_eq!(
            map,
            entries.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<JsonMap>()
        );
        for (k, v) in &entries {
            assert_eq!(map.get(k), Some(v));
        }
        for from in 0..size {
            for to in 0..=size {
                let mut dup = entries.clone();
                dup.insert(to, (format!("k{from}").into(), n(99)));
                assert!(JsonMap::from_distinct(dup).is_none(), "{size} {from} {to}");
            }
        }
    }
}

/// `key_eq` is `==` for every pair of keys up to 20 bytes that differ in at most one byte.
#[test]
fn key_eq_matches_equality() {
    for len in 0..=20 {
        let a: String = (0..len).map(|i| char::from(b'a' + i as u8)).collect();
        assert!(tsrs_core::json::map::key_eq(&a, &a));
        for at in 0..len {
            let mut b = a.clone().into_bytes();
            b[at] = b'Z';
            let b = String::from_utf8(b).unwrap();
            assert!(!tsrs_core::json::map::key_eq(&a, &b), "{a} {b}");
        }
        let longer = format!("{a}a");
        assert!(
            !tsrs_core::json::map::key_eq(&a, &longer)
                && !tsrs_core::json::map::key_eq(&longer, &a)
        );
    }
}

/// `get_from` finds what `get` finds, whatever the hint, and moves the hint past the member found.
#[test]
fn get_from_agrees_with_get() {
    for size in [0, 3, LINEAR_MAX + 2] {
        let map: JsonMap = (0..size).map(|i| (format!("k{i}"), n(i))).collect();
        for start in 0..=size + 1 {
            for i in 0..size + 2 {
                let key = format!("k{i}");
                let mut next = start;
                assert_eq!(map.get_from(&key, &mut next), map.get(&key), "{size} {start} {key}");
                let want = map.get_index_of(&key).map_or(start, |i| i + 1);
                assert_eq!(next, want, "{size} {start} {key}");
            }
        }
    }
}

#[test]
fn equality_ignores_order() {
    let a: JsonMap = [("a".to_string(), n(1)), ("b".to_string(), n(2))].into();
    let b: JsonMap = [("b".to_string(), n(2)), ("a".to_string(), n(1))].into();
    let c: JsonMap = [("a".to_string(), n(1))].into();
    assert_eq!(a, b);
    assert_ne!(a, c);
}
