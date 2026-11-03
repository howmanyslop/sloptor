//! The `rotor-rs` binary, the Rust port of the rotor CLI.
//!
//! Stage 5b ports `rotor build` with `--project`, `--build`, `--json` and `--verbose`, wired as
//! `cmd/rotor` wires them. The rest of the surface is Stage 7.

use std::io::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rotor_compile::compile::DiagnosticInfo;
use rotor_compile::output::{BuildFailure, BuildResult};
use rotor_compile::rbxts_options::{RbxtsOptions, read_rbxts_options};
use rotor_compile::{ProjectOptions, build_project_with_options, build_solution_with_options};
use rotor_support::diagframe::{self, Group, Language, Spot};
use rotor_support::term::Styler;
use rotor_support::{gopath, logservice};

#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

/// `version`
const VERSION: &str = rotor_support::version::VERSION;

/// The parsed `build` argv: the project path and the flags passed.
#[derive(Default)]
struct BuildArgs {
	project: String,
	build: bool,
	build_path: String,
	json: bool,
	version: bool,
	/// `--verbose`, when passed.
	verbose: Option<bool>,
}

/// A failure that ends the command with exit code 1.
enum CommandFailure {
	/// A usage error: red `error:` and the help hint.
	Usage(String),
	/// An operational failure: red `error:`.
	Runtime(String),
	/// Already rendered.
	Reported,
}

fn main() -> ExitCode {
	let args: Vec<String> = std::env::args().skip(1).collect();
	match execute(&args) {
		Ok(()) => ExitCode::SUCCESS,
		Err(failure) => {
			let mut stderr = std::io::stderr();
			let styler = Styler::for_writer(&stderr);
			match failure {
				CommandFailure::Usage(message) => {
					let _ = writeln!(stderr, "{} {message}", styler.error("error:"));
					let _ = writeln!(stderr, "{}", styler.muted("Run 'rotor-rs build --help' for usage."));
				}
				CommandFailure::Runtime(message) => {
					let _ = writeln!(stderr, "{} {message}", styler.error("error:"));
				}
				CommandFailure::Reported => {}
			}
			ExitCode::FAILURE
		}
	}
}

fn execute(args: &[String]) -> Result<(), CommandFailure> {
	match args.first().map(String::as_str) {
		Some("build") => run_build_command(&args[1..]),
		Some("--version" | "-v") => {
			let _ = writeln!(std::io::stdout(), "{VERSION}");
			Ok(())
		}
		Some(other) => Err(CommandFailure::Usage(format!(
			"unknown command \"{other}\" for \"rotor-rs\""
		))),
		None => Err(CommandFailure::Usage("rotor-rs requires a command: build".to_owned())),
	}
}

/// A boolean flag's value: bare is true, `=<bool>` parses like `strconv.ParseBool`.
fn bool_value(name: &str, value: Option<&str>) -> Result<bool, CommandFailure> {
	match value {
		None | Some("true" | "1" | "t" | "T" | "TRUE" | "True") => Ok(true),
		Some("false" | "0" | "f" | "F" | "FALSE" | "False") => Ok(false),
		Some(other) => Err(CommandFailure::Usage(format!(
			"invalid argument \"{other}\" for \"--{name}\" flag: strconv.ParseBool: parsing \"{other}\": invalid syntax"
		))),
	}
}

