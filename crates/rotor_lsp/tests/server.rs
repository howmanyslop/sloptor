//! `internal/lsp/server_test.go` and its harness (`harness_test.go`).
//!
//! Every test drives the server through real Content-Length framed messages and asserts on what
//! an editor would receive. The only fake is the compiler behind `Options::census`. The client
//! side decodes with `serde_json`, not the server's own protocol code.

#![cfg(test)]

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use rotor_compile::census::{FileDiagnostics, FileOutcome, ProjectDiagnostics};
use rotor_compile::compile::{DiagnosticInfo, InternalCompilerError};
use rotor_lsp::{Cancel, Census, FindTsConfig, Options};
use rotor_support as _;
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(10);
/// Keeps edit-driven runs fast without changing which runs coalesce.
const DEBOUNCE_FOR_TESTS: Duration = Duration::from_millis(20);

/// One server-to-client message, decoded just far enough to match on.
#[derive(Clone, Debug)]
struct Message {
	id: Option<Value>,
	method: String,
	params: Value,
	/// `Some(Value::Null)` for an explicit `null` result.
	result: Option<Value>,
	error: Option<(i64, String)>,
}

impl Message {
	fn decode(data: &[u8]) -> Option<Self> {
		let Value::Object(object) = serde_json::from_slice::<Value>(data).ok()? else {
			return None;
		};
		Some(Self {
			id: object.get("id").cloned(),
			method: object
				.get("method")
				.and_then(Value::as_str)
				.unwrap_or_default()
				.to_owned(),
			params: object.get("params").cloned().unwrap_or(Value::Null),
			result: object.get("result").cloned(),
			error: object.get("error").map(|error| {
				(
					error.get("code").and_then(Value::as_i64).unwrap_or_default(),
					error
						.get("message")
						.and_then(Value::as_str)
						.unwrap_or_default()
						.to_owned(),
				)
			}),
		})
	}
}

#[derive(Clone, Debug, Default)]
struct Published {
	uri: String,
	diagnostics: Vec<Value>,
}

fn decode_publish(message: &Message) -> Published {
	Published {
		uri: message.params["uri"].as_str().unwrap_or_default().to_owned(),
		diagnostics: message.params["diagnostics"].as_array().cloned().unwrap_or_default(),
	}
}

fn range_of(diagnostic: &Value) -> (u64, u64, u64, u64) {
	let range = &diagnostic["range"];
	let get = |side: &str, field: &str| range[side][field].as_u64().unwrap_or(u64::MAX);
	(
		get("start", "line"),
		get("start", "character"),
		get("end", "line"),
		get("end", "character"),
	)
}

type Received = Arc<(Mutex<Vec<Message>>, Condvar)>;

/// A scripted editor on the other end of the server's streams.
struct Client {
	to_server: Mutex<Option<std::io::PipeWriter>>,
	received: Received,
	done: Mutex<Option<Receiver<Result<(), String>>>>,
	cancel: Cancel,
}

#[derive(Clone)]
struct ServeConfig {
	census: Census,
	find: FindTsConfig,
	debounce: Duration,
}

fn start_server(config: ServeConfig) -> Client {
	let (server_in, to_server) = std::io::pipe().expect("pipe");
	let (mut from_server, server_out) = std::io::pipe().expect("pipe");
	let received: Received = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
	let (done_tx, done_rx): (Sender<Result<(), String>>, _) = mpsc::channel();
	let cancel = Cancel::default();
	{
		let cancel = cancel.clone();
		std::thread::spawn(move || {
			let result = rotor_lsp::serve(
				&cancel,
				server_in,
				server_out,
				Options {
					find_ts_config: config.find,
					census: config.census,
					debounce: config.debounce,
				},
			);
			let _ = done_tx.send(result);
		});
	}
	{
		let received = Arc::clone(&received);
		std::thread::spawn(move || {
			let mut reader = rotor_lsp::jsonrpc::Reader::new(&mut from_server);
			while let Ok(data) = reader.read() {
				let message = Message::decode(&data)
					.unwrap_or_else(|| panic!("server wrote a frame that is not JSON-RPC: {data:?}"));
				let (lock, signal) = &*received;
				lock.lock().expect("received").push(message);
				signal.notify_all();
			}
		});
	}
	Client {
		to_server: Mutex::new(Some(to_server)),
		received,
		done: Mutex::new(Some(done_rx)),
		cancel,
	}
}

