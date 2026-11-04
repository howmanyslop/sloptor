//! `sloptor diagnostics` (`cmd/rotor/diagnostics.go`): the census counterpart of `check`. It reports
//! what happened to every file of a project, optionally over in-memory source overrides read from
//! stdin, and writes nothing. It exits 0 whenever it produced a census.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{IsTerminal, Read};
use std::time::{Duration, Instant};

use rotor_compile::ProjectOptions;
use rotor_compile::census::{FileOutcome, ProjectDiagnostics, compile_project_diagnostics};
use rotor_compile::census_solution::compile_solution_diagnostics;
use rotor_compile::compile::DiagnosticInfo;
use rotor_support::gopath;
use serde_json::Value;

use crate::VERSION;
use crate::app::{Failure, usage};
use crate::cli::DiagnosticsCommand;
use crate::frames::{self, Sources};
use crate::json::write_string;
use crate::options::{
	MergedOptions, PartialOptions, find_ts_config_path, merge_project_options, project_compile_options,
	read_rbxts_layer,
};
use crate::term::Streams;
use crate::ui::{self, Tone};

/// `diagnosticsCompileOptions`: the options a census runs with, shared with `lsp`. The tsconfig
/// `rbxts` key still decides the project's shape; `allowCommentDirectives` is the one option never
/// honoured, so a file leaning on `@ts-ignore` shows up as a transformer diagnostic.
///
/// # Errors
///
/// An unparsable config or `rbxts` value.
pub fn compile_options(ts_config_path: &str) -> Result<ProjectOptions, String> {
	let rbxts = read_rbxts_layer(ts_config_path)?;
	let mut merged = merge_project_options(MergedOptions::default(), &[rbxts.as_ref()]);
	merged.argv = Some(PartialOptions::default());
	merged.allow_comment_directives = false;
	Ok(project_compile_options(ts_config_path, &merged))
}

/// `readDiagnosticsRequest`: the overlays on stdin. An interactive terminal or an empty stream is
/// no overlays; an unknown key is an error, so a typo never censuses the unmodified tree.
///
/// # Errors
///
/// Unreadable input, input that is not the request object, or an unknown key.
pub fn read_request(input: &mut dyn Read, terminal: bool) -> Result<BTreeMap<String, String>, String> {
	if terminal {
		return Ok(BTreeMap::new());
	}
	let mut text = String::new();
	input
		.read_to_string(&mut text)
		.map_err(|error| format!("read overlay request from stdin: {error}"))?;
	if text.trim().is_empty() {
		return Ok(BTreeMap::new());
	}
	let parse = |message: String| format!("parse overlay request from stdin: {message}");
	// Go's decoder reads the first value and ignores what follows it.
	let value = serde_json::Deserializer::from_str(&text)
		.into_iter::<Value>()
		.next()
		.unwrap_or(Ok(Value::Null))
		.map_err(|error| parse(error.to_string()))?;
	let mut overlays = BTreeMap::new();
	let object = match value {
		Value::Null => return Ok(overlays),
		Value::Object(object) => object,
		other => {
			return Err(parse(format!(
				"json: cannot unmarshal {} into a request object",
				json_kind(&other)
			)));
		}
	};
	for (key, value) in object {
		// encoding/json matches field names without regard to case.
		if !key.eq_ignore_ascii_case("overlays") {
			return Err(parse(format!("json: unknown field \"{key}\"")));
		}
		match value {
			Value::Null => {}
			Value::Object(entries) => {
				for (path, text) in entries {
					let Value::String(text) = text else {
						return Err(parse(format!(
							"json: cannot unmarshal {} into the overlay of {path}",
							json_kind(&text)
						)));
					};
					overlays.insert(path, text);
				}
			}
			other => {
				return Err(parse(format!(
					"json: cannot unmarshal {} into overlays",
					json_kind(&other)
				)));
			}
		}
	}
	Ok(overlays)
}

const fn json_kind(value: &Value) -> &'static str {
	match value {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(_) => "number",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}

