//! `sloptor check` (`cmd/rotor/check.go`): a full-strictness typecheck of one project, styled or
//! as one `--json` result object.

use std::time::{Duration, Instant};

use rotor_support::{gopath, logservice};
use rotor_tsrs::compiler::diagnosticwriter::{
	FormattingOptions, format_diagnostic_with_color_and_context, write_format_diagnostic,
};
use rotor_tsrs::compiler::{get_diagnostics_of_any_program, sort_and_deduplicate_diagnostics};
use rotor_tsrs::diagnostics::Category;
use rotor_tsrs::{Context, P, tspath};

use crate::app::{Failure, not_ported};
use crate::cli::CheckCommand;
use crate::json::{JsonDiagnostic, JsonResult};
use crate::term::Streams;
use crate::{VERSION, ui};

/// How the diagnostics are rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Render {
	/// `--json` entries.
	Json,
	/// tsc's plain `file(line,col): error TS####: message` lines.
	Plain,
	/// tsc's coloured frames.
	Color,
}

/// One check pass (`checkCore`), already rendered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckPass {
	/// The `--json` entries (only for [`Render::Json`]).
	pub json: Vec<JsonDiagnostic>,
	/// The formatted diagnostics (only for the styled renders).
	pub text: Vec<u8>,
	/// Diagnostics, of any category.
	pub diagnostics: usize,
	/// Error diagnostics.
	pub errors: usize,
	/// Source files in the program.
	pub files: usize,
	/// Wall time.
	pub elapsed: Duration,
}

/// `runCheckCore`: builds the project's program and collects its diagnostics in tsc's order
/// (config, syntactic, options, global, then semantic diagnostics of the project's own files),
/// sorted and deduplicated, then refreshes `rotor.d.ts` when a source uses a macro.
#[must_use]
pub fn check_pass(dir: &str, checkers: Option<i64>, render: Render, warnings: &mut dyn std::io::Write) -> CheckPass {
	let started = Instant::now();
	let slash_dir = gopath::to_slash(dir).into_owned();
	let mut pass = rotor_tsrs::with_large_stack(|| check_on_large_stack(&slash_dir, checkers, render));
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

fn check_on_large_stack(slash_dir: &str, checkers: Option<i64>, render: Render) -> (CheckPass, Option<String>) {
	let compare_paths_options = tspath::ComparePathsOptions {
		use_case_sensitive_file_names: rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
		current_directory: slash_dir.to_owned(),
	};
	let config_path = format!("{slash_dir}/tsconfig.json");
	let loaded = rotor_tsrs::project::load_check_program(slash_dir, &config_path, checkers);
	let Some(program) = loaded.program else {
		let pass = render_pass(&loaded.config_diagnostics, &compare_paths_options, render, 0);
		return (pass, None);
	};
	let context = Context::default();
	let file_names = loaded.file_names;
	let mut bind = |context: &Context, file| program.get_bind_diagnostics(context, file);
	let mut semantic = |context: &Context, _| {
		let mut out = Vec::new();
		for name in &file_names {
			if let Some(file) = program.get_source_file(name) {
				out.extend(program.get_semantic_diagnostics(context, Some(file)));
			}
		}
		out
	};
	let diagnostics = get_diagnostics_of_any_program(&context, program, None, false, &mut bind, &mut semantic);
	let diagnostics = sort_and_deduplicate_diagnostics(&diagnostics);
	let pass = render_pass(
		&diagnostics,
		&compare_paths_options,
		render,
		program.source_files().len(),
	);
	let warning = refresh_rotor_types(slash_dir, &file_names, program).err();
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
	let format = FormattingOptions {
		compare_paths_options: compare_paths_options.clone(),
		new_line: "\n".to_owned(),
	};
	match render {
		Render::Json => {
			pass.json = diagnostics
				.iter()
				.map(|&diagnostic| json_diagnostic(diagnostic, compare_paths_options))
				.collect();
		}
		Render::Plain => {
			for &diagnostic in diagnostics {
				write_format_diagnostic(&mut pass.text, diagnostic, &format);
			}
		}
		Render::Color => {
			for (index, &diagnostic) in diagnostics.iter().enumerate() {
				if index > 0 {
					pass.text.push(b'\n');
				}
				format_diagnostic_with_color_and_context(&mut pass.text, diagnostic, &format);
			}
			if !diagnostics.is_empty() {
				pass.text.push(b'\n');
			}
		}
	}
	pass
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
	if command.watch == Some(true) {
		return Err(not_ported("check --watch"));
	}
	if std::fs::metadata(gopath::join(&[&dir, "package.json"])).is_ok()
		&& std::fs::metadata(gopath::join(&[&dir, "node_modules"])).is_err()
	{
		let err = streams.err_styler();
		ui::warn(
			streams.err,
			err,
			&format!(
				"{dir} has a package.json but no node_modules — type packages (e.g. @rbxts/*) cannot be resolved; \
				 install dependencies first"
			),
		);
	}

	if command.json == Some(true) {
		// stdout carries exactly one JSON object, so compiler warnings move to stderr.
		let previous = logservice::set_output_stderr(true);
		let pass = check_pass(&dir, command.checkers, Render::Json, streams.err);
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

	let out = streams.out_styler();
	ui::banner(streams.out, out, &gopath::base(&dir));
	let render = if streams.out_color {
		Render::Color
	} else {
		Render::Plain
	};
	let pass = check_pass(&dir, command.checkers, render, streams.err);
	if pass.diagnostics > 0 {
		let _ = streams.out.write_all(&pass.text);
		let _ = streams.out.write_all(b"\n");
	}
	ui::check_summary(streams.out, out, pass.files, pass.errors, pass.elapsed);
	if pass.errors > 0 {
		return Err(Failure::Reported);
	}
	Ok(())
}
