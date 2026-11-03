//! `id.go`: Flamework 1.3.2's identifier generation (`uid.ts`), salts and string hashes.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read;

use rotor_support::gostr::quote;

use crate::hashids::encode_hash_id;
use crate::project::IdMode;
use crate::{Error, ErrorKind};

/// `DefaultHashContext`
pub const DEFAULT_HASH_CONTEXT: &str = "@";

/// `IdentifierRequest`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdentifierRequest {
	/// The project's ID generation mode.
	pub mode: IdMode,
	/// The hash salt.
	pub salt: String,
	/// The project's hash prefix.
	pub hash_prefix: String,
	/// The project's package name.
	pub package_name: String,
	/// The declaration's internal ID.
	pub internal_id: String,
	/// The declaration's name.
	pub declaration_name: String,
	/// The declaring file's Lua module name.
	pub lua_file_name: String,
	/// A package declaration's build prefix.
	pub package_prefix: String,
	/// The next identifier number.
	pub next_id: u64,
	/// Whether the project is a game.
	pub is_game: bool,
	/// Whether the declaration belongs to a package.
	pub is_package: bool,
}

/// `GenerateIdentifier`: reproduces the stable identifier branches in Flamework 1.3.2 `uid.ts`.
///
/// # Errors
///
/// An unknown ID mode, or an ID number hashids cannot encode.
pub fn generate_identifier(request: &IdentifierRequest) -> Result<String, Error> {
	if request.is_package {
		if request.package_prefix.is_empty() {
			return Ok(request.internal_id.clone());
		}
		return Ok(format_internal_id(&request.internal_id, &request.package_prefix));
	}
	match request.mode {
		IdMode::Full => Ok(format_internal_id(&request.internal_id, &request.hash_prefix)),
		IdMode::Obfuscated => generate_hashed_identifier(request, false),
		IdMode::Short | IdMode::Tiny => {
			let hash = generate_hashed_identifier(request, true)?;
			let mut name = format!("{}{{{hash}}}", request.declaration_name);
			if request.mode == IdMode::Short {
				name = format!("{}@{name}", request.lua_file_name);
			}
			if !request.hash_prefix.is_empty() {
				name = format!("{}:{name}", request.hash_prefix);
			}
			Ok(name)
		}
		IdMode::Other(ref mode) => Err(Error::kinded(ErrorKind::InvalidIdMode, &format!(" {}", quote(mode)))),
	}
}

fn generate_hashed_identifier(request: &IdentifierRequest, omit_prefix: bool) -> Result<String, Error> {
	let hash = encode_hash_id(&request.salt, request.next_id)?;
	if omit_prefix || (request.is_game && request.hash_prefix.is_empty()) {
		return Ok(hash);
	}
	let prefix = if request.hash_prefix.is_empty() {
		&request.package_name
	} else {
		&request.hash_prefix
	};
	Ok(format!("{prefix}:{hash}"))
}

fn format_internal_id(internal_id: &str, hash_prefix: &str) -> String {
	let Some(colon) = internal_id.rfind(':') else {
		return internal_id.to_owned();
	};
	let Some(at) = internal_id.rfind('@') else {
		return internal_id.to_owned();
	};
	if at <= colon + 1 || at == internal_id.len() - 1 {
		return internal_id.to_owned();
	}
	let mut path = &internal_id[colon + 1..at];
	if let Some(separator) = path.find(['/', '\\']) {
		path = &path[separator + 1..];
	}
	let mut formatted = format!("{path}@{}", &internal_id[at + 1..]);
	if !hash_prefix.is_empty() {
		formatted = format!("{hash_prefix}:{formatted}");
	}
	formatted
}

/// The process's cryptographic randomness, Go's `crypto/rand.Reader`.
pub(crate) struct CryptoReader;

impl Read for CryptoReader {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		getrandom::fill(buf).map_err(std::io::Error::other)?;
		Ok(buf.len())
	}
}

/// `NewSalt`: the lowercase hex encoding of 64 cryptographically random bytes.
///
/// # Errors
///
/// The system's randomness failed.
pub fn new_salt() -> Result<String, Error> {
	new_salt_from(&mut CryptoReader)
}

pub(crate) fn new_salt_from(reader: &mut impl Read) -> Result<String, Error> {
	let mut bytes = [0u8; 64];
	reader
		.read_exact(&mut bytes)
		.map_err(|error| Error::msg(format!("generate Flamework salt: {}", go_read_error(&error))))?;
	Ok(hex(&bytes))
}

/// `NewUUIDv4`: a cryptographically random RFC 4122 version 4 UUID.
///
/// # Errors
///
/// The system's randomness failed.
pub fn new_uuid_v4() -> Result<String, Error> {
	new_uuid_v4_from(&mut CryptoReader)
}

pub(crate) fn new_uuid_v4_from(reader: &mut impl Read) -> Result<String, Error> {
	let mut uuid = [0u8; 16];
	reader
		.read_exact(&mut uuid)
		.map_err(|error| Error::msg(format!("generate Flamework UUID: {}", go_read_error(&error))))?;
	uuid[6] = (uuid[6] & 0x0f) | 0x40;
	uuid[8] = (uuid[8] & 0x3f) | 0x80;
	Ok(format!(
		"{}-{}-{}-{}-{}",
		hex(&uuid[0..4]),
		hex(&uuid[4..6]),
		hex(&uuid[6..8]),
		hex(&uuid[8..10]),
		hex(&uuid[10..16])
	))
}

/// `StableStringHash`: the persisted UUID for `context:text`, or a new one.
///
/// # Errors
///
/// `hashes` is `None` (Go's nil map), or the system's randomness failed.
pub fn stable_string_hash<S: std::hash::BuildHasher>(
	hashes: Option<&mut HashMap<String, String, S>>,
	text: &str,
	context: &str,
) -> Result<String, Error> {
	stable_string_hash_from(hashes, text, context, &mut CryptoReader)
}

pub(crate) fn stable_string_hash_from<S: std::hash::BuildHasher>(
	hashes: Option<&mut HashMap<String, String, S>>,
	text: &str,
	context: &str,
	reader: &mut impl Read,
) -> Result<String, Error> {
	let Some(hashes) = hashes else {
		return Err(Error::sentinel(ErrorKind::NilStringHashes));
	};
	let key = format!("{context}:{text}");
	if let Some(hash) = hashes.get(&key)
		&& !hash.is_empty()
	{
		return Ok(hash.clone());
	}
	let hash = new_uuid_v4_from(reader)?;
	hashes.insert(key, hash.clone());
	Ok(hash)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
	let mut out = String::with_capacity(bytes.len() * 2);
	for byte in bytes {
		let _ = write!(out, "{byte:02x}");
	}
	out
}

/// Go's `io.ReadFull` error text.
fn go_read_error(error: &std::io::Error) -> String {
	if error.kind() == std::io::ErrorKind::UnexpectedEof {
		"unexpected EOF".to_owned()
	} else {
		error.to_string()
	}
}
