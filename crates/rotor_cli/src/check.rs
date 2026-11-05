//! `sloptor check` (`cmd/rotor/check.go`): a full-strictness typecheck of one project, styled or
//! as one `--json` result object.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rotor_compile::progress::{BuildProgress, Phase};
use rotor_support::{gopath, logservice};
use rotor_tsrs::compiler::diagnosticwriter::write_flattened_diagnostic_message;
use rotor_tsrs::compiler::{get_diagnostics_of_any_program, sort_and_deduplicate_diagnostics};
use rotor_tsrs::diagnostics::Category;
use rotor_tsrs::{Context, P, tspath};

use crate::app::Failure;
use crate::cli::CheckCommand;
use crate::frames::{self, Frame, Location, Related, Severity};
use crate::json::{JsonDiagnostic, JsonResult};
use crate::term::Streams;
use crate::ui::{self, Tone};
use crate::{VERSION, live};

/// How the diagnostics are rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Render {
	/// `--json` entries.
	Json,
	/// Frames for the styled output.
	Frames,
}

/// One check pass (`checkCore`), already rendered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckPass {
	/// The `--json` entries (only for [`Render::Json`]).
	pub json: Vec<JsonDiagnostic>,
	/// The frames (only for [`Render::Frames`]).
	pub frames: Vec<Frame>,
	/// Diagnostics, of any category.
	pub diagnostics: usize,
	/// Error diagnostics.
	pub errors: usize,
	/// Source files in the program.
	pub files: usize,
	/// Wall time.
	pub elapsed: Duration,
	/// What `check --watch` polls: the tsconfig, the root files, then the macro inputs.
	pub watch_files: Vec<String>,
}

/// `runCheckCore`: builds the project's program and collects its diagnostics in tsc's order
/// (config, syntactic, options, global, then semantic diagnostics of the project's own files),
/// sorted and deduplicated, then refreshes `rotor.d.ts` when a source uses a macro.
#[must_use]
pub fn check_pass(
	dir: &str,
	checkers: Option<i64>,
	render: Render,
	progress: Option<&BuildProgress>,
	warnings: &mut dyn std::io::Write,
) -> CheckPass {
	check_pass_in(dir, checkers, render, progress, warnings, false)
}

/// [`check_pass`]; with `scoped`, the pass frees its program, checkers and their types when it
/// ends, so a watch session's passes do not accumulate, and it runs on the calling thread, which
/// must have a large stack (a watch session checks on one thread for good).
#[must_use]
pub fn check_pass_in(
	dir: &str,
	checkers: Option<i64>,
	render: Render,
	progress: Option<&BuildProgress>,
	warnings: &mut dyn std::io::Write,
	scoped: bool,
) -> CheckPass {
	let started = Instant::now();
	let slash_dir = gopath::to_slash(dir).into_owned();
	let mut pass = if scoped {
		// SAFETY: the pass returns owned data only (rendered frames and JSON entries, counts and
		// paths), and every thread that checks for it is done before it returns, so nothing
		// from the scope's program, or allocated in its region, outlives the scope.
		#[expect(unsafe_code, reason = "check --watch frees each pass's program")]
		let _programs = unsafe { rotor_tsrs::project::ProgramScope::enter() };
		check_on_large_stack(&slash_dir, checkers, render, progress)
	} else {
		rotor_tsrs::with_large_stack(|| check_on_large_stack(&slash_dir, checkers, render, progress))
	};
	if let Some(error) = pass.1 {
		let _ = writeln!(
			warnings,
			"sloptor check: warning: cannot write {}: {error}",
			rotor_compile::rotortypes::ROTOR_TYPES_FILE_NAME
		);
	}
	pass.0.elapsed = started.elapsed();
	pass.0
}

fn check_on_large_stack(
	slash_dir: &str,
	checkers: Option<i64>,
	render: Render,
	progress: Option<&BuildProgress>,
) -> (CheckPass, Option<String>) {
	if let Some(progress) = progress {
		progress.set_phase(Phase::Loading);
	}
	let compare_paths_options = tspath::ComparePathsOptions {
		use_case_sensitive_file_names: rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
		current_directory: slash_dir.to_owned(),
	};
	let config_path = format!("{slash_dir}/tsconfig.json");
	let loaded = rotor_tsrs::project::load_check_program(slash_dir, &config_path, checkers);
	let Some(program) = loaded.program else {
		let mut pass = render_pass(&loaded.config_diagnostics, &compare_paths_options, render, 0);
		pass.watch_files = vec![config_path];
		return (pass, None);
	};
	let context = Context::default();
	let file_names = loaded.file_names;
	if let Some(progress) = progress {
		progress.add_files(file_names.len());
	}
	let mut bind = |context: &Context, file| program.get_bind_diagnostics(context, file);
	let mut semantic = |context: &Context, _| {
		if let Some(progress) = progress {
			progress.set_phase(Phase::Checking);
		}
		let mut out = Vec::new();
		for name in &file_names {
			if let Some(file) = program.get_source_file(name) {
				out.extend(program.get_semantic_diagnostics(context, Some(file)));
			}
			if let Some(progress) = progress {
				progress.file_checked();
			}
		}
		out
	};
	let diagnostics = get_diagnostics_of_any_program(&context, program, None, false, &mut bind, &mut semantic);
	let diagnostics = sort_and_deduplicate_diagnostics(&diagnostics);
	let mut pass = render_pass(
		&diagnostics,
		&compare_paths_options,
		render,
		program.source_files().len(),
	);
	let warning = refresh_rotor_types(slash_dir, &file_names, program).err();
	pass.watch_files = std::iter::once(config_path)
		.chain(file_names.iter().cloned())
		.chain(rotor_compile::macro_inputs::macro_input_files(
			slash_dir,
			program.source_files(),
		))
		.collect();
	(pass, warning)
}

