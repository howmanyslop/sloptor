//! The subset of `tsgo/jsonrpc` and `tsgo/lsp/lsproto` the server speaks.
//!
//! Messages are encoded byte for byte as Go's `go-json-experiment` encoder writes them: struct fields in declaration order, `omitzero`
//! fields left out, and strings escaped without HTML or U+2028/U+2029 escaping.

use std::fmt::Write as _;

use serde_json::{Map, Value};

/// `CodeInvalidRequest`
pub const INVALID_REQUEST: i32 = -32600;
/// `CodeMethodNotFound`
pub const METHOD_NOT_FOUND: i32 = -32601;
/// `CodeInvalidParams`
pub const INVALID_PARAMS: i32 = -32602;
/// `CodeInternalError`
pub const INTERNAL_ERROR: i32 = -32603;
/// `ErrorCodeServerNotInitialized`
pub const SERVER_NOT_INITIALIZED: i32 = -32002;

/// `DiagnosticSeverityError`
pub const SEVERITY_ERROR: u8 = 1;
/// `DiagnosticSeverityWarning`
pub const SEVERITY_WARNING: u8 = 2;
/// `MessageTypeError`
pub const MESSAGE_ERROR: u8 = 1;
/// `MessageTypeWarning`
pub const MESSAGE_WARNING: u8 = 2;

/// A string as `go-json-experiment` writes it by default.
pub fn write_string(out: &mut String, text: &str) {
	out.push('"');
	for character in text.chars() {
		match character {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\u{8}' => out.push_str("\\b"),
			'\u{c}' => out.push_str("\\f"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			control if u32::from(control) < 0x20 => {
				let _ = write!(out, "\\u{:04x}", u32::from(control));
			}
			other => out.push(other),
		}
	}
	out.push('"');
}

/// `jsonrpc.ID`: an integer or a string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Id {
	/// An integer id.
	Int(i32),
	/// A string id.
	Str(String),
}

impl Id {
	/// `MarshalJSON`: an empty string id writes as the integer 0, as Go's does.
	pub fn encode(&self, out: &mut String) {
		match self {
			Self::Str(text) if !text.is_empty() => write_string(out, text),
			Self::Str(_) => out.push('0'),
			Self::Int(value) => {
				let _ = write!(out, "{value}");
			}
		}
	}
}

/// `jsonrpc.Message`, decoded far enough to route it.
#[derive(Clone, Debug, Default)]
pub struct RawMessage {
	/// `None` for a notification.
	pub id: Option<Id>,
	/// Empty for a response.
	pub method: String,
	/// The params as sent, if any (`null` included).
	pub params: Option<Value>,
}

impl RawMessage {
	/// `IsRequest`
	#[must_use]
	pub const fn is_request(&self) -> bool {
		self.id.is_some() && !self.method.is_empty()
	}

	/// `IsNotification`
	#[must_use]
	pub const fn is_notification(&self) -> bool {
		self.id.is_none() && !self.method.is_empty()
	}

	/// `IsResponse`
	#[must_use]
	pub const fn is_response(&self) -> bool {
		self.id.is_some() && self.method.is_empty()
	}
}

/// Decodes the envelope of a message.
///
/// # Errors
///
/// The text of the decoding failure.
pub fn parse_raw(data: &[u8]) -> Result<RawMessage, String> {
	let value: Value = serde_json::from_slice(data).map_err(|error| error.to_string())?;
	let Value::Object(object) = value else {
		return Err(format!(
			"json: cannot unmarshal JSON {} into Go jsonrpc.Message",
			kind(&value)
		));
	};
	let mut message = RawMessage::default();
	if let Some(version) = object.get("jsonrpc")
		&& version != "2.0"
	{
		return Err("invalid JSON-RPC version".to_owned());
	}
	match object.get("id") {
		None | Some(Value::Null) => {}
		Some(Value::String(text)) => message.id = Some(Id::Str(text.clone())),
		Some(Value::Number(number)) => {
			let value = number
				.as_i64()
				.and_then(|value| i32::try_from(value).ok())
				.ok_or_else(|| format!("json: cannot unmarshal JSON number {number} into Go int32"))?;
			message.id = Some(Id::Int(value));
		}
		Some(other) => {
			return Err(format!("json: cannot unmarshal JSON {} into Go int32", kind(other)));
		}
	}
	match object.get("method") {
		None | Some(Value::Null) => {}
		Some(Value::String(method)) => message.method.clone_from(method),
		Some(other) => {
			return Err(format!("json: cannot unmarshal JSON {} into Go string", kind(other)));
		}
	}
	message.params = object.get("params").cloned();
	Ok(message)
}