/// `sloptor diagnostics`
///
/// # Errors
///
/// A usage error, a setup failure, or no census at all.
pub fn run(command: DiagnosticsCommand, streams: &mut Streams) -> Result<(), Failure> {
	if command.project.as_deref() == Some("") {
		return Err(usage("diagnostics", "--project needs a value"));
	}
	if let (Some(path), Some(_)) = (&command.path, &command.project) {
		return Err(usage(
			"diagnostics",
			format!("unexpected extra argument '{path}' (the project is already set by --project)"),
		));
	}
	let build = command.build.is_some();
	if command.builders.is_some() && !build {
		return Err(usage("diagnostics", "--builders requires --build"));
	}
	let project = command.path.or(command.project).unwrap_or_else(|| ".".to_owned());
	let stdin = std::io::stdin();
	let terminal = stdin.is_terminal();
	let overlays = read_request(&mut stdin.lock(), terminal).map_err(Failure::Runtime)?;

	let project_path = command.build.filter(|path| !path.is_empty()).unwrap_or(project);
	let ts_config_path = find_ts_config_path(&project_path).map_err(Failure::Runtime)?;
	let dir = gopath::dir(&ts_config_path);
	let mut options = compile_options(&ts_config_path).map_err(Failure::Runtime)?;
	options.checkers = command.checkers;
	options.builders = command.builders;
	options.overlays.clone_from(&overlays);

	let json = command.json == Some(true);
	let out = streams.out_paint();
	if !json {
		ui::header(
			streams.out,
			out,
			&[
				if build { "diagnostics -b" } else { "diagnostics" },
				&if build {
					ui::rel_for_display(&ts_config_path)
				} else {
					ui::project_name(&ts_config_path)
				},
			],
		);
	}
	let started = Instant::now();
	let (projects, overlay_matches, error) = rotor_tsrs::with_large_stack(|| {
		if build {
			let (solution, error) = compile_solution_diagnostics(&ts_config_path, &options);
			(
				solution.projects,
				solution.overlay_matches,
				error.map(|error| error.to_string()),
			)
		} else {
			let (census, error) = compile_project_diagnostics(&dir, &options);
			let matches = census.overlay_matches;
			(vec![census], matches, error.map(|error| error.to_string()))
		}
	});
	let elapsed = started.elapsed();
	if json {
		let _ = streams
			.out
			.write_all(json_result(&projects, overlay_matches, error.as_deref(), elapsed, build).as_bytes());
	} else {
		text_report(streams, &projects, &overlays, error.as_deref(), elapsed, build);
	}
	if error.is_some() {
		return Err(Failure::Reported);
	}
	Ok(())
}

fn display(path: &str) -> String {
	ui::rel_for_display(&gopath::from_slash(path))
}

fn json_diagnostic(out: &mut String, diagnostic: &DiagnosticInfo) {
	let file = if diagnostic.file_name.is_empty() {
		String::new()
	} else {
		display(&diagnostic.file_name)
	};
	let (line, col) = if diagnostic.file_name.is_empty() {
		(0, 0)
	} else {
		(diagnostic.line, diagnostic.col)
	};
	out.push_str("{\"file\":");
	write_string(out, &file);
	let _ = write!(out, ",\"line\":{line},\"col\":{col}");
	if !diagnostic.code.is_empty() {
		out.push_str(",\"code\":");
		write_string(out, &diagnostic.code);
	}
	out.push_str(",\"severity\":");
	write_string(out, if diagnostic.warning { "warning" } else { "error" });
	out.push_str(",\"message\":");
	write_string(out, &diagnostic.message);
	out.push('}');
}

fn json_array<T>(out: &mut String, items: &[T], mut write: impl FnMut(&mut String, &T)) {
	out.push('[');
	for (index, item) in items.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		write(out, item);
	}
	out.push(']');
}

