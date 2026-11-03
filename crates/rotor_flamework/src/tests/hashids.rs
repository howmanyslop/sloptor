//! `hashids_test.go`

use std::collections::HashMap;

use crate::hashids::{MAX_SAFE_HASH_ID, encode_hash_id};

#[test]
fn encode_hash_id_matches_hashids_228_oracle() {
	let long = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
	let tests: &[(&str, u64, &str)] = &[
		("", 0, "gY"),
		("", 1, "jR"),
		("salt", 1, "XG"),
		("salt", 62, "aaJ"),
		("salt", 12345, "X4j1"),
		("salt", MAX_SAFE_HASH_ID, "wQpRRqRX24R"),
		(long, 12345, "aDgq"),
		("🔥", 12345, "dN4G"),
		("a🔥b", MAX_SAFE_HASH_ID, "jka77A7QzW7"),
	];
	for &(salt, id, want) in tests {
		assert_eq!(encode_hash_id(salt, id).unwrap(), want, "EncodeHashID({salt:?}, {id})");
	}
}

#[test]
fn encode_hash_id_rejects_values_outside_javascript_safe_integer_range() {
	assert!(encode_hash_id("salt", MAX_SAFE_HASH_ID + 1).is_err());
}

#[test]
fn encode_hash_id_is_stable_and_collision_free_for_first_thousand_ids() {
	let mut seen: HashMap<String, u64> = HashMap::with_capacity(1000);
	for id in 1..=1000 {
		let first = encode_hash_id("salt", id).unwrap();
		let second = encode_hash_id("salt", id).unwrap();
		assert_eq!(first, second);
		if let Some(previous) = seen.insert(first.clone(), id) {
			panic!("IDs {previous} and {id} both encoded as {first:?}");
		}
	}
	assert_eq!(seen.len(), 1000);
}