/// `collectBuildArgs` over `normalizeLegacyArgs`: `--build` and `--project` take the next token
/// unless it is flag-like; booleans accept `=<bool>` and `--no-` negation.
fn parse_build_args(args: &[String]) -> Result<BuildArgs, CommandFailure> {
	let mut parsed = BuildArgs {
		project: ".".to_owned(),
		..BuildArgs::default()
	};
	let mut project_set = false;
	let mut positional: Option<String> = None;
	let mut index = 0;
	// A string flag takes its `=` value, else the next token unless it is flag-like.
	let take_value = |value: Option<&str>, index: &mut usize| -> String {
		if let Some(value) = value {
			return value.to_owned();
		}
		args.get(*index + 1)
			.filter(|next| !next.starts_with('-'))
			.map_or_else(String::new, |next| {
				*index += 1;
				next.clone()
			})
	};
	while index < args.len() {
		let arg = &args[index];
		if arg == "--" {
			if let Some(rest) = args.get(index + 1) {
				positional.get_or_insert_with(|| rest.clone());
			}
			break;
		}
		if !arg.starts_with('-') || arg == "-" {
			if positional.is_some() {
				return Err(CommandFailure::Usage(format!(
					"accepts at most 1 arg(s), received {}",
					args.iter().filter(|arg| !arg.starts_with('-')).count()
				)));
			}
			positional = Some(arg.clone());
			index += 1;
			continue;
		}
		let (name, value) = match arg.split_once('=') {
			Some((name, value)) => (name, Some(value)),
			None => (arg.as_str(), None),
		};
		match name {
			"--project" | "-p" => {
				project_set = true;
				parsed.project = take_value(value, &mut index);
			}
			"--build" | "-b" => {
				parsed.build = true;
				parsed.build_path = take_value(value, &mut index);
			}
			"--json" => parsed.json = bool_value("json", value)?,
			"--no-json" => parsed.json = !bool_value("json", value)?,
			"--verbose" => parsed.verbose = Some(bool_value("verbose", value)?),
			"--no-verbose" => parsed.verbose = Some(!bool_value("verbose", value)?),
			"--version" | "-v" => parsed.version = bool_value("version", value)?,
			_ => {
				return Err(CommandFailure::Usage(format!("unknown flag: {name}")));
			}
		}
		index += 1;
	}
	if let Some(positional) = positional {
		if project_set {
			return Err(CommandFailure::Usage(format!(
				"unexpected extra argument \"{positional}\" (project already set via --project)"
			)));
		}
		parsed.project = positional;
	}
	Ok(parsed)
}

/// `findTsConfigPath`: the argument if it is a file, else the nearest `tsconfig.json` upward.
fn find_ts_config_path(project: &str) -> Result<String, String> {
	let resolved = gopath::abs(project).map_err(|error| error.to_string())?;
	if std::fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_file()) {
		return Ok(resolved);
	}
	let mut dir = resolved;
	loop {
		let candidate = gopath::join(&[&dir, "tsconfig.json"]);
		if std::fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()) {
			return Ok(candidate);
		}
		let parent = gopath::dir(&dir);
		if parent == dir {
			return Err("Unable to find tsconfig.json!".to_owned());
		}
		dir = parent;
	}
}

/// `projectOptions`: the merged option set a build runs with (the subset this CLI sets).
#[expect(clippy::struct_excessive_bools, reason = "Go's projectOptions fields")]
struct MergedOptions {
	include_path: String,
	rojo: String,
	type_name: String,
	verbose: bool,
	no_include: bool,
	log_truthy_changes: bool,
	optimized_array_appends: bool,
	optimized_loops: bool,
	allow_comment_directives: bool,
	luau: bool,
	luau_types: bool,
}