fn render_pass(
	diagnostics: &[P<rotor_tsrs::ast::Diagnostic>],
	compare_paths_options: &tspath::ComparePathsOptions,
	render: Render,
	files: usize,
) -> CheckPass {
	let errors = diagnostics
		.iter()
		.filter(|diagnostic| diagnostic.category() == Category::Error)
		.count();
	let mut pass = CheckPass {
		diagnostics: diagnostics.len(),
		errors,
		files,
		..CheckPass::default()
	};
	match render {
		Render::Json => {
			pass.json = diagnostics
				.iter()
				.map(|&diagnostic| json_diagnostic(diagnostic, compare_paths_options))
				.collect();
		}
		Render::Frames => pass.frames = diagnostics.iter().map(|&diagnostic| frame(diagnostic)).collect(),
	}
	pass
}

/// tsc's message with its elaboration chain, one entry per line.
fn flattened(diagnostic: P<rotor_tsrs::ast::Diagnostic>) -> Vec<String> {
	let mut text = Vec::new();
	write_flattened_diagnostic_message(&mut text, diagnostic, "\n");
	String::from_utf8_lossy(&text)
		.lines()
		.map(|line| line.trim().to_owned())
		.filter(|line| !line.is_empty())
		.collect()
}

fn location(diagnostic: P<rotor_tsrs::ast::Diagnostic>) -> Option<Location> {
	let file = diagnostic.file()?;
	Location::new(
		ui::rel_for_display(&gopath::from_slash(file.file_name())),
		file.text(),
		diagnostic.pos().unwrap_or(0) as usize,
		diagnostic.len().unwrap_or(0) as usize,
	)
}

/// A checker diagnostic as a frame.
fn frame(diagnostic: P<rotor_tsrs::ast::Diagnostic>) -> Frame {
	let mut lines = flattened(diagnostic).into_iter();
	Frame {
		severity: if diagnostic.category() == Category::Error {
			Severity::Error
		} else {
			Severity::Warning
		},
		code: rotor_compile::compile::diagnostic_code(&diagnostic),
		message: lines.next().unwrap_or_default(),
		notes: lines.collect(),
		help: Vec::new(),
		location: location(diagnostic),
		related: diagnostic
			.related_information()
			.iter()
			.map(|&related| Related {
				message: flattened(related).join(" "),
				location: location(related),
			})
			.collect(),
	}
}

/// `jsonDiagnostics`: one diagnostic in the `--json` shape, with tsc's location math (1-based line,
/// UTF-16 column, project-relative file).
fn json_diagnostic(
	diagnostic: P<rotor_tsrs::ast::Diagnostic>,
	compare_paths_options: &tspath::ComparePathsOptions,
) -> JsonDiagnostic {
	let mut entry = JsonDiagnostic {
		code: rotor_compile::compile::diagnostic_code(&diagnostic),
		severity: if diagnostic.category() == Category::Error {
			"error"
		} else {
			"warning"
		},
		message: diagnostic.localize(),
		..JsonDiagnostic::default()
	};
	if let Some(file) = diagnostic.file() {
		let (line, character) =
			rotor_tsrs::scanner::get_ecma_line_and_utf16_character_of_position(&*file, diagnostic.pos().unwrap_or(0));
		entry.line = line + 1;
		entry.col = character as usize + 1;
		entry.file = tspath::convert_to_relative_path(file.file_name(), compare_paths_options);
	}
	entry
}

/// `refreshRotorTypesForCheck`: (re)writes `rotor.d.ts` when a project source uses `$env` or a
/// macro. A failure only warns.
fn refresh_rotor_types(
	dir: &str,
	file_names: &[String],
	program: &rotor_tsrs::compiler::Program,
) -> Result<(), String> {
	let sources = file_names.iter().filter(|name| !name.ends_with(".d.ts"));
	for name in sources {
		let Some(file) = program.get_source_file(name) else {
			continue;
		};
		let text = file.text();
		if !text.contains("$env") && !rotor_compile::macrodecl::source_uses_macros(text) {
			continue;
		}
		return rotor_compile::rotortypes::write_rotor_types(dir)
			.map(|_| ())
			.map_err(|error| error.to_string());
	}
	Ok(())
}