impl Drop for Client {
	fn drop(&mut self) {
		self.to_server.lock().expect("writer").take();
		let Some(done) = self.done.lock().expect("done").take() else {
			return;
		};
		let Ok(result) = done.recv_timeout(WAIT) else {
			self.cancel.cancel();
			assert!(std::thread::panicking(), "serve did not return after its input closed");
			return;
		};
		if !std::thread::panicking() {
			assert_eq!(
				result,
				Ok(()),
				"serve returned an error after the client closed its end"
			);
		}
	}
}

impl Client {
	fn write(&self, mut body: Value) {
		body["jsonrpc"] = json!("2.0");
		let data = serde_json::to_vec(&body).expect("encode");
		let mut guard = self.to_server.lock().expect("writer");
		let writer = guard.as_mut().expect("open");
		write!(writer, "Content-Length: {}\r\n\r\n", data.len()).expect("header");
		writer.write_all(&data).expect("body");
		writer.flush().expect("flush");
		drop(guard);
	}

	#[expect(clippy::needless_pass_by_value, reason = "call sites pass json! literals")]
	fn request(&self, id: i64, method: &str, params: Value) {
		self.write(json!({"id": id, "method": method, "params": params}));
	}

	#[expect(clippy::needless_pass_by_value, reason = "call sites pass json! literals")]
	fn notify(&self, method: &str, params: Value) {
		self.write(json!({"method": method, "params": params}));
	}

	fn response_to(&self, id: i64) -> Message {
		self.wait_for(1, &|message| message.method.is_empty() && message.id == Some(json!(id)))
	}

	/// Blocks until `n` received messages match, and returns the nth.
	fn wait_for(&self, n: usize, matches: &dyn Fn(&Message) -> bool) -> Message {
		let deadline = Instant::now() + WAIT;
		let (lock, signal) = &*self.received;
		let mut received = lock.lock().expect("received");
		loop {
			if let Some(message) = received.iter().filter(|message| matches(message)).nth(n - 1) {
				return message.clone();
			}
			let now = Instant::now();
			assert!(
				now < deadline,
				"timed out waiting for message {n} matching the predicate"
			);
			received = signal.wait_timeout(received, deadline - now).expect("wait").0;
		}
	}

	fn count(&self, matches: &dyn Fn(&Message) -> bool) -> usize {
		self.received
			.0
			.lock()
			.expect("received")
			.iter()
			.filter(|message| matches(message))
			.count()
	}

	/// `handshake`: initialize and initialized; the initialize result.
	fn handshake(&self, encodings: Option<&[&str]>, init_options: Option<Value>) -> Value {
		let mut general = json!({});
		if let Some(encodings) = encodings {
			general["positionEncodings"] = json!(encodings);
		}
		let mut params = json!({"processId": null, "rootUri": null, "capabilities": {"general": general}});
		if let Some(options) = init_options {
			params["initializationOptions"] = options;
		}
		self.request(1, "initialize", params);
		let response = self.response_to(1);
		assert!(response.error.is_none(), "initialize failed: {:?}", response.error);
		self.notify("initialized", json!({}));
		response.result.expect("result")
	}

	fn did_open(&self, path: &Path, text: &str) {
		self.notify(
			"textDocument/didOpen",
			json!({"textDocument": {"uri": uri_of(path), "languageId": "typescript", "version": 1, "text": text}}),
		);
	}

	fn did_change(&self, path: &Path, version: i64, text: &str) {
		self.notify(
			"textDocument/didChange",
			json!({"textDocument": {"uri": uri_of(path), "version": version}, "contentChanges": [{"text": text}]}),
		);
	}

	fn did_save(&self, path: &Path) {
		self.notify("textDocument/didSave", json!({"textDocument": {"uri": uri_of(path)}}));
	}