/// `mergeProjectOptions(defaultProjectOptions, rbxts, argv)`
fn merge_project_options(rbxts: Option<&RbxtsOptions>, verbose: Option<bool>) -> MergedOptions {
	let mut merged = MergedOptions {
		include_path: String::new(),
		rojo: String::new(),
		type_name: String::new(),
		verbose: false,
		no_include: false,
		log_truthy_changes: false,
		optimized_array_appends: false,
		optimized_loops: true,
		allow_comment_directives: false,
		luau: true,
		luau_types: false,
	};
	if let Some(rbxts) = rbxts {
		if let Some(value) = &rbxts.include_path {
			merged.include_path.clone_from(value);
		}
		if let Some(value) = &rbxts.rojo {
			merged.rojo.clone_from(value);
		}
		if let Some(value) = &rbxts.project_type {
			merged.type_name.clone_from(value);
		}
		merged.no_include = rbxts.no_include.unwrap_or(merged.no_include);
		merged.log_truthy_changes = rbxts.log_truthy_changes.unwrap_or(merged.log_truthy_changes);
		merged.optimized_array_appends = rbxts.optimized_array_appends.unwrap_or(merged.optimized_array_appends);
		merged.optimized_loops = rbxts.optimized_loops.unwrap_or(merged.optimized_loops);
		merged.allow_comment_directives = rbxts
			.allow_comment_directives
			.unwrap_or(merged.allow_comment_directives);
		merged.luau = rbxts.luau.unwrap_or(merged.luau);
		merged.luau_types = rbxts.luau_types.unwrap_or(merged.luau_types);
	}
	if let Some(verbose) = verbose {
		merged.verbose = verbose;
	}
	merged
}

/// `projectCompileOptions`: a one-shot build, so the parallel transformer token is set.
fn project_compile_options(ts_config_path: &str, merged: &MergedOptions) -> ProjectOptions {
	let mut options = ProjectOptions::default();
	ts_config_path.clone_into(&mut options.ts_config_path);
	options.include_path.clone_from(&merged.include_path);
	options.emit_include_files = !merged.no_include;
	options.project_type = rotor_compile::project::parse_project_type(&merged.type_name);
	options.rojo_config_path.clone_from(&merged.rojo);
	options.log_truthy_changes = merged.log_truthy_changes;
	options.allow_comment_directives = merged.allow_comment_directives;
	options.optimized_array_appends = merged.optimized_array_appends;
	options.no_optimized_loops = !merged.optimized_loops;
	options.lua_extension = !merged.luau;
	options.luau_types = merged.luau_types;
	// The argv layer: no rbxts flag of this CLI was passed.
	options.solution_argv = Some(RbxtsOptions::default());
	rotor_compile::sidecar_parallel::mark_one_shot_cli_transformer_build(&mut options);
	options
}

fn run_build_command(args: &[String]) -> Result<(), CommandFailure> {
	let parsed = parse_build_args(args)?;
	if parsed.version {
		let _ = writeln!(std::io::stdout(), "{VERSION}");
		return Ok(());
	}
	let project_path = if parsed.build_path.is_empty() {
		parsed.project.clone()
	} else {
		parsed.build_path.clone()
	};
	let ts_config_path = find_ts_config_path(&project_path).map_err(CommandFailure::Runtime)?;
	let rbxts = read_rbxts_options(&ts_config_path).map_err(|error| CommandFailure::Runtime(error.to_string()))?;
	let merged = merge_project_options(rbxts.as_ref(), parsed.verbose);
	logservice::set_verbose(merged.verbose);
	if merged.verbose {
		let builders = if parsed.build {
			rotor_compile::effective_solution_builders(&ProjectOptions::default()).to_string()
		} else {
			"n/a".to_owned()
		};
		logservice::write_line_if_verbose(&[&format!(
			"concurrency: builders={builders} checkers=default singleThreaded=unset"
		)]);
	}
	let dir = gopath::dir(&ts_config_path);
	let options = project_compile_options(&ts_config_path, &merged);

	if parsed.json {
		// stdout carries exactly one JSON object, so compiler warnings move to stderr.
		let previous = logservice::set_output_stderr(true);
		let (result, diagnostics, elapsed, error) = run_build(&dir, &ts_config_path, &options, parsed.build);
		logservice::set_output_stderr(previous);
		let json = build_json_result(&dir, result.as_ref(), &diagnostics, elapsed, error.as_deref());
		let _ = std::io::stdout().write_all(json.as_bytes());
		return if error.is_some() {
			Err(CommandFailure::Reported)
		} else {
			Ok(())
		};
	}

	let mut stdout = std::io::stdout();
	let out = Styler::for_writer(&stdout);
	banner(&mut stdout, out, &gopath::base(&dir));
	if std::fs::metadata(gopath::join(&[&dir, "package.json"])).is_ok()
		&& std::fs::metadata(gopath::join(&[&dir, "node_modules"])).is_err()
	{
		warn(
			&mut stdout,
			out,
			&format!(
				"{} has a package.json but no node_modules — type packages (e.g. @rbxts/*) cannot be resolved; install dependencies first",
				gopath::base(&dir)
			),
		);
	}
	let (result, diagnostics, elapsed, error) = run_build(&dir, &ts_config_path, &options, parsed.build);
	if let Some(error) = error {
		let mut stderr = std::io::stderr();
		let styler = Styler::for_writer(&stderr);
		build_failure(&mut stderr, styler, &error, &diagnostics, 50);
		return Err(CommandFailure::Reported);
	}
	let result = result.unwrap_or_default();
	if result.wrote_rotor_types {
		note_line(
			&mut stdout,
			out,
			"rotor.d.ts  (generated — editor types for sloptor macros)",
		);
	}
	let total = result.outputs.len();
	let written = result.emitted_files.len();
	build_success(&mut stdout, out, total, written, total.saturating_sub(written), elapsed);
	Ok(())
}

