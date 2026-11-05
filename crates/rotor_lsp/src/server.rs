//! `server.go`: the message loop, the document notifications, and every write to the client.
//!
//! The loop runs on the calling thread; a helper thread reads frames, so a cancellation ends the
//! session even while the client is silent. Censuses run on the scheduler's threads, so what they
//! share with the loop is behind locks and every write to the client goes through [`Server::send`].

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rotor_compile::census::ProjectDiagnostics;
use rotor_support::gopath;
use serde_json::Value;

use crate::jsonrpc::{self, ReadError};
use crate::protocol::{self, Id, RawMessage};
use crate::scheduler::Scheduler;

/// `serverName`: `serverInfo.name` and every diagnostic's source.
pub const SERVER_NAME: &str = "sloptor";

/// `defaultDebounce`: how long typing must go quiet before a census runs.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(250);

/// Maps an absolute file path (OS separators) to the tsconfig that owns it.
pub type FindTsConfig = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

/// Compiles the project at a tsconfig with the given overlays (absolute path to buffer text) and
/// returns its census. The census may come with an error.
pub type Census =
	Arc<dyn Fn(&str, &BTreeMap<String, String>) -> (Option<ProjectDiagnostics>, Option<String>) + Send + Sync>;

/// `Options`: the compile step is injected, so this crate owns the protocol, scheduling and
/// position mapping and nothing else.
#[derive(Clone)]
pub struct Options {
	/// `FindTsConfig`
	pub find_ts_config: FindTsConfig,
	/// `Census`
	pub census: Census,
	/// The delay after `didChange` before a census; zero means [`DEFAULT_DEBOUNCE`].
	pub debounce: Duration,
}

enum Incoming {
	Frame(Result<Vec<u8>, ReadError>),
	Cancelled,
}

/// Ends a session from outside (Go's context).
#[derive(Clone, Default)]
pub struct Cancel {
	inner: Arc<Mutex<(bool, Vec<Sender<Incoming>>)>>,
}

impl Cancel {
	/// Ends every session serving this token.
	pub fn cancel(&self) {
		let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
		inner.0 = true;
		for sender in inner.1.drain(..) {
			let _ = sender.send(Incoming::Cancelled);
		}
	}

	fn register(&self, sender: Sender<Incoming>) {
		let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
		if inner.0 {
			let _ = sender.send(Incoming::Cancelled);
		} else {
			inner.1.push(sender);
		}
	}
}

/// The state censuses share with the loop.
#[derive(Default)]
pub struct State {
	/// The text of every open source buffer, keyed by absolute path with OS separators.
	pub(crate) open: HashMap<String, String>,
	/// The tsconfig of each open buffer, resolved once at `didOpen`.
	pub(crate) configs: HashMap<String, String>,
	/// Per tsconfig, the URIs last published with a non-empty list.
	pub(crate) published: HashMap<String, BTreeSet<String>>,
	/// Per tsconfig, the window messages on screen.
	pub(crate) shown: HashMap<String, HashSet<String>>,
}