	/// Round-trips an unknown request; the server handles messages in order, so its answer
	/// proves every earlier message was processed.
	fn ping(&self, id: i64) {
		self.request(id, "sloptor/ping", json!({}));
		self.response_to(id);
	}
}

fn slash(path: &Path) -> String {
	path.to_string_lossy().replace('\\', "/")
}

fn uri_of(path: &Path) -> String {
	rotor_lsp::lsconv::file_name_to_document_uri(&slash(path))
}

fn publish_for(uri: String) -> impl Fn(&Message) -> bool {
	move |message| message.method == "textDocument/publishDiagnostics" && message.params["uri"] == json!(uri)
}

fn is_show_message(message: &Message) -> bool {
	message.method == "window/showMessage"
}

/// A throwaway project layout; the files need not exist, the editor's buffers are compiled.
#[derive(Clone)]
struct Project {
	_root: Arc<tempfile::TempDir>,
	config: PathBuf,
	files: Vec<PathBuf>,
}

fn new_project(names: &[&str]) -> Project {
	let root = tempfile::tempdir().expect("temp dir");
	let config = root.path().join("tsconfig.json");
	let files = names.iter().map(|name| root.path().join("src").join(name)).collect();
	Project {
		_root: Arc::new(root),
		config,
		files,
	}
}

impl Project {
	fn find(&self) -> FindTsConfig {
		let config = self.config.to_string_lossy().into_owned();
		Arc::new(move |_: &str| Ok(config.clone()))
	}

	/// `noAnyCensus`: rejects the word `any` in each file's buffer at its byte offset, as the
	/// transformer rejects the type. It reads the overlays, so a result reflects what the editor
	/// sent.
	fn no_any(&self, overlays: &BTreeMap<String, String>) -> ProjectDiagnostics {
		let mut result = ProjectDiagnostics {
			config_path: slash(&self.config),
			..ProjectDiagnostics::default()
		};
		for file in &self.files {
			let file_name = slash(file);
			let mut entry = FileDiagnostics {
				file_name: file_name.clone(),
				outcome: FileOutcome::Ok,
				transformed: true,
				diagnostics: Vec::new(),
				internal_error: None,
			};
			let text = overlay_text(overlays, file);
			if let Some(offset) = text.find("any") {
				entry.outcome = FileOutcome::TransformerDiagnostic;
				entry.diagnostics = vec![DiagnosticInfo {
					code: "noAny".to_owned(),
					message: "Using values of type `any` is not supported!".to_owned(),
					file_name,
					offset,
					len: 3,
					..DiagnosticInfo::default()
				}];
			}
			result.files.push(entry);
		}
		result
	}

	fn no_any_census(&self) -> Census {
		let project = self.clone();
		Arc::new(move |_: &str, overlays: &BTreeMap<String, String>| (Some(project.no_any(overlays)), None))
	}
}

/// The compiler's case-insensitive lookup on Windows: a URI round trip normalizes the drive
/// letter's case, which must not lose the buffer.
fn overlay_text(overlays: &BTreeMap<String, String>, file: &Path) -> String {
	let key = file.to_string_lossy().into_owned();
	if let Some(text) = overlays.get(&key) {
		return text.clone();
	}
	if cfg!(windows) {
		for (path, text) in overlays {
			if path.eq_ignore_ascii_case(&key) {
				return text.clone();
			}
		}
	}
	String::new()
}

fn serve_config(census: Census, find: FindTsConfig, debounce: Duration) -> ServeConfig {
	ServeConfig { census, find, debounce }
}