type BuildOutcome = (Option<BuildResult>, Vec<DiagnosticInfo>, Duration, Option<String>);

/// `runBuildOnceWithTimings` or `runBuildSolutionOnce`, with `buildDiagnostics`.
fn run_build(dir: &str, ts_config_path: &str, options: &ProjectOptions, solution: bool) -> BuildOutcome {
	let started = Instant::now();
	let built = rotor_tsrs::with_large_stack(|| {
		if solution {
			build_solution_with_options(ts_config_path, options)
		} else {
			build_project_with_options(dir, options)
		}
	});
	let elapsed = started.elapsed();
	match built {
		Ok(result) => (Some(result), Vec::new(), elapsed, None),
		Err(BuildFailure { result, failure }) => {
			let result = result.map(|result| *result);
			let diagnostics = match &result {
				Some(result) if !result.diagnostics.is_empty() => result.diagnostics.clone(),
				_ => failure
					.messages()
					.into_iter()
					.map(|message| DiagnosticInfo {
						message,
						..DiagnosticInfo::default()
					})
					.collect(),
			};
			(result, diagnostics, elapsed, Some(failure.error.to_string()))
		}
	}
}

/// A string as `json.Encoder` with `SetEscapeHTML(false)` writes it (`cmd/rotor/build.go`).
fn json_string(out: &mut String, text: &str) {
	rotor_compile::gojson::write_string_escaping(out, text, false);
}

/// `buildJSONResult` encoded as `writeJSONResult` writes it.
fn build_json_result(
	dir: &str,
	result: Option<&BuildResult>,
	diagnostics: &[DiagnosticInfo],
	elapsed: Duration,
	error: Option<&str>,
) -> String {
	use std::fmt::Write as _;
	let mut out = String::from("{\"version\":");
	json_string(&mut out, VERSION);
	let files = if error.is_none() {
		result.map_or(0, |result| result.outputs.len())
	} else {
		0
	};
	let _ = write!(
		out,
		",\"ok\":{},\"files\":{files},\"durationMs\":{},\"diagnostics\":[",
		error.is_none(),
		elapsed.as_millis()
	);
	let mut entries: Vec<String> = Vec::new();
	if let Some(error) = error {
		for diagnostic in diagnostics {
			let (file, line, col) = if diagnostic.file_name.is_empty() {
				(String::new(), 0, 0)
			} else {
				let (line, col) = line_col_of(&diagnostic.file_name, diagnostic.offset);
				(rel_display(dir, &diagnostic.file_name), line, col)
			};
			let mut entry = String::from("{\"file\":");
			json_string(&mut entry, &file);
			let _ = write!(entry, ",\"line\":{line},\"col\":{col}");
			if !diagnostic.code.is_empty() {
				entry.push_str(",\"code\":");
				json_string(&mut entry, &diagnostic.code);
			}
			entry.push_str(",\"severity\":");
			json_string(&mut entry, if diagnostic.warning { "warning" } else { "error" });
			entry.push_str(",\"message\":");
			json_string(&mut entry, &diagnostic.message);
			entry.push('}');
			entries.push(entry);
		}
		if diagnostics.is_empty() {
			let mut entry = String::from("{\"file\":\"\",\"line\":0,\"col\":0,\"severity\":\"error\",\"message\":");
			json_string(&mut entry, error);
			entry.push('}');
			entries.push(entry);
		}
	}
	out.push_str(&entries.join(","));
	out.push_str("]}\n");
	out
}