/// `writeDiagnosticsJSON`: the census as Go's encoder writes `jsonDiagnosticsResult`.
#[must_use]
pub fn json_result(
	projects: &[ProjectDiagnostics],
	overlay_matches: usize,
	error: Option<&str>,
	elapsed: Duration,
	build: bool,
) -> String {
	let mut ok = error.is_none();
	let mut diagnostics: Vec<DiagnosticInfo> = Vec::new();
	let mut files = 0;
	let mut transformed = 0;
	let mut project_entries = Vec::new();
	let mut file_entries = Vec::new();
	for census in projects {
		let mut project_ok = census.diagnostics.is_empty();
		files += census.files.len();
		transformed += census.transformed;
		diagnostics.extend(census.diagnostics.iter().cloned());
		let attribution = if build {
			display(&census.config_path)
		} else {
			String::new()
		};
		for file in &census.files {
			let mut entry = String::from("{\"file\":");
			write_string(&mut entry, &display(&file.file_name));
			if !attribution.is_empty() {
				entry.push_str(",\"project\":");
				write_string(&mut entry, &attribution);
			}
			entry.push_str(",\"outcome\":");
			write_string(&mut entry, file.outcome.as_str());
			let _ = write!(entry, ",\"transformed\":{},\"diagnostics\":", file.transformed);
			json_array(&mut entry, &file.diagnostics, json_diagnostic);
			if let Some(internal) = &file.internal_error {
				entry.push_str(",\"internalError\":{\"message\":");
				write_string(&mut entry, &internal.to_string());
				entry.push_str(",\"stack\":\"\"}");
			}
			entry.push('}');
			if file.outcome != FileOutcome::Ok {
				ok = false;
				project_ok = false;
			}
			file_entries.push(entry);
		}
		if build {
			let mut entry = String::from("{\"projectDir\":");
			write_string(&mut entry, &display(&census.project_dir));
			entry.push_str(",\"configPath\":");
			write_string(&mut entry, &display(&census.config_path));
			let _ = write!(
				entry,
				",\"ok\":{project_ok},\"files\":{},\"transformed\":{},\"overlayMatches\":{},\"diagnostics\":",
				census.files.len(),
				census.transformed,
				census.overlay_matches
			);
			json_array(&mut entry, &census.diagnostics, json_diagnostic);
			entry.push('}');
			project_entries.push(entry);
		}
	}
	if let Some(error) = error
		&& diagnostics.is_empty()
	{
		diagnostics.push(DiagnosticInfo {
			message: error.to_owned(),
			..DiagnosticInfo::default()
		});
	}
	if !diagnostics.is_empty() {
		ok = false;
	}
	let mut out = String::from("{\"version\":");
	write_string(&mut out, VERSION);
	let _ = write!(
		out,
		",\"ok\":{ok},\"files\":{files},\"durationMs\":{},\"diagnostics\":",
		elapsed.as_millis()
	);
	json_array(&mut out, &diagnostics, json_diagnostic);
	let _ = write!(
		out,
		",\"transformed\":{transformed},\"overlayMatches\":{overlay_matches}"
	);
	if build && !project_entries.is_empty() {
		out.push_str(",\"projects\":[");
		out.push_str(&project_entries.join(","));
		out.push(']');
	}
	out.push_str(",\"fileDiagnostics\":[");
	out.push_str(&file_entries.join(","));
	out.push_str("]}\n");
	out
}

const fn outcome_word(outcome: FileOutcome) -> (Tone, &'static str) {
	match outcome {
		FileOutcome::Ok => (Tone::Success, "Ok"),
		FileOutcome::TypeError => (Tone::Failure, "Type error"),
		FileOutcome::TransformerDiagnostic => (Tone::Warning, "Transformer"),
		FileOutcome::InternalCompilerError => (Tone::Failure, "Internal"),
	}
}

