//! ECMAScript number formatting and parsing.

use core::fmt;

/// Formats `value` exactly as ECMAScript `Number::toString(10)` (the JS
/// `String(number)` operation), which roblox-ts relies on for emit. A port of
/// ECMA-262 §6.1.6.1.20 `Number::toString`.
#[must_use]
pub fn js_number_string(value: f64) -> String {
	if value.is_nan() {
		return "NaN".to_owned();
	}
	if value == 0.0 {
		// Covers -0 as well.
		return "0".to_owned();
	}
	if value < 0.0 {
		return format!("-{}", js_number_string(-value));
	}
	if value.is_infinite() {
		return "Infinity".to_owned();
	}

	// The shortest round-trip digits and decimal exponent: `digits` is the
	// digit string (k digits, leading digit first) and n satisfies
	// value = 0.digits × 10^n.
	let (digits, exponent) = shortest_digits(value);
	let k = i64::try_from(digits.len()).unwrap_or(i64::MAX);
	let n = exponent + 1;

	if k <= n && n <= 21 {
		return digits + &"0".repeat(usize::try_from(n - k).unwrap_or(0));
	}
	if 0 < n && n <= 21 {
		let split = usize::try_from(n).unwrap_or(0);
		return format!("{}.{}", &digits[..split], &digits[split..]);
	}
	if -6 < n && n <= 0 {
		return format!("0.{}{digits}", "0".repeat(usize::try_from(-n).unwrap_or(0)));
	}

	// Exponential notation. JS writes the exponent without zero padding,
	// e.g. "1e-7" and "1e+21".
	let mantissa = if k > 1 {
		format!("{}.{}", &digits[..1], &digits[1..])
	} else {
		digits
	};
	let exponent = n - 1;
	let sign = if exponent < 0 { '-' } else { '+' };
	format!("{mantissa}e{sign}{}", exponent.unsigned_abs())
}

/// Splits `d.ddde±X` (Rust's `{:e}` and `{:.N e}` output) into its digits and
/// decimal exponent.
fn split_scientific(repr: &str) -> (String, i64) {
	let (mantissa, exponent) = repr.split_once('e').unwrap_or((repr, "0"));
	(mantissa.replacen('.', "", 1), exponent.parse().unwrap_or(0))
}

/// Whether `digits` × 10^(`exponent` − len + 1) parses back to `value`.
fn round_trips(digits: &str, exponent: i64, value: f64) -> bool {
	let scale = exponent - i64::try_from(digits.len()).unwrap_or(0) + 1;
	format!("{digits}e{scale}")
		.parse::<f64>()
		.is_ok_and(|parsed| parsed.to_bits() == value.to_bits())
}

/// Adds one to a decimal digit string, returning the digits and whether the
/// carry added a leading digit.
fn increment(digits: &str) -> (String, bool) {
	let mut bytes = digits.as_bytes().to_vec();
	for byte in bytes.iter_mut().rev() {
		if *byte == b'9' {
			*byte = b'0';
		} else {
			*byte += 1;
			return (String::from_utf8_lossy(&bytes).into_owned(), false);
		}
	}
	bytes.insert(0, b'1');
	(String::from_utf8_lossy(&bytes).into_owned(), true)
}

/// The shortest digits that round-trip to positive finite `value`, choosing
/// the candidate closest to the exact value and breaking exact ties to an
/// even last digit, as Go's `strconv.FormatFloat(f, 'e', -1, 64)` and JS do.
/// Returns the digits (no trailing zeros) and the decimal exponent of the
/// first digit.
///
/// Rust's `{:e}` is shortest but does not always pick the closest candidate.
/// Two k-digit candidates can only both round-trip when the spacing of
/// k-digit decimals is under the double's rounding interval (about 2^-52 of
/// the value), so for k below 15 the shortest candidate is unique and Rust's
/// answer is Go's. From 15 digits on, the exact decimal expansion decides.
fn shortest_digits(value: f64) -> (String, i64) {
	let (shortest, shortest_exponent) = split_scientific(&format!("{value:e}"));
	if shortest.len() < 15 {
		return (shortest, shortest_exponent);
	}
	let k = shortest.len();
	// Every double's exact expansion has at most 767 significant digits.
	let (exact, exponent) = split_scientific(&format!("{value:.766e}"));
	let (floor, remainder) = exact.split_at(k);
	let remainder = remainder.trim_end_matches('0');
	if remainder.is_empty() {
		return (floor.trim_end_matches('0').to_owned(), exponent);
	}
	let (ceiling, carried) = increment(floor);
	let ceiling_exponent = if carried { exponent + 1 } else { exponent };
	let floor_ok = round_trips(floor, exponent, value);
	let ceiling_ok = round_trips(&ceiling, ceiling_exponent, value);
	let use_ceiling = match (floor_ok, ceiling_ok) {
		(true, true) => {
			// Compare the remainder with one half of a unit in the last place.
			let half = remainder.as_bytes()[0]
				.cmp(&b'5')
				.then_with(|| (remainder.len() > 1).cmp(&false));
			match half {
				core::cmp::Ordering::Greater => true,
				core::cmp::Ordering::Less => false,
				core::cmp::Ordering::Equal => (floor.as_bytes()[k - 1] - b'0') % 2 == 1,
			}
		}
		(false, true) => true,
		(true, false) => false,
		(false, false) => return (shortest, shortest_exponent),
	};
	let (digits, exponent) = if use_ceiling {
		(ceiling, ceiling_exponent)
	} else {
		(floor.to_owned(), exponent)
	};
	(digits.trim_end_matches('0').to_owned(), exponent)
}

