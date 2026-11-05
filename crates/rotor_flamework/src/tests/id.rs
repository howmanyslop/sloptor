//! `id_test.go`

use std::collections::HashMap;

use crate::ErrorKind;
use crate::id::{
	DEFAULT_HASH_CONTEXT, IdentifierRequest, generate_identifier, new_salt, new_salt_from, new_uuid_v4,
	new_uuid_v4_from, stable_string_hash, stable_string_hash_from,
};
use crate::project::IdMode;

fn is_lower_hex(text: &str, len: usize) -> bool {
	text.len() == len && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_uuid_v4(text: &str) -> bool {
	let parts: Vec<&str> = text.split('-').collect();
	parts.len() == 5
		&& is_lower_hex(parts[0], 8)
		&& is_lower_hex(parts[1], 4)
		&& is_lower_hex(parts[2], 4)
		&& parts[2].starts_with('4')
		&& is_lower_hex(parts[3], 4)
		&& matches!(parts[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b')
		&& is_lower_hex(parts[4], 12)
}

#[test]
fn generate_identifier_matches_uid_oracle_for_every_mode() {
	let base = IdentifierRequest {
		salt: "salt".to_owned(),
		hash_prefix: "pkg".to_owned(),
		package_name: "@scope/pkg".to_owned(),
		internal_id: "game:out/server/services/Foo@Bar".to_owned(),
		declaration_name: "Bar".to_owned(),
		lua_file_name: "foo".to_owned(),
		next_id: 1,
		..IdentifierRequest::default()
	};
	for (mode, want) in [
		(IdMode::Full, "pkg:server/services/Foo@Bar"),
		(IdMode::Obfuscated, "pkg:XG"),
		(IdMode::Short, "pkg:foo@Bar{XG}"),
		(IdMode::Tiny, "pkg:Bar{XG}"),
	] {
		let request = IdentifierRequest { mode, ..base.clone() };
		assert_eq!(generate_identifier(&request).unwrap(), want);
	}
}

#[test]
fn generate_identifier_matches_game_and_package_branches() {
	let tests = [
		(
			IdentifierRequest {
				mode: IdMode::Obfuscated,
				salt: "salt".to_owned(),
				package_name: "game".to_owned(),
				is_game: true,
				next_id: 1,
				..IdentifierRequest::default()
			},
			"XG",
		),
		(
			IdentifierRequest {
				mode: IdMode::Full,
				is_package: true,
				package_prefix: "@dep/pkg".to_owned(),
				internal_id: "@dep/pkg:out/shared/index@Thing".to_owned(),
				..IdentifierRequest::default()
			},
			"@dep/pkg:shared/index@Thing",
		),
		(
			IdentifierRequest {
				mode: IdMode::Full,
				is_package: true,
				internal_id: "dep:src/index@Thing".to_owned(),
				..IdentifierRequest::default()
			},
			"dep:src/index@Thing",
		),
	];
	for (request, want) in tests {
		assert_eq!(generate_identifier(&request).unwrap(), want);
	}
}

#[test]
fn generate_identifier_rejects_unknown_mode() {
	let request = IdentifierRequest {
		mode: IdMode::parse("future"),
		..IdentifierRequest::default()
	};
	assert!(generate_identifier(&request).unwrap_err().is(ErrorKind::InvalidIdMode));
}

#[test]
fn stable_string_hash_memoizes_by_context_and_text() {
	let mut hashes = HashMap::new();
	let bytes: Vec<u8> = (0..32).collect();
	let mut reader = bytes.as_slice();
	let first = stable_string_hash_from(Some(&mut hashes), "message", "remotes", &mut reader).unwrap();
	let duplicate = stable_string_hash_from(Some(&mut hashes), "message", "remotes", &mut reader).unwrap();
	let other_context =
		stable_string_hash_from(Some(&mut hashes), "message", DEFAULT_HASH_CONTEXT, &mut reader).unwrap();
	assert_eq!(first, "00010203-0405-4607-8809-0a0b0c0d0e0f");
	assert_eq!(duplicate, first);
	assert_eq!(other_context, "10111213-1415-4617-9819-1a1b1c1d1e1f");
	assert_eq!(hashes["remotes:message"], first);
	assert_eq!(hashes["@:message"], other_context);
}

#[test]
fn new_uuid_v4_uses_rfc4122_version_and_variant_bits() {
	let zeros = [0u8; 16];
	assert_eq!(
		new_uuid_v4_from(&mut zeros.as_slice()).unwrap(),
		"00000000-0000-4000-8000-000000000000"
	);
}

#[test]
fn new_salt_uses_sixty_four_cryptographic_bytes() {
	let zeros = [0u8; 64];
	assert!(is_lower_hex(&new_salt_from(&mut zeros.as_slice()).unwrap(), 128));
}

#[test]
fn exported_crypto_primitives_use_fresh_randomness_and_persist_string_hashes() {
	let mut hashes = HashMap::new();
	let first_uuid = new_uuid_v4().unwrap();
	let second_uuid = new_uuid_v4().unwrap();
	let salt = new_salt().unwrap();
	let first_hash = stable_string_hash(Some(&mut hashes), "message", "remotes").unwrap();
	let duplicate_hash = stable_string_hash(Some(&mut hashes), "message", "remotes").unwrap();
	assert!(is_uuid_v4(&first_uuid) && is_uuid_v4(&second_uuid) && is_uuid_v4(&first_hash));
	assert_ne!(first_uuid, second_uuid);
	assert!(is_lower_hex(&salt, 128));
	assert_eq!(duplicate_hash, first_hash);
	assert_eq!(hashes["remotes:message"], first_hash);
}