/// One session.
pub struct Server {
	options: Options,
	out: Mutex<Box<dyn Write + Send>>,
	failure: Mutex<Option<String>>,
	cancelled: AtomicBool,
	wake: Mutex<Option<Sender<Incoming>>>,
	/// Negotiated at `initialize`, before any census.
	pub(crate) utf8: AtomicBool,
	/// `initializationOptions.typeErrors`
	pub(crate) type_errors: AtomicBool,
	pub(crate) state: Mutex<State>,
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `Serve`: runs the server until the client sends `exit`, the input ends, or `cancel` fires.
/// Nothing but LSP messages is written to `output`.
///
/// # Errors
///
/// `context canceled` when `cancel` ends the session, the first write failure, or a transport
/// error reading `input`. `exit` and the end of input are not errors.
pub fn serve(
	cancel: &Cancel,
	input: impl Read + Send + 'static,
	output: impl Write + Send + 'static,
	mut options: Options,
) -> Result<(), String> {
	if options.debounce.is_zero() {
		options.debounce = DEFAULT_DEBOUNCE;
	}
	let (sender, receiver) = mpsc::channel::<Incoming>();
	let server = Arc::new(Server {
		options: options.clone(),
		out: Mutex::new(Box::new(output)),
		failure: Mutex::new(None),
		cancelled: AtomicBool::new(false),
		wake: Mutex::new(Some(sender.clone())),
		utf8: AtomicBool::new(false),
		type_errors: AtomicBool::new(false),
		state: Mutex::new(State::default()),
	});
	let mut scheduler = {
		let server = Arc::clone(&server);
		Scheduler::new(
			options.debounce,
			Arc::new(move |config: &str| server.run_census(config)),
		)
	};
	cancel.register(sender.clone());
	let reader = sender;
	let spawned = std::thread::Builder::new()
		.name("rotor-lsp-reader".to_owned())
		.spawn(move || {
			let mut reader_input = jsonrpc::Reader::new(input);
			loop {
				let frame = reader_input.read();
				let end = frame.is_err();
				if reader.send(Incoming::Frame(frame)).is_err() || end {
					return;
				}
			}
		});
	if let Err(error) = spawned {
		return Err(format!("lsp: start reader: {error}"));
	}

	let mut session = Session {
		server: &server,
		scheduler: &scheduler,
		initialized: false,
		shutdown: false,
	};
	let result = loop {
		match receiver.recv() {
			Ok(Incoming::Frame(Ok(data))) => {
				if session.handle(&data) {
					break Ok(());
				}
			}
			Ok(Incoming::Frame(Err(ReadError::Eof))) => break Ok(()),
			Ok(Incoming::Frame(Err(error))) => break Err(error.to_string()),
			Ok(Incoming::Cancelled) | Err(_) => break Err("context canceled".to_owned()),
		}
	};
	// Stop publishing first so a run in flight drops its result, then wait for it, so nothing
	// writes to the output after `serve` returns.
	server.cancelled.store(true, Ordering::SeqCst);
	*lock(&server.wake) = None;
	scheduler.close();
	let failure = lock(&server.failure).clone();
	failure.map_or(result, Err)
}

/// The loop's own state.
struct Session<'a> {
	server: &'a Arc<Server>,
	scheduler: &'a Scheduler,
	initialized: bool,
	shutdown: bool,
}