/// The error of [`js_number_parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsNumberParseError {
	text: String,
}

impl fmt::Display for JsNumberParseError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "invalid number literal {:?}", self.text)
	}
}

impl core::error::Error for JsNumberParseError {}

/// Mirrors JS `Number(text)` for the literal forms the compiler emits.
///
/// Those are decimal floats (with optional `_` numeric separators) plus
/// `0x`/`0b`/`0o` integer forms. It follows the Go port, which tries
/// `strconv.ParseFloat` and then `strconv.ParseInt(text, 0, 64)`.
///
/// # Errors
///
/// When neither form parses, or the float overflows.
pub fn js_number_parse(text: &str) -> Result<f64, JsNumberParseError> {
	let cleaned: String = text.chars().filter(|&c| c != '_').collect();
	if let Some(value) = parse_float(&cleaned) {
		return Ok(value);
	}
	if let Some(value) = parse_int_base_prefixed(&cleaned) {
		#[expect(
			clippy::cast_precision_loss,
			reason = "JS Number(text) rounds integers to the nearest double"
		)]
		return Ok(value as f64);
	}
	Err(JsNumberParseError { text: text.to_owned() })
}

fn split_sign(text: &str) -> (bool, &str) {
	text.strip_prefix('-')
		.map_or_else(|| (false, text.strip_prefix('+').unwrap_or(text)), |rest| (true, rest))
}

/// `strconv.ParseFloat(text, 64)`: decimal floats, `inf`/`infinity`/`nan`,
/// and hexadecimal floats with a mandatory `p` exponent. Overflow is an
/// error, as Go reports `ErrRange`.
fn parse_float(text: &str) -> Option<f64> {
	let (negative, body) = split_sign(text);
	if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
		let magnitude = parse_hex_float(hex)?;
		return Some(if negative { -magnitude } else { magnitude });
	}
	let value: f64 = text.parse().ok()?;
	if value.is_infinite() && !body.to_ascii_lowercase().starts_with("inf") {
		return None;
	}
	Some(value)
}

/// The digits of a Go hexadecimal float after `0x`:
/// `hex* [. hex*] [pP] [+-]? digits`, with at least one mantissa digit.
fn parse_hex_float(text: &str) -> Option<f64> {
	let (mantissa, exponent) = text.split_once(['p', 'P'])?;
	let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
	if whole.is_empty() && fraction.is_empty() {
		return None;
	}
	let (exponent_negative, exponent_digits) = split_sign(exponent);
	if exponent_digits.is_empty() || !exponent_digits.bytes().all(|c| c.is_ascii_digit()) {
		return None;
	}
	let exponent = exponent_digits.parse::<i64>().unwrap_or(i64::MAX);
	let mut binary_exponent = if exponent_negative { -exponent } else { exponent };
	let mut bits: u64 = 0;
	let mut sticky = false;
	for (index, c) in whole.chars().chain(fraction.chars()).enumerate() {
		let digit = u64::from(c.to_digit(16)?);
		if index >= whole.len() {
			binary_exponent = binary_exponent.saturating_sub(4);
		}
		if bits >> 60 == 0 {
			bits = bits << 4 | digit;
		} else {
			// Extra digits only matter for rounding: fold them into a sticky
			// bit and keep the scale.
			sticky |= digit != 0;
			binary_exponent = binary_exponent.saturating_add(4);
		}
	}
	if sticky {
		bits |= 1;
	}
	#[expect(
		clippy::cast_precision_loss,
		reason = "one correctly rounded conversion of the mantissa"
	)]
	let mut value = bits as f64;
	// Scale in steps so an intermediate power of two never under- or
	// overflows before the product does.
	while binary_exponent != 0 {
		let step = binary_exponent.clamp(-1000, 1000);
		value *= 2f64.powi(i32::try_from(step).unwrap_or(0));
		binary_exponent -= step;
		if value == 0.0 || value.is_infinite() {
			break;
		}
	}
	(!value.is_infinite()).then_some(value)
}

/// The `strconv.ParseInt` base prefixes and their radixes.
const PREFIXES: [(&str, &str, u32); 3] = [("0x", "0X", 16), ("0b", "0B", 2), ("0o", "0O", 8)];

/// `strconv.ParseInt(text, 0, 64)`: an optional sign, then `0x`, `0b`, `0o`,
/// or a leading `0` for octal, or decimal digits.
fn parse_int_base_prefixed(text: &str) -> Option<i64> {
	let (negative, body) = split_sign(text);
	let (radix, digits) = PREFIXES
		.iter()
		.find_map(|&(lower, upper, radix)| {
			body.strip_prefix(lower)
				.or_else(|| body.strip_prefix(upper))
				.map(|rest| (radix, rest))
		})
		.unwrap_or_else(|| match body.strip_prefix('0') {
			Some(rest) if !rest.is_empty() => (8, rest),
			_ => (10, body),
		});
	if digits.is_empty() {
		return None;
	}
	let mut magnitude: u64 = 0;
	for c in digits.chars() {
		let digit = c.to_digit(radix)?;
		magnitude = magnitude.checked_mul(u64::from(radix))?.checked_add(u64::from(digit))?;
	}
	if negative {
		return if magnitude == 1 << 63 {
			Some(i64::MIN)
		} else {
			i64::try_from(magnitude).ok().map(|value| -value)
		};
	}
	i64::try_from(magnitude).ok()
}

#[cfg(test)]
mod tests;