/// `lineColOf`: the 1-based line and byte column of `offset` in the file; `(0, 0)` when it
/// cannot be read or the offset is outside it.
fn line_col_of(file_name: &str, offset: usize) -> (usize, usize) {
	let Ok(data) = std::fs::read(file_name) else {
		return (0, 0);
	};
	if offset > data.len() {
		return (0, 0);
	}
	let (mut line, mut col) = (1, 1);
	for &byte in &data[..offset] {
		if byte == b'\n' {
			line += 1;
			col = 1;
		} else {
			col += 1;
		}
	}
	(line, col)
}

/// `relDisplay`: relative to `dir` when below it, slash-separated, `/` after a directory.
fn rel_display(dir: &str, target: &str) -> String {
	let mut rel = match gopath::rel(dir, target) {
		Ok(rel) if !rel.starts_with("..") => rel,
		_ => target.to_owned(),
	};
	rel = gopath::to_slash(&rel).into_owned();
	if std::fs::metadata(target).is_ok_and(|metadata| metadata.is_dir()) && !rel.ends_with('/') {
		rel.push('/');
	}
	rel
}

fn banner(out: &mut impl std::io::Write, styler: Styler, sub: &str) {
	let glyphs = styler.glyphs();
	let mut head = format!(
		"{} {}",
		styler.accent(&styler.bold("sloptor")),
		styler.muted(&format!("v{VERSION}"))
	);
	if !sub.is_empty() {
		head = format!("{head}  {}  {}", styler.muted(glyphs.dot), styler.info(sub));
	}
	let _ = write!(out, "\n  {head}\n\n");
}

fn warn(out: &mut impl std::io::Write, styler: Styler, message: &str) {
	let _ = writeln!(out, "  {}  {message}", styler.warn_bold(styler.glyphs().warn));
}

fn note_line(out: &mut impl std::io::Write, styler: Styler, message: &str) {
	let _ = writeln!(
		out,
		"    {} {}",
		styler.muted(styler.glyphs().arrow),
		styler.muted(message)
	);
}

fn plural(count: usize, noun: &str) -> String {
	if count == 1 {
		format!("1 {noun}")
	} else {
		format!("{count} {noun}s")
	}
}

fn build_success(
	out: &mut impl std::io::Write,
	styler: Styler,
	total: usize,
	written: usize,
	unchanged: usize,
	elapsed: Duration,
) {
	let glyphs = styler.glyphs();
	let ms = elapsed.as_millis();
	let headline = format!(
		"{}  {} {}",
		styler.success_bold(glyphs.check),
		styler.bold(&format!("compiled {}", plural(total, "file"))),
		styler.muted(&format!("in {ms} ms"))
	);
	let mut parts = vec![format!(
		"{}{}",
		styler.bold(&written.to_string()),
		styler.muted(" written")
	)];
	if unchanged > 0 {
		parts.push(styler.muted(&format!("{unchanged} unchanged")));
	}
	if ms > 0 {
		#[expect(clippy::cast_precision_loss, reason = "Go's float64 throughput")]
		#[expect(clippy::cast_possible_truncation, reason = "Go's int conversion")]
		let rate = (total as f64 / (ms as f64 / 1000.0)) as i64;
		parts.push(styler.muted(&format!("{rate} files/s")));
	}
	let _ = writeln!(out, "  {headline}");
	let _ = write!(out, "    {}\n\n", parts.join(&format!(" {} ", glyphs.dot)));
}