const fn kind(value: &Value) -> &'static str {
	match value {
		Value::Null => "null",
		Value::Bool(_) => "boolean",
		Value::Number(_) => "number",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}

fn object(value: &Value) -> Option<&Map<String, Value>> {
	value.as_object()
}

fn string<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
	object.get(key)?.as_str()
}

fn int32(object: &Map<String, Value>, key: &str) -> Option<i32> {
	i32::try_from(object.get(key)?.as_i64()?).ok()
}

/// The URI of a `TextDocumentIdentifier` (`uri` required).
fn document_uri(params: &Map<String, Value>) -> Option<String> {
	let document = object(params.get("textDocument")?)?;
	Some(string(document, "uri")?.to_owned())
}

/// `DidOpenTextDocumentParams`: the item's URI and text; every field of the item is required.
#[must_use]
pub fn did_open(params: &Value) -> Option<(String, String)> {
	let item = object(object(params)?.get("textDocument")?)?;
	string(item, "languageId")?;
	int32(item, "version")?;
	Some((string(item, "uri")?.to_owned(), string(item, "text")?.to_owned()))
}

/// `DidChangeTextDocumentParams`: the URI and the last whole-document change's text, if any.
#[must_use]
pub fn did_change(params: &Value) -> Option<(String, Option<String>)> {
	let params = object(params)?;
	let document = object(params.get("textDocument")?)?;
	int32(document, "version")?;
	let uri = string(document, "uri")?.to_owned();
	let mut text = None;
	for change in params.get("contentChanges")?.as_array()? {
		let change = object(change)?;
		let changed = string(change, "text")?;
		if change.contains_key("range") {
			// A partial change; the server advertises full sync, so it carries no new text.
			object(change.get("range")?)?;
			continue;
		}
		text = Some(changed.to_owned());
	}
	Some((uri, text))
}

/// `DidSaveTextDocumentParams`: the URI and the saved text when the client includes it.
#[must_use]
pub fn did_save(params: &Value) -> Option<(String, Option<String>)> {
	let params = object(params)?;
	let uri = document_uri(params)?;
	let text = match params.get("text") {
		None | Some(Value::Null) => None,
		Some(Value::String(text)) => Some(text.clone()),
		Some(_) => return None,
	};
	Some((uri, text))
}

/// `DidCloseTextDocumentParams`
#[must_use]
pub fn did_close(params: &Value) -> Option<String> {
	document_uri(object(params)?)
}

/// `InitializeParams`, as far as the server reads it: the position encodings the client offers.
///
/// `None` when the params do not decode (`processId`, `rootUri` and `capabilities` are
/// required), which leaves the encoding at UTF-16.
#[must_use]
pub fn initialize_encodings(params: &Value) -> Option<Vec<String>> {
	let params = object(params)?;
	match params.get("processId")? {
		Value::Null => {}
		Value::Number(number) if number.as_i64().is_some() => {}
		_ => return None,
	}
	match params.get("rootUri")? {
		Value::Null | Value::String(_) => {}
		_ => return None,
	}
	let capabilities = match params.get("capabilities")? {
		Value::Null => return Some(Vec::new()),
		Value::Object(capabilities) => capabilities,
		_ => return None,
	};
	let general = match capabilities.get("general") {
		None | Some(Value::Null) => return Some(Vec::new()),
		Some(Value::Object(general)) => general,
		Some(_) => return None,
	};
	match general.get("positionEncodings") {
		None | Some(Value::Null) => Some(Vec::new()),
		Some(Value::Array(encodings)) => encodings
			.iter()
			.map(|encoding| encoding.as_str().map(str::to_owned))
			.collect(),
		Some(_) => None,
	}
}