/// An editor that cannot start the server: a handshake without full sync, the wrong name, or a
/// client's UTF-8 offer ignored (which would shift every squiggle).
#[test]
fn initialize_handshake() {
	let project = new_project(&["a.ts"]);
	{
		let client = start_server(serve_config(project.no_any_census(), project.find(), Duration::ZERO));
		let result = client.handshake(None, None);
		assert_eq!(result["serverInfo"]["name"], json!("sloptor"));
		let capabilities = &result["capabilities"];
		if let Some(encoding) = capabilities.get("positionEncoding") {
			assert_eq!(encoding, &json!("utf-16"), "utf-16 when the client offers nothing");
		}
		let sync = &capabilities["textDocumentSync"];
		assert_eq!(sync["change"], json!(1), "full sync");
		assert_eq!(sync["openClose"], json!(true));
		assert!(!sync["save"].is_null() && sync["save"] != json!(false), "save enabled");
	}
	let client = start_server(serve_config(project.no_any_census(), project.find(), Duration::ZERO));
	let result = client.handshake(Some(&["utf-8", "utf-16"]), None);
	assert_eq!(result["capabilities"]["positionEncoding"], json!("utf-8"));
}

/// A squiggle in the wrong place: a byte offset used as a character, or UTF-16 units miscounted
/// past an astral character (two units, four bytes) on the same line.
#[test]
fn transformer_diagnostic_range() {
	const TEXT: &str = "// héllo\nconst s = \"\u{1F600}\"; let x: any;";
	for (encodings, start, end) in [(None, 23, 26), (Some(&["utf-8"][..]), 25, 28)] {
		let project = new_project(&["a.ts"]);
		let client = start_server(serve_config(project.no_any_census(), project.find(), Duration::ZERO));
		client.handshake(encodings, None);
		client.did_open(&project.files[0], TEXT);
		let published = decode_publish(&client.wait_for(1, &publish_for(uri_of(&project.files[0]))));
		assert_eq!(published.diagnostics.len(), 1, "{:?}", published.diagnostics);
		let diagnostic = &published.diagnostics[0];
		assert_eq!(range_of(diagnostic), (1, start, 1, end), "{encodings:?}");
		assert_eq!(diagnostic["code"], json!("noAny"));
		assert_eq!(diagnostic["source"], json!("sloptor"));
		assert_eq!(diagnostic["severity"], json!(1));
		assert!(diagnostic["message"].as_str().unwrap_or_default().contains("any"));
	}
}

/// A stale squiggle: the editor is never told a fixed file is clean.
#[test]
fn fixed_file_is_cleared() {
	let project = new_project(&["a.ts"]);
	let client = start_server(serve_config(
		project.no_any_census(),
		project.find(),
		DEBOUNCE_FOR_TESTS,
	));
	client.handshake(None, None);
	let uri = uri_of(&project.files[0]);
	client.did_open(&project.files[0], "let x: any;");
	let first = decode_publish(&client.wait_for(1, &publish_for(uri.clone())));
	assert_eq!(first.diagnostics.len(), 1, "a diagnostic before the fix");
	client.did_change(&project.files[0], 2, "let x: unknown;");
	let cleared = decode_publish(&client.wait_for(2, &publish_for(uri.clone())));
	assert!(cleared.diagnostics.is_empty(), "{:?}", cleared.diagnostics);
	assert_eq!(cleared.uri, uri);
	assert_eq!(
		client.count(&publish_for(uri)),
		2,
		"a clean file is cleared once, not republished"
	);
}