fn build_failure(
	out: &mut impl std::io::Write,
	styler: Styler,
	headline: &str,
	diagnostics: &[DiagnosticInfo],
	max_frames: usize,
) {
	let _ = writeln!(
		out,
		"  {}  {}",
		styler.error_bold(styler.glyphs().cross),
		styler.bold(headline)
	);
	let _ = writeln!(out);
	if diagnostics.is_empty() {
		return;
	}
	let _ = write!(out, "{}", render_diag_frames(styler.color(), diagnostics, max_frames));
	let _ = writeln!(out);
}

/// `renderDiagFrames`: located diagnostics framed per file, the rest as indented lines.
fn render_diag_frames(color: bool, diagnostics: &[DiagnosticInfo], max_frames: usize) -> String {
	let mut groups: Vec<Group> = Vec::new();
	let mut loose = Vec::new();
	for diagnostic in diagnostics {
		if diagnostic.file_name.is_empty() || diagnostic.len == 0 {
			loose.push(diagnostic.message.clone());
			continue;
		}
		let (primary, help) = split_help(&diagnostic.message);
		let spot = Spot {
			offset: isize::try_from(diagnostic.offset).unwrap_or(isize::MAX),
			len: isize::try_from(diagnostic.len).unwrap_or(isize::MAX),
			severity: if diagnostic.warning {
				diagframe::Severity::Warning
			} else {
				diagframe::Severity::Error
			},
			code: diagnostic.code.clone(),
			message: primary,
			help,
		};
		let path = rel_for_display(&diagnostic.file_name);
		if let Some(group) = groups.iter_mut().find(|group| group.path == path) {
			group.spots.push(spot);
		} else {
			groups.push(Group {
				path,
				source: std::fs::read(&diagnostic.file_name)
					.map(|data| String::from_utf8_lossy(&data).into_owned())
					.unwrap_or_default(),
				lang: Language::TypeScript,
				spots: vec![spot],
			});
		}
	}
	let mut out = diagframe::render_groups(
		&groups,
		diagframe::Options {
			context: 0,
			color,
			link: color,
		},
		max_frames,
	);
	for line in loose {
		out.push_str("    ");
		out.push_str(&line);
		out.push('\n');
	}
	out
}

/// `splitHelp`: the primary lines and the trailing `Suggestion:` / `More information:` lines.
fn split_help(message: &str) -> (String, Vec<String>) {
	let mut parts = message.split('\n');
	let mut primary = parts.next().unwrap_or_default().to_owned();
	let mut help = Vec::new();
	for part in parts {
		if part.starts_with("Suggestion: ") || part.starts_with("More information: ") {
			help.push(part.to_owned());
		} else {
			primary.push('\n');
			primary.push_str(part);
		}
	}
	(primary, help)
}

/// `relForDisplay`: relative to the working directory when possible.
fn rel_for_display(path: &str) -> String {
	std::env::current_dir()
		.ok()
		.and_then(|cwd| gopath::rel(&cwd.to_string_lossy(), path).ok())
		.map_or_else(|| path.to_owned(), |rel| gopath::to_slash(&rel).into_owned())
}

#[cfg(test)]
mod tests {
	use super::json_string;

	#[test]
	fn json_string_matches_go_encoder_without_html_escaping() {
		let mut out = String::new();
		json_string(&mut out, "a\u{8}b\u{c}c\u{1}<>&\"\\\n\r\t\u{2028}\u{2029}é");
		assert_eq!(out, r#""a\bb\fc\u0001<>&\"\\\n\r\t\u2028\u2029é""#);
	}
}
