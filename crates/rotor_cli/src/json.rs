//! The `--json` wire shapes of `cmd/rotor/build.go`, encoded byte for byte as Go's `json.Encoder`
//! with `SetEscapeHTML(false)` writes them.

use std::fmt::Write as _;

/// `jsonDiagnostic`: one entry of the `diagnostics` array.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JsonDiagnostic {
	/// Relative to the project directory; empty without a location.
	pub file: String,
	/// 1-based; 0 without a location.
	pub line: usize,
	/// 1-based; 0 without a location.
	pub col: usize,
	/// `TS####`, or a transformer diagnostic's factory name; omitted when empty.
	pub code: String,
	/// `error` or `warning`.
	pub severity: &'static str,
	/// The message.
	pub message: String,
}

/// `jsonResult`: the one object `build --json` and `check --json` print.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JsonResult {
	/// sloptor's version.
	pub version: String,
	/// Whether the command succeeded.
	pub ok: bool,
	/// Files compiled or checked.
	pub files: usize,
	/// Wall time.
	pub duration_ms: u128,
	/// The diagnostics; always an array.
	pub diagnostics: Vec<JsonDiagnostic>,
}

/// A string as Go's encoder writes it without HTML escaping.
pub fn write_string(out: &mut String, text: &str) {
	rotor_compile::gojson::write_string_escaping(out, text, false);
}

impl JsonDiagnostic {
	fn encode(&self, out: &mut String) {
		out.push_str("{\"file\":");
		write_string(out, &self.file);
		let _ = write!(out, ",\"line\":{},\"col\":{}", self.line, self.col);
		if !self.code.is_empty() {
			out.push_str(",\"code\":");
			write_string(out, &self.code);
		}
		out.push_str(",\"severity\":");
		write_string(out, self.severity);
		out.push_str(",\"message\":");
		write_string(out, &self.message);
		out.push('}');
	}
}

impl JsonResult {
	/// `writeJSONResult`: the object and its trailing newline.
	#[must_use]
	pub fn encode(&self) -> String {
		let mut out = String::from("{\"version\":");
		write_string(&mut out, &self.version);
		let _ = write!(
			out,
			",\"ok\":{},\"files\":{},\"durationMs\":{},\"diagnostics\":[",
			self.ok, self.files, self.duration_ms
		);
		for (index, diagnostic) in self.diagnostics.iter().enumerate() {
			if index > 0 {
				out.push(',');
			}
			diagnostic.encode(&mut out);
		}
		out.push_str("]}\n");
		out
	}
}

#[cfg(test)]
mod tests {
	use super::{JsonDiagnostic, JsonResult, write_string};

	#[test]
	fn strings_match_go_without_html_escaping() {
		let mut out = String::new();
		write_string(&mut out, "a\u{8}b\u{c}c\u{1}<>&\"\\\n\r\t\u{2028}\u{2029}é");
		assert_eq!(out, r#""a\bb\fc\u0001<>&\"\\\n\r\t\u2028\u2029é""#);
	}

	#[test]
	fn the_result_omits_an_empty_code_and_keeps_an_empty_array() {
		let mut result = JsonResult {
			version: "1.2.3".to_owned(),
			ok: true,
			files: 2,
			duration_ms: 7,
			diagnostics: Vec::new(),
		};
		assert_eq!(
			result.encode(),
			"{\"version\":\"1.2.3\",\"ok\":true,\"files\":2,\"durationMs\":7,\"diagnostics\":[]}\n"
		);
		result.ok = false;
		result.diagnostics = vec![
			JsonDiagnostic {
				file: "src/a.ts".to_owned(),
				line: 1,
				col: 2,
				code: "TS2322".to_owned(),
				severity: "error",
				message: "m".to_owned(),
			},
			JsonDiagnostic {
				severity: "error",
				message: "bare".to_owned(),
				..JsonDiagnostic::default()
			},
		];
		assert_eq!(
			result.encode(),
			"{\"version\":\"1.2.3\",\"ok\":false,\"files\":2,\"durationMs\":7,\"diagnostics\":[\
			{\"file\":\"src/a.ts\",\"line\":1,\"col\":2,\"code\":\"TS2322\",\"severity\":\"error\",\"message\":\"m\"},\
			{\"file\":\"\",\"line\":0,\"col\":0,\"severity\":\"error\",\"message\":\"bare\"}]}\n"
		);
	}
}