/// TypeScript errors are the editor's own server's job unless the client opts in.
#[test]
fn typescript_errors_are_opt_in() {
	let census = |project: &Project| -> Census {
		let type_only = slash(&project.files[0]);
		let both = slash(&project.files[1]);
		Arc::new(move |_: &str, _: &BTreeMap<String, String>| {
			let info = |code: &str, message: &str, file: &str, offset: usize, warning: bool| DiagnosticInfo {
				code: code.to_owned(),
				message: message.to_owned(),
				file_name: file.to_owned(),
				offset,
				len: 1,
				warning,
				..DiagnosticInfo::default()
			};
			let files = vec![
				FileDiagnostics {
					file_name: type_only.clone(),
					outcome: FileOutcome::TypeError,
					transformed: true,
					diagnostics: vec![info(
						"TS2322",
						"Type 'string' is not assignable to type 'number'.",
						&type_only,
						4,
						false,
					)],
					internal_error: None,
				},
				FileDiagnostics {
					file_name: both.clone(),
					outcome: FileOutcome::TransformerDiagnostic,
					transformed: true,
					diagnostics: vec![
						info("TS2304", "Cannot find name 'q'.", &both, 0, false),
						info("noAny", "no any", &both, 0, false),
						// Warnings stay warnings.
						info("someWarning", "careful", &both, 0, true),
					],
					internal_error: None,
				},
			];
			(
				Some(ProjectDiagnostics {
					files,
					..ProjectDiagnostics::default()
				}),
				None,
			)
		})
	};

	let project = new_project(&["types.ts", "both.ts"]);
	{
		let client = start_server(serve_config(census(&project), project.find(), Duration::ZERO));
		client.handshake(None, None);
		client.did_open(&project.files[1], "q");
		// Program order: once the second file's publication arrived, the first's would have.
		let published = decode_publish(&client.wait_for(1, &publish_for(uri_of(&project.files[1]))));
		let codes: Vec<&str> = published
			.diagnostics
			.iter()
			.map(|diagnostic| diagnostic["code"].as_str().unwrap_or_default())
			.collect();
		assert_eq!(codes, ["noAny", "someWarning"], "only sloptor's own");
		assert_eq!(published.diagnostics[1]["severity"], json!(2));
		assert_eq!(
			client.count(&publish_for(uri_of(&project.files[0]))),
			0,
			"a file with only type errors gets no publication"
		);
	}
	let project = new_project(&["types.ts", "both.ts"]);
	let client = start_server(serve_config(census(&project), project.find(), Duration::ZERO));
	client.handshake(None, Some(json!({"typeErrors": true})));
	client.did_open(&project.files[1], "q");
	let type_only = decode_publish(&client.wait_for(1, &publish_for(uri_of(&project.files[0]))));
	assert_eq!(type_only.diagnostics.len(), 1);
	assert_eq!(type_only.diagnostics[0]["code"], json!("TS2322"));
	let both = decode_publish(&client.wait_for(1, &publish_for(uri_of(&project.files[1]))));
	assert_eq!(both.diagnostics.len(), 3);
}

/// A compiler crash going unreported: the user would conclude the file compiled.
#[test]
fn internal_compiler_error_is_published() {
	let project = new_project(&["a.ts"]);
	let file_name = slash(&project.files[0]);
	let census: Census = Arc::new(move |_: &str, _: &BTreeMap<String, String>| {
		(
			Some(ProjectDiagnostics {
				files: vec![FileDiagnostics {
					file_name: file_name.clone(),
					outcome: FileOutcome::InternalCompilerError,
					transformed: false,
					diagnostics: Vec::new(),
					internal_error: Some(InternalCompilerError {
						file_name: file_name.clone(),
						value: "assertion failed".to_owned(),
					}),
				}],
				..ProjectDiagnostics::default()
			}),
			None,
		)
	});
	let client = start_server(serve_config(census, project.find(), Duration::ZERO));
	client.handshake(None, None);
	client.did_open(&project.files[0], "let a = 1;\nlet b = 2;");
	let published = decode_publish(&client.wait_for(1, &publish_for(uri_of(&project.files[0]))));
	assert_eq!(published.diagnostics.len(), 1);
	let diagnostic = &published.diagnostics[0];
	assert_eq!(diagnostic["severity"], json!(1));
	assert_eq!(range_of(diagnostic), (0, 0, 0, 0));
	assert!(
		diagnostic["message"]
			.as_str()
			.unwrap_or_default()
			.contains("assertion failed")
	);
}

