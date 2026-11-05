//! `publish.go`: one census as protocol traffic: per-file diagnostics, a clear for every file that
//! went clean, and window messages for what belongs to no file.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::Ordering;

use rotor_compile::census::{FileDiagnostics, ProjectDiagnostics};

use crate::lsconv::{self, LineMap};
use crate::protocol::{self, Diagnostic, Range};
use crate::server::{SERVER_NAME, Server, lock, source_path_key};

/// `internalErrorCode`: the code of the diagnostic standing in for a transform that panicked.
const INTERNAL_ERROR_CODE: &str = "internalCompilerError";

/// `isTypeScriptCode`: `TS####`, as opposed to one of sloptor's own factory names.
fn is_typescript_code(code: &str) -> bool {
	code.starts_with("TS")
}

impl Server {
	/// `publish`: `overlays` is the snapshot the census compiled, so each offset maps through the
	/// text it was measured against, however the client edited the buffer since.
	pub(crate) fn publish(
		&self,
		config: &str,
		overlays: &BTreeMap<String, String>,
		result: Option<&ProjectDiagnostics>,
		error: Option<&str>,
	) {
		if let Some(result) = result
			&& (error.is_none() || !result.files.is_empty())
		{
			self.publish_files(config, overlays, &result.files);
		}
		self.show_messages(config, result, error);
	}

	/// `publishFiles`, in program order so the same census always publishes the same sequence.
	fn publish_files(&self, config: &str, overlays: &BTreeMap<String, String>, files: &[FileDiagnostics]) {
		let mut next = Vec::new();
		let mut current = BTreeSet::new();
		for file in files {
			let diagnostics = self.file_diagnostics(file, overlays);
			if diagnostics.is_empty() {
				continue;
			}
			let uri = lsconv::file_name_to_document_uri(&file.file_name);
			current.insert(uri.clone());
			next.push((uri, diagnostics));
		}
		let previous = {
			let mut state = lock(&self.state);
			state
				.published
				.insert(config.to_owned(), current.clone())
				.unwrap_or_default()
		};
		for (uri, diagnostics) in &next {
			self.publish_diagnostics(uri, diagnostics);
		}
		for uri in previous.difference(&current) {
			self.publish_diagnostics(uri, &[]);
		}
	}

	fn publish_diagnostics(&self, uri: &str, diagnostics: &[Diagnostic]) {
		self.notify(
			"textDocument/publishDiagnostics",
			&protocol::publish_diagnostics_params(uri, diagnostics),
		);
	}

	/// `fileDiagnostics`: offsets refer to the open buffer when there is one, else the file on
	/// disk.
	fn file_diagnostics(&self, file: &FileDiagnostics, overlays: &BTreeMap<String, String>) -> Vec<Diagnostic> {
		if let Some(internal) = &file.internal_error {
			// The panic leaves no trustworthy span, and the file's other diagnostics came from a
			// transform that did not finish.
			return vec![new_diagnostic(
				Range::default(),
				INTERNAL_ERROR_CODE,
				&internal.to_string(),
				protocol::SEVERITY_ERROR,
			)];
		}
		let type_errors = self.type_errors.load(Ordering::SeqCst);
		let kept: Vec<_> = file
			.diagnostics
			.iter()
			.filter(|info| type_errors || !is_typescript_code(&info.code))
			.collect();
		if kept.is_empty() {
			return Vec::new();
		}
		let text = source_text(&file.file_name, overlays);
		let map = LineMap::new(&text);
		let utf8 = self.utf8.load(Ordering::SeqCst);
		kept.into_iter()
			.map(|info| {
				let severity = if info.warning {
					protocol::SEVERITY_WARNING
				} else {
					protocol::SEVERITY_ERROR
				};
				let range = lsconv::range(&text, &map, utf8, info.offset, info.offset + info.len);
				new_diagnostic(range, &info.code, &info.message, severity)
			})
			.collect()
	}

	/// `showMessages`: the census error and the project-level diagnostics, each shown once and
	/// again only after it went away and came back, so a persistent problem raises no popup per
	/// keystroke.
	fn show_messages(&self, config: &str, result: Option<&ProjectDiagnostics>, error: Option<&str>) {
		let mut messages: Vec<(String, u8)> = Vec::new();
		if let Some(error) = error {
			messages.push((error.to_owned(), protocol::MESSAGE_ERROR));
		}
		let type_errors = self.type_errors.load(Ordering::SeqCst);
		if let Some(result) = result {
			for info in &result.diagnostics {
				if is_typescript_code(&info.code) && !type_errors {
					continue;
				}
				let kind = if info.warning {
					protocol::MESSAGE_WARNING
				} else {
					protocol::MESSAGE_ERROR
				};
				messages.push((info.message.clone(), kind));
			}
		}
		let mut current = HashSet::new();
		let mut fresh = Vec::new();
		{
			let mut state = lock(&self.state);
			let previous = state.shown.get(config);
			for (text, kind) in messages {
				if !current.insert(text.clone()) {
					continue;
				}
				if !previous.is_some_and(|previous| previous.contains(&text)) {
					fresh.push((text, kind));
				}
			}
			state.shown.insert(config.to_owned(), current);
		}
		for (text, kind) in fresh {
			self.notify("window/showMessage", &protocol::show_message_params(kind, &text));
		}
	}
}

/// `sourceText`: the open buffer, else the disk; an unreadable file is empty text, which clamps
/// every position to the start rather than dropping the diagnostic.
fn source_text(file_name: &str, overlays: &BTreeMap<String, String>) -> String {
	let path = source_path_key(file_name);
	if let Some(text) = overlays.get(&path) {
		return text.clone();
	}
	std::fs::read(&path).map_or_else(|_| String::new(), |data| String::from_utf8_lossy(&data).into_owned())
}

fn new_diagnostic(range: Range, code: &str, message: &str, severity: u8) -> Diagnostic {
	Diagnostic {
		range,
		severity,
		code: code.to_owned(),
		source: SERVER_NAME.to_owned(),
		message: message.to_owned(),
	}
}
