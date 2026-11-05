//! `sloptor build` (`cmd/rotor/build.go`): the rbxtsc build flag surface, its implication checks,
//! and the styled or `--json` one-shot build.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rotor_compile::compile::DiagnosticInfo;
use rotor_compile::output::{BuildFailure, BuildResult};
use rotor_compile::progress::BuildProgress;
use rotor_compile::timings::BuildTimings;
use rotor_compile::{ProjectOptions, build_project_with_options, build_solution_with_options};
use rotor_support::{gopath, logservice};

use crate::app::{Failure, usage};
use crate::cli::BuildCommand;
use crate::json::{JsonDiagnostic, JsonResult};
use crate::options::{
	MergedOptions, PartialOptions, find_ts_config_path, merge_project_options, project_compile_options,
	read_rbxts_layer,
};
use crate::term::Streams;
use crate::{VERSION, live, report, ui};

/// `buildArgs`: the project path, the argv layer (exactly the flags passed), and the rest of the
/// build surface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "Go's buildArgs fields")]
pub struct BuildArgs {
	pub project: String,
	pub opts: PartialOptions,
	pub build: bool,
	pub build_path: String,
	pub emit_declaration_only: bool,
	pub builders: Option<i64>,
	pub checkers: Option<i64>,
	pub single_threaded: Option<bool>,
	pub json: bool,
	pub version: bool,
	/// Whether `--use-polling` was passed (Go's implication reads presence, not the value).
	pub use_polling_passed: bool,
	/// The finite diagnostic outputs, by flag, in Go's order.
	pub outputs: Vec<(&'static str, String)>,
	pub timings: String,
	/// 0 renders every frame.
	pub max_errors: usize,
	pub bell: bool,
	pub clear_screen: bool,
}

/// The finite diagnostic output flags, in Go's order.
const OUTPUT_FLAGS: [&str; 6] = [
	"--cpuprofile",
	"--trace-out",
	"--blockprofile",
	"--mutexprofile",
	"--heapprofile",
	"--timings",
];

/// `collectBuildArgs`: the parsed flags as Go's `buildArgs`.
///
/// # Errors
///
/// A usage error when both the positional path and `--project` are given.
pub fn collect(command: BuildCommand) -> Result<BuildArgs, Failure> {
	if let (Some(path), Some(_)) = (&command.path, &command.project) {
		return Err(usage(
			"build",
			format!("unexpected extra argument '{path}' (the project is already set by --project)"),
		));
	}
	let project = command.path.or(command.project).unwrap_or_else(|| ".".to_owned());
	let outputs: Vec<(&'static str, String)> = OUTPUT_FLAGS
		.into_iter()
		.zip([
			command.cpuprofile,
			command.trace_out,
			command.blockprofile,
			command.mutexprofile,
			command.heapprofile,
			command.timings.clone(),
		])
		.filter_map(|(flag, path)| path.map(|path| (flag, path)))
		.collect();
	Ok(BuildArgs {
		project,
		opts: PartialOptions {
			include_path: command.include_path,
			rojo: command.rojo,
			type_name: command.project_type.map(|kind| kind.name().to_owned()),
			watch: command.watch,
			use_polling: command.use_polling,
			verbose: command.verbose,
			no_include: command.no_include,
			log_truthy_changes: command.log_truthy_changes,
			write_only_changed: command.write_only_changed,
			write_transformed_files: command.write_transformed_files,
			optimized_array_appends: command.optimized_array_appends,
			optimized_loops: command.optimized_loops,
			allow_comment_directives: command.allow_comment_directives,
			luau: command.luau,
			luau_types: command.luau_types,
		},
		build: command.build.is_some(),
		build_path: command.build.unwrap_or_default(),
		emit_declaration_only: command.emit_declaration_only.unwrap_or(false),
		builders: command.builders,
		checkers: command.checkers,
		single_threaded: command.single_threaded,
		json: command.json.unwrap_or(false),
		version: command.version.unwrap_or(false),
		use_polling_passed: command.use_polling.is_some(),
		outputs,
		timings: command.timings.unwrap_or_default(),
		max_errors: command.max_errors.unwrap_or(50),
		bell: command.bell.unwrap_or(false),
		clear_screen: command.clear.unwrap_or(true),
	})
}

/// `runBuildCommand` before the build: the option implications, in Go's order.
///
/// # Errors
///
/// The usage error of the first implication that fails.
pub fn check_implications(args: &BuildArgs) -> Result<(), Failure> {
	if let Some((flag, _)) = args.outputs.iter().find(|(_, path)| path.is_empty()) {
		return Err(usage("build", format!("{flag} requires a path")));
	}
	if args.builders.is_some() && !args.build {
		return Err(usage("build", "--builders requires --build"));
	}
	if args.use_polling_passed && args.opts.watch.is_none() {
		return Err(usage("build", "--use-polling requires --watch"));
	}
	if args.emit_declaration_only && !args.build {
		return Err(usage("build", "--emit-declaration-only requires --build"));
	}
	let watch = args.opts.watch == Some(true);
	if args.build && args.emit_declaration_only && watch {
		return Err(usage(
			"build",
			"--build --watch is incompatible with --emit-declaration-only (no Luau emit to incrementally watch)",
		));
	}
	if watch && let Some((flag, _)) = args.outputs.first() {
		return Err(usage("build", format!("{flag} cannot be used with --watch")));
	}
	Ok(())
}

/// `sloptor build`
///
/// # Errors
///
/// A usage error, a setup failure, or a build failure already reported.
pub fn run(command: BuildCommand, streams: &mut Streams) -> Result<(), Failure> {
	let args = collect(command)?;
	if args.version {
		let _ = writeln!(streams.out, "{VERSION}");
		return Ok(());
	}
	check_implications(&args)?;
	validate_diagnostic_paths(&args.outputs).map_err(Failure::Runtime)?;
	if let Some((flag, _)) = args.outputs.iter().find(|(flag, _)| *flag != "--timings") {
		return Err(Failure::Runtime(format!(
			"{flag} writes a Go runtime profile, which the Rust build of sloptor cannot produce; profile it \
			 with a sampling profiler instead (for example `samply record sloptor build`)"
		)));
	}
	run_body(&args, streams)
}

/// `runBuildBody`: config resolution, the merge, and the styled or `--json` build.
fn run_body(args: &BuildArgs, streams: &mut Streams) -> Result<(), Failure> {
	let project_path = if args.build_path.is_empty() {
		&args.project
	} else {
		&args.build_path
	};
	let ts_config_path = find_ts_config_path(project_path).map_err(Failure::Runtime)?;
	let rbxts = read_rbxts_layer(&ts_config_path).map_err(Failure::Runtime)?;
	let mut opts = merge_project_options(MergedOptions::default(), &[rbxts.as_ref(), Some(&args.opts)]);
	opts.argv = Some(args.opts.clone());
	opts.emit_declaration_only = args.emit_declaration_only;
	opts.builders = args.builders;
	opts.checkers = args.checkers;
	opts.single_threaded = args.single_threaded;
	opts.one_shot_build = !opts.watch && !opts.emit_declaration_only;
	if opts.watch {
		return run_watch(args, streams, &ts_config_path, opts);
	}
	if !args.timings.is_empty() {
		prepare_timings_path(&args.timings)
			.map_err(|error| Failure::Runtime(format!("cannot prepare timings output: {error}")))?;
	}

	logservice::set_verbose(opts.verbose);
	let dir = gopath::dir(&ts_config_path);
	let mut options = project_compile_options(&ts_config_path, &opts);
	let timings = (!args.timings.is_empty()).then(|| {
		let timings = Arc::new(BuildTimings::new());
		timings.set_product_version(VERSION);
		timings
	});
	options.timings.clone_from(&timings);

	if args.json {
		// Go logs the concurrency and slot lines before moving the channel to stderr.
		if opts.verbose {
			log_concurrency(args, &opts);
		}
		let _lease = admit(streams);
		// stdout carries exactly one JSON object, so compiler warnings move to stderr.
		let previous = logservice::set_output_stderr(true);
		let outcome = run_build(&dir, &ts_config_path, &options, args.build);
		let written = finish_timings(timings.as_deref(), &args.timings, outcome.error.is_none());
		logservice::set_output_stderr(previous);
		if let Err(error) = written {
			let _ = writeln!(streams.err, "write timings: {error}");
			return Err(Failure::Reported);
		}
		let _ = streams.out.write_all(json_result(&dir, &outcome).encode().as_bytes());
		return if outcome.error.is_some() {
			Err(Failure::Reported)
		} else {
			Ok(())
		};
	}

	let out = streams.out_paint();
	let header_target = if args.build {
		ui::rel_for_display(&ts_config_path)
	} else {
		ui::project_name(&ts_config_path)
	};
	let builders = args.build.then(|| {
		let mut entry = ProjectOptions::default();
		entry.builders = opts.builders;
		entry.single_threaded = opts.single_threaded;
		ui::plural(
			usize::try_from(rotor_compile::effective_solution_builders(&entry)).unwrap_or(1),
			"builder",
		)
	});
	ui::header(
		streams.out,
		out,
		&[
			if args.build { "build -b" } else { "build" },
			&header_target,
			builders.as_deref().unwrap_or_default(),
		],
	);
	if opts.verbose {
		log_concurrency(args, &opts);
	}
	let _lease = admit(streams);
	if opts.write_transformed_files {
		ui::warn(
			streams.out,
			out,
			"--write-transformed-files is not supported by sloptor (rbxtsc transformer-plugin debug output); ignoring it",
		);
	}
	if std::fs::metadata(gopath::join(&[&dir, "package.json"])).is_ok()
		&& std::fs::metadata(gopath::join(&[&dir, "node_modules"])).is_err()
	{
		ui::warn(
			streams.out,
			out,
			&format!(
				"{} has a package.json but no node_modules, so type packages (@rbxts/*) cannot resolve; install \
				 dependencies first",
				gopath::base(&dir)
			),
		);
	}

	let progress = Arc::new(BuildProgress::new(opts.verbose));
	options.progress = Some(Arc::clone(&progress));
	// --verbose reports the phases, so it keeps a timing report even without --timings.
	let report_timings = timings
		.clone()
		.or_else(|| opts.verbose.then(|| Arc::new(BuildTimings::new())));
	options.timings.clone_from(&report_timings);
	let view = live::View {
		progress_line: !opts.verbose && live::progress_line_allowed(streams),
		project_lines: args.build,
		solution: args.build,
		out,
	};
	let outcome = live::run(streams, &progress, view, || {
		run_build(&dir, &ts_config_path, &options, args.build)
	});
	finish_timings(timings.as_deref(), &args.timings, outcome.error.is_none())
		.map_err(|error| Failure::Runtime(format!("write timings: {error}")))?;
	let shown = report::Shown {
		config_path: &ts_config_path,
		solution: args.build,
		max_errors: args.max_errors,
	};
	if opts.verbose {
		// The verbose log lines end here; the result block starts after a gap.
		let _ = writeln!(streams.out);
	}
	let failed = report::build(streams, &shown, &outcome, &progress);
	if opts.verbose {
		report::verbose(streams.out, out, report_timings.as_deref(), &progress, args.build);
	}
	if failed { Err(Failure::Reported) } else { Ok(()) }
}

/// The watch half of `runBuildBody`. A watch session takes no build slot, as in Go (it applies
/// only the runtime policy, which has no Rust equivalent), and runs until Ctrl+C.
fn run_watch(
	args: &BuildArgs,
	streams: &mut Streams,
	ts_config_path: &str,
	opts: MergedOptions,
) -> Result<(), Failure> {
	rotor_tsrs::project::set_long_lived(true);
	logservice::set_verbose(opts.verbose);
	if opts.verbose {
		log_concurrency(args, &opts);
	}
	let dir = gopath::dir(ts_config_path);
	let options = project_compile_options(ts_config_path, &opts);
	let include_dir = crate::options::include_dir(&dir, &opts);
	let stop = crate::watch::Stop::default();
	let watcher: Arc<dyn crate::fswatch::Watcher> = Arc::new(crate::fswatch::NotifyWatcher::new(opts.use_polling));
	let reload = reload_options(ts_config_path, args);
	let unsupported =
		"--write-transformed-files is not supported by sloptor (rbxtsc transformer-plugin debug output); ignoring it";

	if args.json {
		crate::watch::exit_on_interrupt(false);
		// The compiler's warnings must not corrupt the event stream.
		logservice::set_output_stderr(true);
		if opts.write_transformed_files {
			let err = streams.err_paint();
			ui::warn(streams.err, err, unsupported);
		}
		let mut reporter =
			crate::watch_json::JsonBuildReporter(crate::watch_json::WatchEventWriter::new(streams.out, &dir));
		if args.build {
			let code = crate::solution_watch::run_build_solution_watch_loop(
				&stop,
				ts_config_path,
				opts,
				&reload,
				&mut reporter,
				watcher,
			);
			return if code == 0 { Ok(()) } else { Err(Failure::Reported) };
		}
		rotor_tsrs::with_large_stack(|| {
			crate::watch::run_build_watch_loop(&stop, &dir, ts_config_path, &options, &include_dir, &mut reporter);
		});
		return Ok(());
	}

	let out = streams.out_paint();
	let header_target = if args.build {
		ui::rel_for_display(ts_config_path)
	} else {
		ui::project_name(ts_config_path)
	};
	ui::header(
		streams.out,
		out,
		&[
			if args.build {
				"build -b --watch"
			} else {
				"build --watch"
			},
			&header_target,
		],
	);
	if opts.write_transformed_files {
		ui::warn(streams.out, out, unsupported);
	}
	crate::watch::exit_on_interrupt(streams.err_color && crate::term::process_terminals().1);
	let watch_options = crate::watch::WatchOptions {
		max_errors: args.max_errors,
		bell: args.bell,
		clear_screen: args.clear_screen,
	};
	let mut reporter = crate::watch::StyledBuildReporter::new(streams, ts_config_path, watch_options);
	if args.build {
		let code = crate::solution_watch::run_build_solution_watch_loop(
			&stop,
			ts_config_path,
			opts,
			&reload,
			&mut reporter,
			watcher,
		);
		return if code == 0 { Ok(()) } else { Err(Failure::Reported) };
	}
	rotor_tsrs::with_large_stack(|| {
		crate::watch::run_build_watch_loop(&stop, &dir, ts_config_path, &options, &include_dir, &mut reporter);
	});
	Ok(())
}

/// `newBuildOptionsReload`: the options a watch rebuild reloads after a config change: the
/// defaults, the tsconfig's `rbxts` key, then the command line.
pub fn reload_options(
	ts_config_path: &str,
	args: &BuildArgs,
) -> impl Fn() -> Result<MergedOptions, String> + Sync + use<> {
	let ts_config_path = ts_config_path.to_owned();
	let layer = args.opts.clone();
	let (emit_declaration_only, builders, checkers) = (args.emit_declaration_only, args.builders, args.checkers);
	move || {
		let declared = read_rbxts_layer(&ts_config_path)?;
		let mut next = merge_project_options(MergedOptions::default(), &[declared.as_ref(), Some(&layer)]);
		next.argv = Some(layer.clone());
		next.emit_declaration_only = emit_declaration_only;
		next.builders = builders;
		next.checkers = checkers;
		Ok(next)
	}
}

/// Takes a build slot and sizes the worker pool (`admitBuild`); the wait notice goes to stderr.
pub fn admit(streams: &mut Streams) -> Option<crate::runtime::Lease> {
	let err = &mut *streams.err;
	crate::runtime::admit(&mut |message| {
		let _ = writeln!(err, "{message}");
	})
}

/// The verbose `concurrency:` line.
fn log_concurrency(args: &BuildArgs, opts: &MergedOptions) {
	let builders = if args.build {
		let mut entry = ProjectOptions::default();
		entry.builders = opts.builders;
		entry.single_threaded = opts.single_threaded;
		rotor_compile::effective_solution_builders(&entry).to_string()
	} else {
		"n/a".to_owned()
	};
	let checkers = opts
		.checkers
		.map_or_else(|| "default".to_owned(), |checkers| checkers.to_string());
	let single = opts
		.single_threaded
		.map_or_else(|| "unset".to_owned(), |single| single.to_string());
	logservice::write_line_if_verbose(&[&format!(
		"concurrency: builders={builders} checkers={checkers} singleThreaded={single}"
	)]);
}

/// What one build produced.
pub struct Outcome {
	/// The build's result, also on most failures.
	pub result: Option<BuildResult>,
	/// The structured diagnostics, or the failure's messages.
	pub diagnostics: Vec<DiagnosticInfo>,
	/// Wall time.
	pub elapsed: Duration,
	/// The failure's text.
	pub error: Option<String>,
}

/// `runBuildOnceWithTimings` or `runBuildSolutionOnce`, with `buildDiagnostics`.
#[must_use]
pub fn run_build(dir: &str, ts_config_path: &str, options: &ProjectOptions, solution: bool) -> Outcome {
	rotor_tsrs::with_large_stack(|| run_build_here(dir, ts_config_path, options, solution))
}

/// [`run_build`] on the calling thread, which must have a large stack. A watch session builds on
/// one thread for good, so the allocators never see a thread per rebuild.
#[must_use]
pub fn run_build_here(dir: &str, ts_config_path: &str, options: &ProjectOptions, solution: bool) -> Outcome {
	let started = Instant::now();
	let built = if solution {
		build_solution_with_options(ts_config_path, options)
	} else {
		build_project_with_options(dir, options)
	};
	let elapsed = started.elapsed();
	match built {
		Ok(result) => Outcome {
			result: Some(result),
			diagnostics: Vec::new(),
			elapsed,
			error: None,
		},
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
			Outcome {
				result,
				diagnostics,
				elapsed,
				error: Some(failure.error.to_string()),
			}
		}
	}
}

/// `buildJSONResult`: a build pass in the `--json` wire shape. A failure without structured
/// diagnostics reports its own text as the one error.
#[must_use]
pub fn json_result(dir: &str, outcome: &Outcome) -> JsonResult {
	let mut result = JsonResult {
		version: VERSION.to_owned(),
		ok: outcome.error.is_none(),
		files: 0,
		duration_ms: outcome.elapsed.as_millis(),
		diagnostics: Vec::new(),
	};
	let Some(error) = &outcome.error else {
		result.files = outcome.result.as_ref().map_or(0, |result| result.outputs.len());
		return result;
	};
	for diagnostic in &outcome.diagnostics {
		let mut entry = JsonDiagnostic {
			code: diagnostic.code.clone(),
			severity: if diagnostic.warning { "warning" } else { "error" },
			message: diagnostic.message.clone(),
			..JsonDiagnostic::default()
		};
		if !diagnostic.file_name.is_empty() {
			entry.file = rel_display(dir, &diagnostic.file_name);
			(entry.line, entry.col) = line_col_of(&diagnostic.file_name, diagnostic.offset);
		}
		result.diagnostics.push(entry);
	}
	if outcome.diagnostics.is_empty() {
		result.diagnostics.push(JsonDiagnostic {
			severity: "error",
			message: error.clone(),
			..JsonDiagnostic::default()
		});
	}
	result
}

/// `lineColOf`: the 1-based line and byte column of `offset` in the file; `(0, 0)` when it cannot
/// be read or the offset is outside it.
#[must_use]
pub fn line_col_of(file_name: &str, offset: usize) -> (usize, usize) {
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
#[must_use]
pub fn rel_display(dir: &str, target: &str) -> String {
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

/// `prepareBuildTimingsPath`: creates the directory and proves a file can be created in it, so a
/// build never runs only to fail writing its report.
fn prepare_timings_path(path: &str) -> Result<(), String> {
	let dir = gopath::dir(path);
	std::fs::create_dir_all(&dir).map_err(|error| format!("create timings directory \"{dir}\": {error}"))?;
	let probe = tempfile::Builder::new()
		.prefix(".rotor-timings-")
		.tempfile_in(&dir)
		.map_err(|error| format!("create timings destination in \"{dir}\": {error}"))?;
	probe
		.close()
		.map_err(|error| format!("remove timings destination in \"{dir}\": {error}"))
}

/// `SetOK` and `writeBuildTimings`: the report and its newline, replacing `path` atomically.
fn finish_timings(timings: Option<&BuildTimings>, path: &str, ok: bool) -> Result<(), String> {
	let Some(timings) = timings else {
		return Ok(());
	};
	timings.set_ok(ok);
	let mut data = timings.report().to_json();
	data.push('\n');
	rotor_compile::envdecl::atomic_write_file(path, data.as_bytes()).map_err(|error| error.to_string())
}

/// A diagnostic output path, resolved through symlinks (even dangling ones), with its metadata
/// when it exists.
fn resolve_output_path(path: &str) -> Result<(String, Option<std::fs::Metadata>), String> {
	let mut path = gopath::clean(&gopath::abs(path).map_err(|error| error.to_string())?);
	for _ in 0..255 {
		match std::fs::metadata(&path) {
			Ok(metadata) => {
				let resolved = rotor_support::fsutil::eval_symlinks(&path).map_err(|error| error.to_string())?;
				return Ok((resolved, Some(metadata)));
			}
			Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
			Err(_) => {}
		}
		match std::fs::symlink_metadata(&path) {
			Ok(link) if link.file_type().is_symlink() => {
				let target = std::fs::read_link(&path).map_err(|error| error.to_string())?;
				let target = target.to_string_lossy();
				path = if gopath::is_abs(&target) {
					gopath::clean(&target)
				} else {
					gopath::clean(&gopath::join(&[&gopath::dir(&path), &target]))
				};
				continue;
			}
			Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
			_ => {}
		}
		if let Ok(parent) = rotor_support::fsutil::eval_symlinks(&gopath::dir(&path)) {
			path = gopath::join(&[&parent, &gopath::base(&path)]);
		}
		return Ok((path, None));
	}
	Err("too many symbolic links in diagnostic output path".to_owned())
}

/// `os.SameFile`
#[cfg(unix)]
fn same_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
	use std::os::unix::fs::MetadataExt as _;
	left.dev() == right.dev() && left.ino() == right.ino()
}

/// `os.SameFile`: Windows aliases are caught by their resolved paths only.
#[cfg(not(unix))]
const fn same_file(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
	false
}

/// `validateBuildDiagnosticPaths`: no two finite diagnostic outputs may name the same file, by
/// path, symlink or hard link.
///
/// # Errors
///
/// The first collision, or a path that cannot be resolved.
pub fn validate_diagnostic_paths(outputs: &[(&'static str, String)]) -> Result<(), String> {
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let mut seen: Vec<(&str, String, Option<std::fs::Metadata>)> = Vec::new();
	for (flag, path) in outputs {
		let (mut resolved, metadata) =
			resolve_output_path(path).map_err(|error| format!("resolve {flag} output path: {error}"))?;
		if !case_sensitive {
			resolved = resolved.to_lowercase();
		}
		for (previous, previous_path, previous_metadata) in &seen {
			let aliased = matches!((previous_metadata, &metadata), (Some(left), Some(right)) if same_file(left, right));
			if *previous_path == resolved || aliased {
				return Err(format!("{previous} and {flag} cannot write to the same path"));
			}
		}
		seen.push((flag, resolved, metadata));
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	//! The flag-semantics half of `build_test.go`, parsed through the real command tree.

	use std::ffi::OsString;

	use super::{BuildArgs, check_implications, collect, validate_diagnostic_paths};
	use crate::app::Failure;
	use crate::cli::{self, Command};
	use crate::options::{MergedOptions, PartialOptions, project_compile_options};
	use crate::{argv, options};

	fn parse(args: &[&str]) -> Result<BuildArgs, String> {
		let mut tokens: Vec<OsString> = vec!["build".into()];
		tokens.extend(args.iter().map(OsString::from));
		let tokens = argv::normalize_negations(&cli::command(), tokens);
		let parsed = cli::parse(tokens).map_err(|error| error.to_string())?;
		let Some(Command::Build(command)) = parsed.command else {
			return Err("not a build".to_owned());
		};
		let args = collect(*command).map_err(|failure| format!("{failure:?}"))?;
		check_implications(&args).map_err(|failure| match failure {
			Failure::Usage(error) => error.to_string(),
			other => format!("{other:?}"),
		})?;
		Ok(args)
	}

	fn parsed(args: &[&str]) -> BuildArgs {
		parse(args).unwrap_or_else(|error| panic!("parse {args:?}: {error}"))
	}

	fn rejected(args: &[&str], want: &str) {
		match parse(args) {
			Ok(parsed) => panic!("{args:?} parsed: {parsed:?}"),
			Err(error) => assert!(error.contains(want), "{args:?}: {error:?} lacks {want:?}"),
		}
	}

	#[test]
	fn no_args_defaults_the_project_and_leaves_the_argv_layer_empty() {
		let got = parsed(&[]);
		assert_eq!(got.project, ".");
		assert_eq!(got.opts, PartialOptions::default());
		assert_eq!(got.max_errors, 50);
		assert!(!got.bell);
		assert!(got.clear_screen);
		assert_eq!(got.builders, None);
		assert_eq!(got.checkers, None);
		assert_eq!(got.single_threaded, None);
	}

	#[test]
	fn boolean_value_forms() {
		for args in [
			&["--no-luau"][..],
			&["--luau=false"],
			&["--luau=0"],
			&["--luau", "--no-luau"],
		] {
			assert_eq!(parsed(args).opts.luau, Some(false), "{args:?}");
		}
		for args in [
			&["--luau"][..],
			&["--luau=true"],
			&["--luau=T"],
			&["--no-luau=false"],
			&["--no-luau", "--luau"],
		] {
			assert_eq!(parsed(args).opts.luau, Some(true), "{args:?}");
		}
		assert_eq!(parsed(&["--optimized-loops=false"]).opts.optimized_loops, Some(false));
		assert_eq!(
			parsed(&["--optimized-array-appends=false"])
				.opts
				.optimized_array_appends,
			Some(false)
		);
		assert_eq!(parsed(&["--luau-types=false"]).opts.luau_types, Some(false));
		// `--flag value` never takes the value: it is the positional path.
		let got = parsed(&["--luau", "false"]);
		assert_eq!((got.opts.luau, got.project.as_str()), (Some(true), "false"));
		rejected(&["--luau=maybe"], "invalid value 'maybe'");
	}

	#[test]
	fn kebab_names_and_go_camel_aliases_are_the_same_flag() {
		let pairs: [(&str, &str); 11] = [
			("--no-include", "--noInclude"),
			("--log-truthy-changes", "--logTruthyChanges"),
			("--write-only-changed", "--writeOnlyChanged"),
			("--write-transformed-files", "--writeTransformedFiles"),
			("--optimized-array-appends", "--optimizedArrayAppends"),
			("--optimized-loops", "--optimizedLoops"),
			("--allow-comment-directives", "--allowCommentDirectives"),
			("--luau-types", "--luauTypes"),
			("--single-threaded", "--singleThreaded"),
			("--emit-declaration-only", "--emitDeclarationOnly"),
			("--use-polling", "--usePolling"),
		];
		for (kebab, camel) in pairs {
			let extra: &[&str] = match kebab {
				"--emit-declaration-only" => &["--build"],
				"--use-polling" => &["--watch"],
				_ => &[],
			};
			for value in ["", "=false", "=true"] {
				let kebab_args: Vec<String> = extra
					.iter()
					.map(|arg| (*arg).to_owned())
					.chain([format!("{kebab}{value}")])
					.collect();
				let camel_args: Vec<String> = extra
					.iter()
					.map(|arg| (*arg).to_owned())
					.chain([format!("{camel}{value}")])
					.collect();
				let kebab_args: Vec<&str> = kebab_args.iter().map(String::as_str).collect();
				let camel_args: Vec<&str> = camel_args.iter().map(String::as_str).collect();
				assert_eq!(
					parsed(&kebab_args),
					parsed(&camel_args),
					"{kebab}{value} vs {camel}{value}"
				);
			}
			let negated = format!("--no-{}", camel.trim_start_matches("--"));
			let mut args: Vec<&str> = extra.to_vec();
			args.push(&negated);
			assert_eq!(
				parsed(&args),
				parsed(&[extra, &[&format!("{kebab}=false")]].concat()),
				"{negated}"
			);
		}
		assert_eq!(parsed(&["--includePath", "inc"]), parsed(&["--include-path", "inc"]));
		assert_eq!(parsed(&["-i", "inc"]).opts.include_path.as_deref(), Some("inc"));
	}

	#[test]
	fn plain_boolean_flags_set_true() {
		let got = parsed(&[
			"--verbose",
			"--no-include",
			"--log-truthy-changes",
			"--write-only-changed",
			"--write-transformed-files",
			"--allow-comment-directives",
			"--optimized-array-appends",
			"--luau-types",
		]);
		let opts = got.opts;
		for (name, value) in [
			("verbose", opts.verbose),
			("noInclude", opts.no_include),
			("logTruthyChanges", opts.log_truthy_changes),
			("writeOnlyChanged", opts.write_only_changed),
			("writeTransformedFiles", opts.write_transformed_files),
			("allowCommentDirectives", opts.allow_comment_directives),
			("optimizedArrayAppends", opts.optimized_array_appends),
			("luauTypes", opts.luau_types),
		] {
			assert_eq!(value, Some(true), "--{name}");
		}
	}

	#[test]
	fn type_and_string_flags() {
		assert_eq!(parsed(&["--type", "model"]).opts.type_name.as_deref(), Some("model"));
		rejected(&["--type", "bogus"], "invalid value 'bogus'");
		rejected(&["--type"], "a value is required");
		let got = parsed(&["-p", "proj", "-i", "inc", "--rojo=custom.project.json"]);
		assert_eq!(got.project, "proj");
		assert_eq!(got.opts.include_path.as_deref(), Some("inc"));
		assert_eq!(got.opts.rojo.as_deref(), Some("custom.project.json"));
		// A valueless --rojo is present and empty: Rojo config auto-discovery.
		let got = parsed(&["--rojo", "--verbose"]);
		assert_eq!(got.opts.rojo.as_deref(), Some(""));
		assert_eq!(got.opts.verbose, Some(true));
		assert_eq!(parsed(&["some/dir", "--verbose"]).project, "some/dir");
		rejected(&["a", "-p", "b"], "already set by --project");
		rejected(&["--bogus"], "unexpected argument '--bogus'");
		rejected(&["--minify"], "unexpected argument '--minify'");
	}

	#[test]
	fn repeated_flags_take_the_last_value() {
		let got = parsed(&["--checkers", "1", "--checkers=3", "--project", "a", "--project", "b"]);
		assert_eq!((got.checkers, got.project.as_str()), (Some(3), "b"));
	}

	#[test]
	fn implications_fail_as_usage_errors() {
		rejected(&["--use-polling"], "--use-polling requires --watch");
		assert_eq!(parsed(&["--use-polling", "-w"]).opts.use_polling, Some(true));
		rejected(&["--emit-declaration-only"], "--emit-declaration-only requires --build");
		assert!(parsed(&["--emit-declaration-only=false"]).opts.watch.is_none());
		rejected(
			&["--build", "--watch", "--emit-declaration-only"],
			"--build --watch is incompatible with --emit-declaration-only",
		);
		rejected(&["--builders", "2"], "--builders requires --build");
		for flag in [
			"--cpuprofile",
			"--trace-out",
			"--blockprofile",
			"--mutexprofile",
			"--heapprofile",
			"--timings",
		] {
			rejected(
				&["--watch", flag, "profile.out"],
				&format!("{flag} cannot be used with --watch"),
			);
			rejected(&[&format!("{flag}=")], &format!("{flag} requires a path"));
		}
	}

	#[test]
	fn build_mode_args() {
		let got = parsed(&["-b", "tsconfig.solution.json"]);
		assert_eq!((got.build, got.build_path.as_str()), (true, "tsconfig.solution.json"));
		let got = parsed(&["--build"]);
		assert_eq!((got.build, got.build_path.as_str()), (true, ""));
		let got = parsed(&["--build", "--emit-declaration-only"]);
		assert!(got.build && got.emit_declaration_only);
		assert!(parsed(&["--json", "."]).json);
	}

	#[test]
	fn single_threaded_is_tri_state() {
		assert_eq!(parsed(&[]).single_threaded, None);
		assert_eq!(parsed(&["--single-threaded"]).single_threaded, Some(true));
		assert_eq!(parsed(&["--single-threaded=true"]).single_threaded, Some(true));
		assert_eq!(parsed(&["--single-threaded=false"]).single_threaded, Some(false));
		assert_eq!(parsed(&["--no-singleThreaded"]).single_threaded, Some(false));
	}

	#[test]
	fn concurrency_controls() {
		assert_eq!(parsed(&["--build", "--builders", "1"]).builders, Some(1));
		assert_eq!(parsed(&["--build", "--builders=4"]).builders, Some(4));
		assert_eq!(parsed(&["--checkers", "1"]).checkers, Some(1));
		assert_eq!(parsed(&["--checkers=4"]).checkers, Some(4));
		let got = parsed(&["--build", "--builders", "4", "--checkers", "2"]);
		assert_eq!((got.builders, got.checkers), (Some(4), Some(2)));
		assert_eq!(parsed(&["-b", "--builders=1"]).builders, Some(1));
		rejected(&["--build", "--builders"], "a value is required for '--builders <N>'");
		rejected(&["--checkers"], "a value is required for '--checkers <N>'");
		for value in ["0", "-1", "many"] {
			rejected(
				&["--build", &format!("--builders={value}")],
				"must be a positive integer",
			);
			rejected(&[&format!("--checkers={value}")], "must be a positive integer");
		}
	}

	#[test]
	fn max_errors_and_watch_display_flags() {
		assert_eq!(parsed(&["--max-errors", "10"]).max_errors, 10);
		assert_eq!(parsed(&["--max-errors=5"]).max_errors, 5);
		assert_eq!(parsed(&["--max-errors", "0"]).max_errors, 0);
		// A valueless --max-errors means 0, unlimited.
		assert_eq!(parsed(&["--max-errors", "--verbose"]).max_errors, 0);
		rejected(&["--max-errors=-1"], "must be a non-negative integer");
		assert!(parsed(&["--bell"]).bell);
		assert!(!parsed(&["--no-clear"]).clear_screen);
		assert!(!parsed(&["--clear=false"]).clear_screen);
	}

	#[test]
	fn timings_and_profile_paths_are_collected_in_go_order() {
		for args in [&["--timings", "report.json"][..], &["--timings=report.json"]] {
			assert_eq!(parsed(args).timings, "report.json");
		}
		let got = parsed(&[
			"--heapprofile",
			"heap.prof",
			"--trace-out",
			"trace.out",
			"--blockprofile",
			"block.prof",
			"--mutexprofile",
			"mutex.prof",
		]);
		assert_eq!(
			got.outputs,
			[
				("--trace-out", "trace.out".to_owned()),
				("--blockprofile", "block.prof".to_owned()),
				("--mutexprofile", "mutex.prof".to_owned()),
				("--heapprofile", "heap.prof".to_owned()),
			]
		);
	}

	#[test]
	fn diagnostic_outputs_reject_duplicate_and_aliased_paths() {
		for args in [
			&["--cpuprofile", "profile.out", "--trace-out", "./profile.out"][..],
			&["--heapprofile", "profile.out", "--timings", "profile.out"],
		] {
			assert!(validate_diagnostic_paths(&parsed(args).outputs).is_err(), "{args:?}");
		}
		let dir = tempfile::tempdir().expect("temp dir");
		let target = dir.path().join("target.prof");
		let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
		// `same_file` only knows inodes, so links are only caught on Unix.
		#[cfg(unix)]
		{
			let dangling = dir.path().join("dangling.prof");
			std::os::unix::fs::symlink(&target, &dangling).expect("symlink");
			assert!(
				validate_diagnostic_paths(&[("--cpuprofile", text(&target)), ("--trace-out", text(&dangling))])
					.is_err()
			);
			assert!(!target.exists());
			std::fs::write(&target, "sentinel").expect("write");
			let hard = dir.path().join("hard.prof");
			std::fs::hard_link(&target, &hard).expect("hard link");
			assert!(
				validate_diagnostic_paths(&[("--cpuprofile", text(&target)), ("--trace-out", text(&hard))]).is_err()
			);
		}
		let other = dir.path().join("other.prof");
		assert!(validate_diagnostic_paths(&[("--cpuprofile", text(&target)), ("--trace-out", text(&other))]).is_ok());
	}

	#[test]
	fn project_compile_options_carry_the_concurrency_controls_and_argv_layer() {
		let opts = MergedOptions {
			builders: Some(3),
			checkers: Some(3),
			single_threaded: Some(true),
			..MergedOptions::default()
		};
		let got = project_compile_options("tsconfig.json", &opts);
		assert_eq!(
			(got.builders, got.checkers, got.single_threaded),
			(Some(3), Some(3), Some(true))
		);
		let missing = project_compile_options("tsconfig.json", &MergedOptions::default());
		assert_eq!((missing.builders, missing.checkers), (None, None));

		let opts = MergedOptions {
			luau_types: true,
			argv: Some(PartialOptions {
				luau_types: Some(false),
				..PartialOptions::default()
			}),
			..MergedOptions::default()
		};
		let got = project_compile_options("tsconfig.json", &opts);
		assert!(got.luau_types);
		assert_eq!(got.solution_argv.map(|argv| argv.luau_types), Some(Some(false)));
		let silent = MergedOptions {
			argv: Some(PartialOptions::default()),
			..MergedOptions::default()
		};
		let got = options::project_compile_options("tsconfig.json", &silent);
		assert!(!got.luau_types);
		assert_eq!(got.solution_argv.map(|argv| argv.luau_types), Some(None));
	}
}