/// The census for humans: each failing file with its frames, the project-level diagnostics, then
/// the counts per outcome. A census that could not be produced is an error on stderr.
fn text_report(
	streams: &mut Streams,
	projects: &[ProjectDiagnostics],
	overlays: &BTreeMap<String, String>,
	error: Option<&str>,
	elapsed: Duration,
	build: bool,
) {
	let out = streams.out_paint();
	let err = streams.err_paint();
	let mut sources = Sources::new(Some(overlays));
	let mut counts = [0usize; 4];
	let (mut files, mut transformed) = (0, 0);
	for census in projects {
		if build {
			ui::verb_line(
				streams.out,
				out,
				Tone::Info,
				"Project",
				&format!(
					"{} {}",
					out.bold(&ui::project_name(&gopath::from_slash(&census.config_path))),
					out.muted(&format!("({} files)", census.files.len()))
				),
			);
		}
		files += census.files.len();
		transformed += census.transformed;
		for file in &census.files {
			counts[match file.outcome {
				FileOutcome::Ok => 0,
				FileOutcome::TypeError => 1,
				FileOutcome::TransformerDiagnostic => 2,
				FileOutcome::InternalCompilerError => 3,
			}] += 1;
			if file.outcome == FileOutcome::Ok {
				continue;
			}
			let (tone, word) = outcome_word(file.outcome);
			ui::verb_line(streams.out, out, tone, word, &out.bold(&display(&file.file_name)));
			let mut text = String::new();
			let frames: Vec<_> = file
				.diagnostics
				.iter()
				.map(|diagnostic| sources.frame(diagnostic))
				.collect();
			frames::render_all(&mut text, out, &frames, 0);
			if let Some(internal) = &file.internal_error {
				let _ = writeln!(
					text,
					"{}{}\n",
					out.red("error[internal]"),
					out.bold(&format!(": {}", internal.value))
				);
			}
			let _ = streams.out.write_all(text.as_bytes());
		}
		if !census.diagnostics.is_empty() {
			let frames: Vec<_> = census
				.diagnostics
				.iter()
				.map(|diagnostic| sources.frame(diagnostic))
				.collect();
			let mut text = String::new();
			frames::render_all(&mut text, out, &frames, 0);
			let _ = streams.out.write_all(text.as_bytes());
		}
	}
	if error.is_none() || build {
		summary(streams.out, out, counts, files, transformed, elapsed);
	}
	if let Some(error) = error {
		let _ = writeln!(streams.err, "{} census failed: {error}", err.red("error:"));
	}
}

/// The closing counts of a census.
fn summary(
	out_stream: &mut dyn std::io::Write,
	out: ui::Paint,
	counts: [usize; 4],
	files: usize,
	transformed: usize,
	elapsed: Duration,
) {
	let [ok, typed, transformer_diagnostics, internal] = counts;
	ui::verb_line(
		out_stream,
		out,
		Tone::Success,
		"Censused",
		&ui::summary_parts(
			out,
			&[
				format!(
					"{} in {}",
					out.bold(&ui::plural(files, "file")),
					out.bold(&ui::duration(elapsed))
				),
				format!("{} transformed", ui::thousands(transformed)),
			],
		),
	);
	ui::continuation(
		out_stream,
		&ui::summary_parts(
			out,
			&[
				out.green(&format!("ok {ok}")),
				tinted(out, typed, &format!("typeError {typed}"), false),
				tinted(
					out,
					transformer_diagnostics,
					&format!("transformerDiagnostic {transformer_diagnostics}"),
					true,
				),
				tinted(out, internal, &format!("internalCompilerError {internal}"), false),
			],
		),
	);
}

fn tinted(paint: ui::Paint, count: usize, text: &str, warning: bool) -> String {
	match (count, warning) {
		(0, _) => paint.muted(text),
		(_, true) => paint.yellow_plain(text),
		(_, false) => paint.red_plain(text),
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use super::read_request;

	#[test]
	fn the_request_reads_overlays_and_rejects_unknown_keys() {
		let read = |text: &str| read_request(&mut text.as_bytes(), false);
		assert_eq!(read(""), Ok(BTreeMap::default()));
		assert_eq!(read("  \n"), Ok(BTreeMap::default()));
		assert_eq!(read("null"), Ok(BTreeMap::default()));
		let overlays = read(r#"{"overlays":{"/a.ts":"x"}} trailing"#).expect("parses");
		assert_eq!(overlays.get("/a.ts").map(String::as_str), Some("x"));
		assert!(read(r#"{"Overlays":{}}"#).is_ok());
		assert!(read(r#"{"overlay":{}}"#).is_err_and(|error| error.contains("unknown field")));
		assert!(read("[1]").is_err());
		assert!(read("{").is_err_and(|error| error.starts_with("parse overlay request from stdin:")));
		assert_eq!(read_request(&mut &b"{"[..], true), Ok(BTreeMap::default()));
	}
}
