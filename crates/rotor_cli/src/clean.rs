//! `sloptor clean` (`cmd/rotor/clean.go`): removes a project's build outputs (the tsconfig
//! `outDir` and the runtime include folder) and, with `--types`, the generated editor types. It
//! never touches source; `--dry-run` only lists.

use std::time::{Duration, Instant};

use rotor_support::gopath;

use crate::app::Failure;
use crate::cli::CleanCommand;
use crate::options::{MergedOptions, find_ts_config_path, include_dir, merge_project_options, read_rbxts_layer};
use crate::term::Streams;
use crate::ui::{self, Tone};

/// The generated editor companions `--types` removes, when present.
const TYPE_FILES: [&str; 3] = [
	rotor_compile::rotortypes::ROTOR_TYPES_FILE_NAME,
	rotor_compile::envdecl::ENV_DECL_COMPANION_FILE_NAME,
	rotor_compile::macrodecl::MACRO_DECL_COMPANION_FILE_NAME,
];

/// What happened to one target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
	/// Removed: its file count.
	Removed(usize),
	/// A dry run's would-be removal: its file count.
	Planned(usize),
	/// It could not be removed.
	Failed(String),
}

/// One target that existed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
	/// The target relative to the project, `/` after a directory.
	pub target: String,
	/// What happened.
	pub outcome: Outcome,
}

/// What one clean found and did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanReport {
	/// The project directory.
	pub dir: String,
	/// One row per target that existed.
	pub events: Vec<Event>,
	/// Whether a target could not be removed.
	pub failed: bool,
	/// Wall time.
	pub elapsed: Duration,
}

/// `resolveOutDir`: the tsconfig `outDir` (default `out`), resolved against the tsconfig's
/// directory. A raw read of the one file, like the rbxts key; `extends` is not followed.
#[must_use]
pub fn resolve_out_dir(dir: &str, ts_config_path: &str) -> String {
	let out_dir = std::fs::read(ts_config_path)
		.ok()
		.and_then(|data| {
			let text = rotor_tsrs::tsconfig::strip_jsonc(&String::from_utf8_lossy(&data));
			let root: serde_json::Value = serde_json::from_str(&text).ok()?;
			let out_dir = root.get("compilerOptions")?.get("outDir")?.as_str()?;
			(!out_dir.is_empty()).then(|| out_dir.to_owned())
		})
		.unwrap_or_else(|| "out".to_owned());
	if gopath::is_abs(&out_dir) {
		return gopath::clean(&out_dir);
	}
	gopath::join(&[dir, &gopath::from_slash(&out_dir)])
}

/// The regular files under `target`, or 1 for a file.
fn count_files(target: &str, metadata: &std::fs::Metadata) -> usize {
	if !metadata.is_dir() {
		return 1;
	}
	let mut count = 0;
	let mut pending = vec![std::path::PathBuf::from(target)];
	while let Some(dir) = pending.pop() {
		let Ok(entries) = std::fs::read_dir(&dir) else {
			continue;
		};
		for entry in entries.flatten() {
			match entry.file_type() {
				Ok(kind) if kind.is_dir() => pending.push(entry.path()),
				Ok(_) => count += 1,
				Err(_) => {}
			}
		}
	}
	count
}

/// `cleanTarget`: the file count of `target` and whether it existed, removing it unless
/// `dry_run`.
fn clean_target(target: &str, dry_run: bool) -> Result<Option<usize>, std::io::Error> {
	let Ok(metadata) = std::fs::metadata(target) else {
		return Ok(None);
	};
	let count = count_files(target, &metadata);
	if !dry_run {
		if metadata.is_dir() {
			std::fs::remove_dir_all(target)?;
		} else {
			std::fs::remove_file(target)?;
		}
	}
	Ok(Some(count))
}

/// `relDisplay` for a clean row: relative to the project, with `/` after a directory.
fn display(dir: &str, target: &str) -> String {
	crate::build::rel_display(dir, target)
}