/// A popup storm: the same failure on every keystroke must be shown once, and again only after
/// it went away and came back.
#[test]
fn census_error_is_shown_once_per_occurrence() {
	let project = new_project(&["a.ts"]);
	let (started_tx, started) = mpsc::channel::<()>();
	let started_tx = Mutex::new(started_tx);
	let census: Census = {
		let project = project.clone();
		Arc::new(move |_: &str, overlays: &BTreeMap<String, String>| {
			let _ = started_tx.lock().expect("started").send(());
			if overlay_text(overlays, &project.files[0]).contains("fail") {
				return (
					Some(ProjectDiagnostics::default()),
					Some("tsconfig.json is unreadable".to_owned()),
				);
			}
			(Some(project.no_any(overlays)), None)
		})
	};
	let client = start_server(serve_config(census, project.find(), DEBOUNCE_FOR_TESTS));
	client.handshake(None, None);
	client.did_open(&project.files[0], "fail");
	started.recv_timeout(WAIT).expect("first census");
	client.wait_for(1, &is_show_message);
	client.did_save(&project.files[0]);
	started.recv_timeout(WAIT).expect("second census");
	client.did_change(&project.files[0], 2, "let x: any;");
	// A run starting means the one before it finished publishing.
	started.recv_timeout(WAIT).expect("third census");
	client.wait_for(1, &publish_for(uri_of(&project.files[0])));
	assert_eq!(
		client.count(&is_show_message),
		1,
		"one window message for one persistent failure"
	);

	client.did_change(&project.files[0], 3, "fail");
	let second = client.wait_for(2, &is_show_message);
	assert!(
		second.params.to_string().contains("tsconfig.json is unreadable"),
		"{}",
		second.params
	);
	assert_eq!(second.params["type"], json!(1), "error");
}

/// Project-level problems are shown, and the TypeScript ones hidden by default.
#[test]
fn project_diagnostics_are_shown() {
	let project = new_project(&["a.ts"]);
	let census: Census = Arc::new(|_: &str, _: &BTreeMap<String, String>| {
		(
			Some(ProjectDiagnostics {
				diagnostics: vec![
					DiagnosticInfo {
						message: "Rojo project not found".to_owned(),
						..DiagnosticInfo::default()
					},
					DiagnosticInfo {
						code: "TS5083".to_owned(),
						message: "Cannot read file tsconfig.base.json.".to_owned(),
						..DiagnosticInfo::default()
					},
				],
				..ProjectDiagnostics::default()
			}),
			None,
		)
	});
	let client = start_server(serve_config(census, project.find(), Duration::ZERO));
	client.handshake(None, None);
	client.did_open(&project.files[0], "x");
	let message = client.wait_for(1, &is_show_message);
	assert!(message.params.to_string().contains("Rojo project not found"));
	client.ping(10);
	assert_eq!(client.count(&is_show_message), 1);
}

/// A burst of edits behind a run costs exactly one more run, and it sees the newest text.
#[test]
fn edits_during_a_run_coalesce_into_one_more_run() {
	let project = new_project(&["a.ts"]);
	let calls = Arc::new(AtomicUsize::new(0));
	let (started_tx, started) = mpsc::channel::<()>();
	let (release_tx, release) = mpsc::channel::<()>();
	let gates = Mutex::new((started_tx, release));
	let census: Census = {
		let project = project.clone();
		let calls = Arc::clone(&calls);
		Arc::new(move |_: &str, overlays: &BTreeMap<String, String>| {
			if calls.fetch_add(1, Ordering::SeqCst) == 0 {
				let gates = gates.lock().expect("gates");
				let _ = gates.0.send(());
				let _ = gates.1.recv_timeout(WAIT);
			}
			(Some(project.no_any(overlays)), None)
		})
	};
	let client = start_server(serve_config(census, project.find(), DEBOUNCE_FOR_TESTS));
	client.handshake(None, None);
	client.did_open(&project.files[0], "let x = 1;");
	started.recv_timeout(WAIT).expect("the first census is in flight");
	for version in 2..52 {
		client.did_change(&project.files[0], version, "let x: unknown;");
	}
	client.did_change(&project.files[0], 52, "let last: any;");
	client.ping(10);
	release_tx.send(()).expect("release");
	let published = decode_publish(&client.wait_for(1, &publish_for(uri_of(&project.files[0]))));
	assert_eq!(published.diagnostics.len(), 1, "the follow-up run saw the newest text");
	client.ping(11);
	assert_eq!(
		calls.load(Ordering::SeqCst),
		2,
		"52 edits behind one run cost one more run"
	);
}

