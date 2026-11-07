//! `sloptor build`: the rbxtsc build flag surface, its implication checks, and the styled or
//! `--json` one-shot build.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rotor_compile::compile::DiagnosticInfo;
use rotor_compile::output::{BuildFailure, BuildResult};
use rotor_compile::progress::BuildProgress;
use rotor_compile::timings::BuildTimings;
use rotor_compile::{ProjectOptions, build_project_with_options, build_solution_with_options};
use rotor_support::{gopath, logservice};

use crate::app::{Failure, usage};
use crate::cli::{BuildCommand, NonNegative, Positive};
use crate::json::{JsonDiagnostic, JsonResult};
use crate::options::{
	MergedOptions, PartialOptions, find_ts_config_path, merge_project_options, project_compile_options,
	read_rbxts_layer,
};
use crate::term::Streams;
use crate::{VERSION, live, report, ui};

/// The project path, the argv layer (exactly the flags passed), and the rest of the build
/// surface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one field per boolean flag")]
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
	/// Where `--timings` writes its report; empty when it was not given.
	pub timings: String,
	/// 0 renders every frame.
	pub max_errors: usize,
	pub bell: bool,
	pub clear_screen: bool,
}

/// A three-state boolean from its two flags. They override each other, so at most one was given.
const fn either(value: Option<bool>, negated: bool) -> Option<bool> {
	if negated { Some(false) } else { value }
}

/// The parsed flags as [`BuildArgs`].
///
/// # Errors
///
/// A usage error when both the positional path and `--project` are given, or `--timings` has no
/// path.
pub fn collect(command: BuildCommand) -> Result<BuildArgs, Failure> {
	if let (Some(path), Some(_)) = (&command.path, &command.project) {
		return Err(usage(
			"build",
			format!("unexpected extra argument '{path}' (the project is already set by --project)"),
		));
	}
	if command.timings.as_deref() == Some("") {
		return Err(usage("build", "--timings requires a path"));
	}
	let project = command.path.or(command.project).unwrap_or_else(|| ".".to_owned());
	Ok(BuildArgs {
		project,
		opts: PartialOptions {
			include_path: command.include_path,
			rojo: command.rojo,
			type_name: command.project_type.map(|kind| kind.name().to_owned()),
			watch: either(command.watch, command.no_watch),
			use_polling: either(command.use_polling, command.no_use_polling),
			verbose: either(command.verbose, command.no_verbose),
			no_include: command.no_include,
			log_truthy_changes: either(command.log_truthy_changes, command.no_log_truthy_changes),
			write_only_changed: either(command.write_only_changed, command.no_write_only_changed),
			write_transformed_files: either(command.write_transformed_files, command.no_write_transformed_files),
			optimized_array_appends: either(command.optimized_array_appends, command.no_optimized_array_appends),
			optimized_loops: either(command.optimized_loops, command.no_optimized_loops),
			allow_comment_directives: either(command.allow_comment_directives, command.no_allow_comment_directives),
			luau: either(command.luau, command.no_luau),
			luau_types: either(command.luau_types, command.no_luau_types),
		},
		build: command.build.is_some(),
		build_path: command.build.unwrap_or_default(),
		emit_declaration_only: command.emit_declaration_only,
		builders: command.builders.map(Positive::get),
		checkers: command.checkers.map(Positive::get),
		single_threaded: either(command.single_threaded, command.no_single_threaded),
		json: command.json,
		timings: command.timings.unwrap_or_default(),
		max_errors: command.max_errors.map_or(50, NonNegative::get),
		bell: command.bell,
		clear_screen: command.clear,
	})
}