/// Resolves and cleans the project at `path`.
///
/// # Errors
///
/// No tsconfig could be found.
pub fn clean(path: &str, types: bool, dry_run: bool) -> Result<CleanReport, String> {
	let started = Instant::now();
	let ts_config_path = find_ts_config_path(path)?;
	let dir = gopath::dir(&ts_config_path);
	let out_dir = resolve_out_dir(&dir, &ts_config_path);
	let rbxts = read_rbxts_layer(&ts_config_path).ok().flatten();
	let opts = merge_project_options(MergedOptions::default(), &[rbxts.as_ref()]);
	let mut targets = vec![out_dir];
	let include = include_dir(&dir, &opts);
	if !include.is_empty() {
		targets.push(include);
	}
	if types {
		targets.extend(TYPE_FILES.iter().map(|name| gopath::join(&[&dir, name])));
	}

	let mut events = Vec::new();
	let mut failed = false;
	for target in &targets {
		let shown = display(&dir, target);
		let outcome = clean_target(target, dry_run);
		let outcome = match outcome {
			Err(error) => {
				failed = true;
				Outcome::Failed(rotor_support::gostr::io_error_text(&error))
			}
			Ok(None) => continue,
			Ok(Some(count)) if dry_run => Outcome::Planned(count),
			Ok(Some(count)) => Outcome::Removed(count),
		};
		events.push(Event { target: shown, outcome });
	}
	Ok(CleanReport {
		dir,
		events,
		failed,
		elapsed: started.elapsed(),
	})
}

/// `sloptor clean`
///
/// # Errors
///
/// No tsconfig, or a target that could not be removed (already reported).
pub fn run(command: CleanCommand, streams: &mut Streams) -> Result<(), Failure> {
	let path = command.path.unwrap_or_else(|| ".".to_owned());
	// The header names the project, so the tsconfig is found first.
	let dir = gopath::dir(&find_ts_config_path(&path).map_err(Failure::Runtime)?);
	let out = streams.out_paint();
	let dry_run = command.dry_run;
	ui::header(
		streams.out,
		out,
		&[
			if dry_run { "clean --dry-run" } else { "clean" },
			&ui::project_name(&gopath::join(&[&dir, "tsconfig.json"])),
		],
	);
	let report = clean(&path, command.types, dry_run).map_err(Failure::Runtime)?;
	let width = report
		.events
		.iter()
		.map(|event| event.target.chars().count())
		.max()
		.unwrap_or(0);
	let mut removed = 0;
	for event in &report.events {
		let target = ui::pad(&out.bold(&event.target), width);
		match &event.outcome {
			Outcome::Removed(count) => {
				removed += count;
				ui::verb_line(
					streams.out,
					out,
					Tone::Success,
					"Removed",
					&format!("{target}  {}", out.muted(&ui::plural(*count, "file"))),
				);
			}
			Outcome::Planned(count) => {
				removed += count;
				ui::verb_line(
					streams.out,
					out,
					Tone::Info,
					"Would remove",
					&format!("{target}  {}", out.muted(&ui::plural(*count, "file"))),
				);
			}
			Outcome::Failed(error) => {
				ui::verb_line(
					streams.out,
					out,
					Tone::Failure,
					"Failed",
					&format!("{target}  {}", out.red_plain(&format!("cannot remove: {error}"))),
				);
			}
		}
	}
	if report.events.is_empty() {
		ui::verb_line(streams.out, out, Tone::Quiet, "Clean", &out.muted("nothing to clean"));
	} else {
		let what = if dry_run { "to remove" } else { "removed" };
		ui::verb_line(
			streams.out,
			out,
			if report.failed { Tone::Failure } else { Tone::Success },
			"Finished",
			&format!(
				"{} {what} in {}",
				out.bold(&ui::plural(removed, "file")),
				out.bold(&ui::duration(report.elapsed))
			),
		);
	}
	if report.failed {
		return Err(Failure::Reported);
	}
	Ok(())
}
