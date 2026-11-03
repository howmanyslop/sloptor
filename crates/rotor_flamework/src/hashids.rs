//! `hashids.go`: hashids 2.2.8's `encode`, as Flamework's obfuscated identifiers use it.

use crate::Error;

/// JavaScript's `Number.MAX_SAFE_INTEGER`.
pub const MAX_SAFE_HASH_ID: u64 = (1 << 53) - 1;
const HASH_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";
const HASH_SEPARATORS: &str = "cfhistuCFHISTU";

/// `EncodeHashID`: reproduces `new Hashids(salt, 2).encode(id)` from hashids 2.2.8.
///
/// # Errors
///
/// `id` is above JavaScript's maximum safe integer.
#[expect(
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	clippy::cast_sign_loss,
	reason = "hashids computes in JavaScript numbers; every value here is a safe integer"
)]
pub fn encode_hash_id(salt: &str, id: u64) -> Result<String, Error> {
	if id > MAX_SAFE_HASH_ID {
		return Err(Error::msg(format!(
			"flamework hash ID {id} exceeds JavaScript's maximum safe integer {MAX_SAFE_HASH_ID}"
		)));
	}
	let salt: Vec<char> = salt.chars().collect();
	let all: Vec<char> = HASH_ALPHABET.chars().collect();
	let separators_all: Vec<char> = HASH_SEPARATORS.chars().collect();

	let mut alphabet = without_runes(&all, &separators_all);
	let separators = shuffle_runes(&only_runes(&separators_all, &all), &salt);
	if separators.is_empty() || alphabet.len() as f64 / separators.len() as f64 > 3.5 {
		let separator_count = (alphabet.len() as f64 / 3.5).ceil() as usize;
		if separator_count > separators.len() {
			let difference = separator_count - separators.len();
			alphabet = alphabet[difference..].to_vec();
		}
	}

	alphabet = shuffle_runes(&alphabet, &salt);
	let guard_count = (alphabet.len() as f64 / 12.0).ceil() as usize;
	alphabet = alphabet[guard_count..].to_vec();

	let number = id as f64;
	let numbers_id = number % 100.0;
	let lottery = alphabet[(numbers_id % alphabet.len() as f64) as usize];
	let mut buffer = vec![lottery];
	buffer.extend_from_slice(&salt);
	buffer.extend_from_slice(&alphabet);
	alphabet = shuffle_runes(&alphabet, &buffer);

	let mut out = String::new();
	out.push(lottery);
	out.extend(to_hash_alphabet(number, &alphabet));
	Ok(out)
}

fn shuffle_runes(alphabet: &[char], salt: &[char]) -> Vec<char> {
	let mut shuffled = alphabet.to_vec();
	if salt.is_empty() {
		return shuffled;
	}
	let mut value_index = 0usize;
	let mut sum = 0usize;
	let mut i = shuffled.len().saturating_sub(1);
	while i > 0 {
		value_index %= salt.len();
		let integer = salt[value_index] as usize;
		sum += integer;
		let j = (integer + value_index + sum) % i;
		shuffled.swap(i, j);
		i -= 1;
		value_index += 1;
	}
	shuffled
}

#[expect(
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	clippy::cast_sign_loss,
	reason = "hashids computes in JavaScript numbers; every value here is a safe integer"
)]
fn to_hash_alphabet(mut input: f64, alphabet: &[char]) -> Vec<char> {
	let mut encoded = Vec::with_capacity(12);
	let base = alphabet.len() as f64;
	loop {
		encoded.push(alphabet[(input % base) as usize]);
		input = (input / base).floor();
		if input <= 0.0 {
			break;
		}
	}
	encoded.reverse();
	encoded
}

fn without_runes(input: &[char], excluded: &[char]) -> Vec<char> {
	input
		.iter()
		.copied()
		.filter(|candidate| !excluded.contains(candidate))
		.collect()
}

fn only_runes(input: &[char], allowed: &[char]) -> Vec<char> {
	input
		.iter()
		.copied()
		.filter(|candidate| allowed.contains(candidate))
		.collect()
}