impl Session<'_> {
	/// `handle`: one message; true for `exit`.
	fn handle(&mut self, data: &[u8]) -> bool {
		let raw = match protocol::parse_raw(data) {
			Ok(raw) => raw,
			Err(error) => {
				self.server
					.respond_error(None, protocol::INVALID_REQUEST, &format!("invalid request: {error}"));
				return false;
			}
		};
		if raw.is_response() {
			// The server sends no requests, so there is nothing to correlate.
			return false;
		}
		if raw.is_request() && !self.initialized && raw.method != "initialize" {
			self.server.respond_error(
				raw.id.as_ref(),
				protocol::SERVER_NOT_INITIALIZED,
				"server not initialized",
			);
			return false;
		}
		// Params Go's `lsproto` fails to decode: initialize and shutdown proceed without them, any
		// other request is refused with `CodeInvalidParams`, and a notification is dropped.
		if raw
			.params
			.as_ref()
			.is_some_and(|params| crate::params::rejects(&raw.method, params))
			&& raw.method != "initialize"
			&& raw.method != "shutdown"
		{
			if raw.is_request() {
				self.server.respond_error(
					raw.id.as_ref(),
					protocol::INVALID_PARAMS,
					&format!("invalid params for {}", raw.method),
				);
			}
			return raw.method == "exit";
		}
		if raw.is_notification() {
			return self.handle_notification(&raw);
		}
		self.handle_request(&raw);
		false
	}

	fn handle_request(&mut self, request: &RawMessage) {
		let id = request.id.as_ref();
		if request.method == "initialize" {
			self.handle_initialize(id, request.params.as_ref());
		} else if !self.initialized {
			self.server
				.respond_error(id, protocol::SERVER_NOT_INITIALIZED, "server not initialized");
		} else if self.shutdown {
			self.server
				.respond_error(id, protocol::INVALID_REQUEST, "server is shutting down");
		} else if request.method == "shutdown" {
			self.shutdown = true;
			self.server.respond(id, "null");
		} else {
			self.server.respond_error(
				id,
				protocol::METHOD_NOT_FOUND,
				&format!("method not found: {}", request.method),
			);
		}
	}

	fn handle_notification(&self, request: &RawMessage) -> bool {
		if request.method == "exit" {
			return true;
		}
		if !self.initialized {
			return false;
		}
		let Some(params) = request.params.as_ref() else {
			return false;
		};
		match request.method.as_str() {
			"textDocument/didOpen" => {
				if let Some((uri, text)) = protocol::did_open(params) {
					self.did_open(&uri, text);
				}
			}
			"textDocument/didChange" => {
				if let Some((uri, Some(text))) = protocol::did_change(params) {
					self.did_change(&uri, text);
				}
			}
			"textDocument/didSave" => {
				if let Some((uri, text)) = protocol::did_save(params) {
					self.did_save(&uri, text);
				}
			}
			"textDocument/didClose" => {
				if let Some(uri) = protocol::did_close(params) {
					self.did_close(&uri);
				}
			}
			// `$/cancelRequest`: every request is answered inline, so nothing is left to cancel.
			// Unknown notifications are ignored, as the protocol requires.
			_ => {}
		}
		false
	}

	/// `handleInitialize`: the position encoding and the one sloptor option.
	fn handle_initialize(&mut self, id: Option<&Id>, params: Option<&Value>) {
		if self.initialized {
			self.server
				.respond_error(id, protocol::INVALID_REQUEST, "server already initialized");
			return;
		}
		let encodings = params.and_then(protocol::initialize_encodings).unwrap_or_default();
		if encodings.iter().any(|encoding| encoding == "utf-8") {
			self.server.utf8.store(true, Ordering::SeqCst);
		}
		self.server
			.type_errors
			.store(protocol::type_errors_option(params), Ordering::SeqCst);
		self.initialized = true;
		let encoding = if self.server.utf8.load(Ordering::SeqCst) {
			"utf-8"
		} else {
			"utf-16"
		};
		self.server
			.respond(id, &protocol::initialize_result(encoding, SERVER_NAME));
	}

	/// `didOpen`: records the buffer and runs a census at once.
	fn did_open(&self, uri: &str, text: String) {
		let Some(path) = source_path(uri) else {
			return;
		};
		// A file outside any project has nothing for sloptor to say.
		let Ok(config) = (self.server.options.find_ts_config)(&path) else {
			return;
		};
		{
			let mut state = lock(&self.server.state);
			state.open.insert(path.clone(), text);
			state.configs.insert(path, config.clone());
		}
		self.scheduler.schedule(&config, true);
	}

	/// `didChange`: full sync, so the last whole-document change is the new text; debounced.
	fn did_change(&self, uri: &str, text: String) {
		let Some(path) = source_path(uri) else {
			return;
		};
		let config = {
			let mut state = lock(&self.server.state);
			let config = state.configs.get(&path).cloned();
			if config.is_some() {
				state.open.insert(path, text);
			}
			config
		};
		if let Some(config) = config {
			self.scheduler.schedule(&config, false);
		}
	}

	/// `didSave`: runs a census at once; a saved text refreshes the buffer.
	fn did_save(&self, uri: &str, text: Option<String>) {
		let Some(path) = source_path(uri) else {
			return;
		};
		let config = {
			let mut state = lock(&self.server.state);
			let config = state.configs.get(&path).cloned();
			if config.is_some()
				&& let Some(text) = text
			{
				state.open.insert(path, text);
			}
			config
		};
		if let Some(config) = config {
			self.scheduler.schedule(&config, true);
		}
	}

	/// `didClose`: drops the buffer and re-runs the census, so the project reflects the disk.
	fn did_close(&self, uri: &str) {
		let Some(path) = source_path(uri) else {
			return;
		};
		let config = {
			let mut state = lock(&self.server.state);
			state.open.remove(&path);
			state.configs.remove(&path)
		};
		if let Some(config) = config {
			self.scheduler.schedule(&config, true);
		}
	}
}