/// A debounce that never fires: the edit shows up once typing stops.
#[test]
fn typing_is_eventually_applied() {
	let project = new_project(&["a.ts"]);
	let calls = Arc::new(AtomicUsize::new(0));
	let (started_tx, started) = mpsc::channel::<()>();
	let started_tx = Mutex::new(started_tx);
	let census: Census = {
		let project = project.clone();
		let calls = Arc::clone(&calls);
		Arc::new(move |_: &str, overlays: &BTreeMap<String, String>| {
			calls.fetch_add(1, Ordering::SeqCst);
			let _ = started_tx.lock().expect("started").send(());
			(Some(project.no_any(overlays)), None)
		})
	};
	let client = start_server(serve_config(census, project.find(), DEBOUNCE_FOR_TESTS));
	client.handshake(None, None);
	client.did_open(&project.files[0], "let x: unknown;");
	started.recv_timeout(WAIT).expect("open census");
	client.did_change(&project.files[0], 2, "let x: any;");
	client.wait_for(1, &publish_for(uri_of(&project.files[0])));
	client.ping(10);
	assert_eq!(calls.load(Ordering::SeqCst), 2, "an open and one edit");
}

/// A scratch file outside every project neither takes the server down nor raises an error.
#[test]
fn file_outside_any_project_is_ignored() {
	let calls = Arc::new(AtomicUsize::new(0));
	let census: Census = {
		let calls = Arc::clone(&calls);
		Arc::new(move |_: &str, _: &BTreeMap<String, String>| {
			calls.fetch_add(1, Ordering::SeqCst);
			(Some(ProjectDiagnostics::default()), None)
		})
	};
	let find: FindTsConfig = Arc::new(|_: &str| Err("no tsconfig.json found".to_owned()));
	let client = start_server(serve_config(census, find, DEBOUNCE_FOR_TESTS));
	client.handshake(None, None);
	let project = new_project(&["scratch.ts"]);
	client.did_open(&project.files[0], "let x: any;");
	client.did_change(&project.files[0], 2, "let y: any;");
	client.did_save(&project.files[0]);
	client.ping(10);
	assert_eq!(calls.load(Ordering::SeqCst), 0);
	assert_eq!(client.count(&is_show_message), 0);
}

/// "Unsupported" said the right way: an unknown request gets `MethodNotFound` (or the client
/// waits forever), an unknown notification is dropped.
#[test]
fn unknown_messages() {
	let project = new_project(&["a.ts"]);
	let client = start_server(serve_config(project.no_any_census(), project.find(), Duration::ZERO));
	client.handshake(None, None);
	client.notify("workspace/didChangeConfiguration", json!({"settings": null}));
	client.notify("$/cancelRequest", json!({"id": 99}));
	client.notify("custom/unheardOf", json!({}));
	client.request(
		7,
		"textDocument/hover",
		json!({"textDocument": {"uri": uri_of(&project.files[0])}, "position": {"line": 0, "character": 0}}),
	);
	let response = client.response_to(7);
	assert_eq!(response.error.map(|error| error.0), Some(-32601));
	assert_eq!(
		client.count(&|message| message.method.is_empty() && message.error.is_some()),
		1,
		"only the request is answered with an error"
	);
}

/// The editor can stop the server: shutdown answers a null result (not none) and exit ends it.
#[test]
fn shutdown_and_exit() {
	let project = new_project(&["a.ts"]);
	let client = start_server(serve_config(project.no_any_census(), project.find(), Duration::ZERO));
	client.handshake(None, None);
	client.request(5, "shutdown", Value::Null);
	let response = client.response_to(5);
	assert!(response.error.is_none(), "{:?}", response.error);
	assert_eq!(response.result, Some(Value::Null), "a null result, written out");
	client.notify("exit", Value::Null);
	let done = client.done.lock().expect("done").take().expect("receiver");
	assert_eq!(done.recv_timeout(WAIT).expect("serve returned after exit"), Ok(()));
}

/// Requests before initialize are refused.
#[test]
fn request_before_initialize() {
	let project = new_project(&["a.ts"]);
	let client = start_server(serve_config(project.no_any_census(), project.find(), Duration::ZERO));
	client.request(3, "textDocument/hover", json!({}));
	assert_eq!(client.response_to(3).error.map(|error| error.0), Some(-32002));
}