/// `sloptor check`
///
/// # Errors
///
/// A missing project, an unported mode, or type errors already reported.
pub fn run(command: CheckCommand, streams: &mut Streams) -> Result<(), Failure> {
	let project = command.path.unwrap_or_else(|| ".".to_owned());
	let dir = gopath::abs(&project)
		.map_err(|error| Failure::Runtime(format!("cannot resolve path \"{project}\": {error}")))?;
	if !std::fs::metadata(&dir).is_ok_and(|metadata| metadata.is_dir()) {
		return Err(Failure::Runtime(format!("{dir} is not a directory")));
	}
	if std::fs::metadata(gopath::join(&[&dir, "tsconfig.json"])).is_err() {
		return Err(Failure::Runtime(format!("no tsconfig.json found in {dir}")));
	}
	let watch = command.watch == Some(true);
	rotor_tsrs::project::set_long_lived(watch);
	// A watch session takes no build slot, as in Go.
	let _lease = (!watch).then(|| crate::build::admit(streams));
	let missing_modules = std::fs::metadata(gopath::join(&[&dir, "package.json"])).is_ok()
		&& std::fs::metadata(gopath::join(&[&dir, "node_modules"])).is_err();
	let modules_warning = format!(
		"{} has a package.json but no node_modules, so type packages (@rbxts/*) cannot resolve; install \
		 dependencies first",
		ui::rel_for_display(&dir)
	);

	if watch && command.json == Some(true) {
		if missing_modules {
			let err = streams.err_paint();
			ui::warn(streams.err, err, &modules_warning);
		}
		crate::watch::exit_on_interrupt(false);
		// The compiler's warnings must not corrupt the event stream.
		logservice::set_output_stderr(true);
		let mut reporter =
			crate::watch_json::JsonCheckReporter(crate::watch_json::WatchEventWriter::new(streams.out, &dir));
		rotor_tsrs::with_large_stack(|| {
			crate::watch::run_check_watch_loop(&crate::watch::Stop::default(), &dir, command.checkers, &mut reporter);
		});
		return Ok(());
	}

	if command.json == Some(true) {
		// stdout carries exactly one JSON object, so compiler warnings move to stderr.
		if missing_modules {
			let err = streams.err_paint();
			ui::warn(streams.err, err, &modules_warning);
		}
		let previous = logservice::set_output_stderr(true);
		let pass = check_pass(&dir, command.checkers, Render::Json, None, streams.err);
		logservice::set_output_stderr(previous);
		let result = JsonResult {
			version: VERSION.to_owned(),
			ok: pass.errors == 0,
			files: pass.files,
			duration_ms: pass.elapsed.as_millis(),
			diagnostics: pass.json,
		};
		let _ = streams.out.write_all(result.encode().as_bytes());
		return if result.ok { Ok(()) } else { Err(Failure::Reported) };
	}

	let out = streams.out_paint();
	let name = ui::project_name(&gopath::join(&[&dir, "tsconfig.json"]));
	ui::header(
		streams.out,
		out,
		&[if watch { "check --watch" } else { "check" }, &name],
	);
	if missing_modules {
		ui::warn(streams.out, out, &modules_warning);
	}
	if watch {
		crate::watch::exit_on_interrupt(streams.err_color && crate::term::process_terminals().1);
		let mut reporter = crate::watch::StyledCheckReporter::new(streams, &dir);
		rotor_tsrs::with_large_stack(|| {
			crate::watch::run_check_watch_loop(&crate::watch::Stop::default(), &dir, command.checkers, &mut reporter);
		});
		return Ok(());
	}
	let progress = Arc::new(BuildProgress::new(false));
	let view = live::View {
		progress_line: live::progress_line_allowed(streams),
		project_lines: false,
		solution: false,
		out,
	};
	let checkers = command.checkers;
	let mut warnings = Vec::new();
	let pass = live::run(streams, &progress, view, || {
		check_pass(&dir, checkers, Render::Frames, Some(&progress), &mut warnings)
	});
	let _ = streams.err.write_all(&warnings);
	let mut text = String::new();
	frames::render_all(&mut text, out, &pass.frames, 0);
	let _ = streams.out.write_all(text.as_bytes());
	let (errors, warned) = frames::tally(&pass.frames);
	let mut parts = vec![
		out.bold(&name),
		format!(
			"{} in {}",
			out.bold(&ui::plural(pass.files, "file")),
			out.bold(&ui::duration(pass.elapsed))
		),
	];
	let counts = ui::counts(out, errors, warned);
	if counts.is_empty() {
		parts.push(out.muted("no errors"));
	}
	parts.extend(counts);
	let (tone, word) = if pass.errors > 0 {
		(Tone::Failure, "Failed")
	} else {
		(Tone::Success, "Checked")
	};
	ui::verb_line(streams.out, out, tone, word, &ui::summary_parts(out, &parts));
	if pass.errors > 0 {
		return Err(Failure::Reported);
	}
	Ok(())
}