/// The option implications, checked before anything is built.
///
/// # Errors
///
/// The usage error of the first implication that fails.
pub fn check_implications(args: &BuildArgs) -> Result<(), Failure> {
	if args.builders.is_some() && !args.build {
		return Err(usage("build", "--builders requires --build"));
	}
	// Presence, not the value: `--use-polling=false` without `--watch` is as confused as `--use-polling`.
	if args.opts.use_polling.is_some() && args.opts.watch.is_none() {
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
	if watch && !args.timings.is_empty() {
		return Err(usage("build", "--timings cannot be used with --watch"));
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
	check_implications(&args)?;
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

#[cfg(test)]
mod tests {
	//! The flag semantics of `build`, parsed through the real command tree.

	use std::ffi::OsStr;

	use usage_rs::diagnostic;

	use super::{BuildArgs, check_implications, collect};
	use crate::app::{Failure, render_usage};
	use crate::cli::{Cli, Command};
	use crate::options::{self, MergedOptions, PartialOptions, project_compile_options};

	fn describe(failure: Failure) -> String {
		match failure {
			Failure::Usage { command, message } => render_usage(command, &message, false),
			other => format!("{other:?}"),
		}
	}

	fn parse(args: &[&str]) -> Result<BuildArgs, String> {
		let mut tokens = vec![OsStr::new("build")];
		tokens.extend(args.iter().map(OsStr::new));
		let parsed = Cli::parse_from(&tokens)
			.map_err(|error| diagnostic::render(Cli::spec(), &tokens, &error, diagnostic::Style::PLAIN))?;
		let Some(Command::Build(command)) = parsed.command else {
			return Err("not a build".to_owned());
		};
		let args = collect(*command).map_err(describe)?;
		check_implications(&args).map_err(describe)?;
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
		assert!(!got.json);
		assert_eq!(got.builders, None);
		assert_eq!(got.checkers, None);
		assert_eq!(got.single_threaded, None);
		assert_eq!(got.timings, "");
	}

	#[test]
	fn boolean_value_forms() {
		for args in [
			&["--no-luau"][..],
			&["--luau=false"],
			&["--luau", "--no-luau"],
			&["--no-luau", "--luau=false"],
		] {
			assert_eq!(parsed(args).opts.luau, Some(false), "{args:?}");
		}
		for args in [
			&["--luau"][..],
			&["--luau=true"],
			&["--no-luau", "--luau"],
			&["--luau=false", "--luau"],
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
		for spelling in ["maybe", "1", "TRUE"] {
			rejected(
				&[&format!("--luau={spelling}")],
				&format!("invalid value '{spelling}' for '--luau'"),
			);
		}
	}

	#[test]
	fn every_tri_state_boolean_has_a_negation() {
		type Read = fn(&BuildArgs) -> Option<bool>;
		let negations: [(&str, Read); 12] = [
			("watch", |args| args.opts.watch),
			("use-polling", |args| args.opts.use_polling),
			("verbose", |args| args.opts.verbose),
			("log-truthy-changes", |args| args.opts.log_truthy_changes),
			("write-only-changed", |args| args.opts.write_only_changed),
			("write-transformed-files", |args| args.opts.write_transformed_files),
			("optimized-array-appends", |args| args.opts.optimized_array_appends),
			("optimized-loops", |args| args.opts.optimized_loops),
			("allow-comment-directives", |args| args.opts.allow_comment_directives),
			("luau", |args| args.opts.luau),
			("luau-types", |args| args.opts.luau_types),
			("single-threaded", |args| args.single_threaded),
		];
		for (name, read) in negations {
			let extra: &[&str] = if name == "use-polling" { &["--watch"] } else { &[] };
			let negated = format!("--no-{name}");
			let positive = format!("--{name}");
			assert_eq!(read(&parsed(&[extra, &[&negated]].concat())), Some(false), "{negated}");
			assert_eq!(
				read(&parsed(&[extra, &[&negated, &positive]].concat())),
				Some(true),
				"{negated} {positive}"
			);
			assert_eq!(read(&parsed(extra)), None, "--{name} absent");
		}
	}

	#[test]
	fn plain_booleans_negate_natively() {
		assert!(!parsed(&["--json", "--no-json"]).json);
		assert!(parsed(&["--json=true"]).json);
		assert!(!parsed(&["--bell=false"]).bell);
		assert!(!parsed(&["--no-clear"]).clear_screen);
		assert!(!parsed(&["--clear=false"]).clear_screen);
		assert!(parsed(&["--no-clear", "--clear"]).clear_screen);
		assert!(!parsed(&["--build", "--emit-declaration-only=false"]).emit_declaration_only);
	}

	#[test]
	fn kebab_names_and_rbxtsc_camel_aliases_are_the_same_flag() {
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
				let kebab_arg = format!("{kebab}{value}");
				let camel_arg = format!("{camel}{value}");
				assert_eq!(
					parsed(&[extra, &[&kebab_arg]].concat()),
					parsed(&[extra, &[&camel_arg]].concat()),
					"{kebab_arg} vs {camel_arg}"
				);
			}
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
			("no-include", opts.no_include),
			("log-truthy-changes", opts.log_truthy_changes),
			("write-only-changed", opts.write_only_changed),
			("write-transformed-files", opts.write_transformed_files),
			("allow-comment-directives", opts.allow_comment_directives),
			("optimized-array-appends", opts.optimized_array_appends),
			("luau-types", opts.luau_types),
		] {
			assert_eq!(value, Some(true), "--{name}");
		}
	}

	#[test]
	fn type_and_string_flags() {
		assert_eq!(parsed(&["--type", "model"]).opts.type_name.as_deref(), Some("model"));
		rejected(&["--type", "bogus"], "invalid value 'bogus'");
		rejected(&["--type"], "a value is required for '--type <KIND>'");
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
		rejected(&["a", "-p", "b"], "Usage: sloptor build");
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
		rejected(&["--no-use-polling"], "--use-polling requires --watch");
		assert_eq!(parsed(&["--use-polling", "-w"]).opts.use_polling, Some(true));
		assert_eq!(parsed(&["--use-polling", "--no-watch"]).opts.watch, Some(false));
		rejected(&["--emit-declaration-only"], "--emit-declaration-only requires --build");
		assert!(parsed(&["--emit-declaration-only=false"]).opts.watch.is_none());
		rejected(
			&["--build", "--watch", "--emit-declaration-only"],
			"--build --watch is incompatible with --emit-declaration-only",
		);
		rejected(&["--builders", "2"], "--builders requires --build");
		rejected(
			&["--watch", "--timings", "t.json"],
			"--timings cannot be used with --watch",
		);
		rejected(&["--timings="], "--timings requires a path");
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
		assert_eq!(parsed(&["--no-single-threaded"]).single_threaded, Some(false));
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
				&format!("invalid value '{value}' for '--builders': must be a positive integer"),
			);
			rejected(&["--checkers", value], "must be a positive integer");
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
		rejected(&["--max-errors", "-1"], "must be a non-negative integer");
		assert!(parsed(&["--bell"]).bell);
	}

	#[test]
	fn timings_take_a_path() {
		for args in [&["--timings", "report.json"][..], &["--timings=report.json"]] {
			assert_eq!(parsed(args).timings, "report.json");
		}
	}

	#[test]
	fn removed_go_profile_flags_are_unknown() {
		for flag in [
			"--cpuprofile",
			"--trace-out",
			"--blockprofile",
			"--mutexprofile",
			"--heapprofile",
		] {
			rejected(&[flag, "p.out"], &format!("unexpected argument '{flag}'"));
		}
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