/// The sloptor-only `initializationOptions.typeErrors`; anything of the wrong shape is off.
#[must_use]
pub fn type_errors_option(params: Option<&Value>) -> bool {
	let Some(Value::Object(params)) = params else {
		return false;
	};
	match params.get("initializationOptions") {
		Some(Value::Object(options)) => matches!(options.get("typeErrors"), Some(Value::Bool(true))),
		_ => false,
	}
}

/// A response with `result`, raw JSON (`null` included).
#[must_use]
pub fn response(id: Option<&Id>, result: &str) -> String {
	let mut out = String::from("{\"jsonrpc\":\"2.0\",\"id\":");
	encode_id(&mut out, id);
	out.push_str(",\"result\":");
	out.push_str(result);
	out.push('}');
	out
}

/// An error response.
#[must_use]
pub fn error_response(id: Option<&Id>, code: i32, message: &str) -> String {
	let mut out = String::from("{\"jsonrpc\":\"2.0\",\"id\":");
	encode_id(&mut out, id);
	let _ = write!(out, ",\"error\":{{\"code\":{code},\"message\":");
	write_string(&mut out, message);
	out.push_str("}}");
	out
}

fn encode_id(out: &mut String, id: Option<&Id>) {
	match id {
		Some(id) => id.encode(out),
		None => out.push_str("null"),
	}
}

/// A notification of `method` with raw JSON `params`.
#[must_use]
pub fn notification(method: &str, params: &str) -> String {
	let mut out = String::from("{\"jsonrpc\":\"2.0\",\"method\":");
	write_string(&mut out, method);
	out.push_str(",\"params\":");
	out.push_str(params);
	out.push('}');
	out
}

/// `InitializeResult` with full-document sync, open/close and save, and the negotiated encoding.
#[must_use]
pub fn initialize_result(encoding: &str, server_name: &str) -> String {
	let mut out = String::from("{\"capabilities\":{\"positionEncoding\":");
	write_string(&mut out, encoding);
	out.push_str(",\"textDocumentSync\":{\"openClose\":true,\"change\":1,\"save\":true}},\"serverInfo\":{\"name\":");
	write_string(&mut out, server_name);
	out.push_str("}}");
	out
}

/// `lsproto.Position`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
	/// 0-based.
	pub line: u32,
	/// 0-based, in the negotiated encoding.
	pub character: u32,
}

/// `lsproto.Range`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Range {
	/// Inclusive.
	pub start: Position,
	/// Exclusive.
	pub end: Position,
}

/// `lsproto.Diagnostic`, as the server publishes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
	/// Where.
	pub range: Range,
	/// Error or warning.
	pub severity: u8,
	/// Omitted when empty.
	pub code: String,
	/// Always `sloptor`.
	pub source: String,
	/// The text.
	pub message: String,
}

impl Diagnostic {
	fn encode(&self, out: &mut String) {
		let range = self.range;
		let _ = write!(
			out,
			"{{\"range\":{{\"start\":{{\"line\":{},\"character\":{}}},\"end\":{{\"line\":{},\"character\":{}}}}},\"severity\":{}",
			range.start.line, range.start.character, range.end.line, range.end.character, self.severity
		);
		if !self.code.is_empty() {
			out.push_str(",\"code\":");
			write_string(out, &self.code);
		}
		out.push_str(",\"source\":");
		write_string(out, &self.source);
		out.push_str(",\"message\":");
		write_string(out, &self.message);
		out.push('}');
	}
}