/// `sourcePath`: a `file://` URI of a TypeScript source as the overlay key; anything else
/// (untitled buffers, `package.json`, Luau) is not sloptor's concern.
fn source_path(uri: &str) -> Option<String> {
	if !uri.starts_with("file://") {
		return None;
	}
	let file_name = crate::lsconv::document_uri_to_file_name(uri)?;
	match gopath::ext(uri).to_lowercase().as_str() {
		".ts" | ".tsx" | ".mts" | ".cts" => Some(source_path_key(&file_name)),
		_ => None,
	}
}

/// `sourcePathKey`: one key form for open buffers and overlays (a URI round trip normalizes a
/// Windows drive letter's case; compiler results may keep the original).
pub fn source_path_key(path: &str) -> String {
	let path = gopath::clean(&gopath::from_slash(path));
	if cfg!(windows) { path.to_lowercase() } else { path }
}

impl Server {
	/// `snapshotOverlays`: a stable copy of every open buffer, for the census and for mapping
	/// its offsets.
	fn snapshot_overlays(&self) -> BTreeMap<String, String> {
		lock(&self.state)
			.open
			.iter()
			.map(|(path, text)| (path.clone(), text.clone()))
			.collect()
	}

	/// `runCensus`: one census of one tsconfig, then its publication.
	fn run_census(&self, config: &str) {
		let overlays = self.snapshot_overlays();
		let (result, error) = self.census(config, &overlays);
		if self.cancelled.load(Ordering::SeqCst) {
			return;
		}
		self.publish(config, &overlays, result.as_ref(), error.as_deref());
	}

	/// `census`: a panic in the compile step must not take down a long-lived editor process.
	fn census(
		&self,
		config: &str,
		overlays: &BTreeMap<String, String>,
	) -> (Option<ProjectDiagnostics>, Option<String>) {
		let census = Arc::clone(&self.options.census);
		let run = std::panic::AssertUnwindSafe(|| census(config, overlays));
		std::panic::catch_unwind(run).unwrap_or_else(|payload| {
			let value = payload
				.downcast_ref::<&str>()
				.map(|text| (*text).to_owned())
				.or_else(|| payload.downcast_ref::<String>().cloned())
				.unwrap_or_default();
			(None, Some(format!("sloptor: census panicked: {value}")))
		})
	}

	pub(crate) fn respond(&self, id: Option<&Id>, result: &str) {
		self.send(&protocol::response(id, result));
	}

	pub(crate) fn respond_error(&self, id: Option<&Id>, code: i32, message: &str) {
		self.send(&protocol::error_response(id, code, message));
	}

	pub(crate) fn notify(&self, method: &str, params: &str) {
		self.send(&protocol::notification(method, params));
	}

	/// `send`: one framed message. A failed write means the client is gone, so it ends the
	/// session.
	fn send(&self, message: &str) {
		let written = {
			let mut out = lock(&self.out);
			jsonrpc::write(&mut **out, message.as_bytes())
		};
		if let Err(error) = written {
			self.fail(format!("lsp: write message: {error}"));
		}
	}

	fn fail(&self, error: String) {
		{
			let mut failure = lock(&self.failure);
			if failure.is_none() {
				*failure = Some(error);
			}
		}
		self.cancelled.store(true, Ordering::SeqCst);
		if let Some(wake) = lock(&self.wake).as_ref() {
			let _ = wake.send(Incoming::Cancelled);
		}
	}
}