/// `PublishDiagnosticsParams`
#[must_use]
pub fn publish_diagnostics_params(uri: &str, diagnostics: &[Diagnostic]) -> String {
	let mut out = String::from("{\"uri\":");
	write_string(&mut out, uri);
	out.push_str(",\"diagnostics\":[");
	for (index, diagnostic) in diagnostics.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		diagnostic.encode(&mut out);
	}
	out.push_str("]}");
	out
}

/// `ShowMessageParams`
#[must_use]
pub fn show_message_params(kind: u8, message: &str) -> String {
	let mut out = format!("{{\"type\":{kind},\"message\":");
	write_string(&mut out, message);
	out.push('}');
	out
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::{
		Diagnostic, Id, Position, Range, error_response, initialize_encodings, initialize_result, notification,
		parse_raw, publish_diagnostics_params, response, write_string,
	};

	#[test]
	fn strings_match_go_json_v2() {
		let mut out = String::new();
		write_string(&mut out, "a<>&\u{2028}\"\\\n\u{1}é");
		assert_eq!(out, "\"a<>&\u{2028}\\\"\\\\\\n\\u0001é\"");
	}

	#[test]
	fn envelopes_keep_go_field_order() {
		assert_eq!(
			response(Some(&Id::Int(5)), "null"),
			r#"{"jsonrpc":"2.0","id":5,"result":null}"#
		);
		assert_eq!(
			error_response(None, -32600, "invalid request: x"),
			r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"invalid request: x"}}"#
		);
		assert_eq!(
			response(Some(&Id::Str(String::new())), "1"),
			r#"{"jsonrpc":"2.0","id":0,"result":1}"#
		);
		assert_eq!(
			initialize_result("utf-16", "sloptor"),
			r#"{"capabilities":{"positionEncoding":"utf-16","textDocumentSync":{"openClose":true,"change":1,"save":true}},"serverInfo":{"name":"sloptor"}}"#
		);
		let diagnostic = Diagnostic {
			range: Range {
				start: Position { line: 1, character: 23 },
				end: Position { line: 1, character: 26 },
			},
			severity: 1,
			code: "noAny".to_owned(),
			source: "sloptor".to_owned(),
			message: "m".to_owned(),
		};
		assert_eq!(
			notification(
				"textDocument/publishDiagnostics",
				&publish_diagnostics_params("file:///a.ts", &[diagnostic])
			),
			r#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///a.ts","diagnostics":[{"range":{"start":{"line":1,"character":23},"end":{"line":1,"character":26}},"severity":1,"code":"noAny","source":"sloptor","message":"m"}]}}"#
		);
	}

	#[test]
	fn raw_messages_route_like_go() {
		let request = parse_raw(br#"{"jsonrpc":"2.0","id":1,"method":"x"}"#).expect("request");
		assert!(request.is_request());
		let notification = parse_raw(br#"{"jsonrpc":"2.0","method":"x","params":null}"#).expect("notification");
		assert!(notification.is_notification() && notification.params.is_some());
		let reply = parse_raw(br#"{"jsonrpc":"2.0","id":"a","result":1}"#).expect("response");
		assert!(reply.is_response());
		assert!(parse_raw(br#"{"jsonrpc":"1.0","id":1,"method":"x"}"#).is_err());
		assert!(parse_raw(b"[]").is_err());
	}

	#[test]
	fn initialize_requires_its_fields() {
		let offered =
			json!({"processId": null, "rootUri": null, "capabilities": {"general": {"positionEncodings": ["utf-8"]}}});
		assert_eq!(initialize_encodings(&offered), Some(vec!["utf-8".to_owned()]));
		assert_eq!(
			initialize_encodings(&json!({"rootUri": null, "capabilities": {}})),
			None,
			"processId is required"
		);
	}
}
