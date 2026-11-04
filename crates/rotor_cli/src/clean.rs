//! `sloptor clean` (`cmd/rotor/clean.go`): removes a project's build outputs (the tsconfig
//! `outDir` and the runtime include folder) and, with `--types`, the generated editor types. It
//! never touches source; `--dry-run` only lists.

use std::time::{Duration, Instant};

use rotor_support::gopath;

use crate::app::Failure;
use crate::cli::CleanCommand;
use crate::options::{MergedOptions, find_ts_config_path, include_dir, merge_project_options, read_rbxts_layer};
use crate::term::Streams;
use crate::ui::{self, Event, EventStatus};

/// The generated editor companions `--types` removes, when present.
const TYPE_FILES: [&str; 3] = [
	rotor_compile::rotortypes::ROTOR_TYPES_FILE_NAME,
	rotor_compile::envdecl::ENV_DECL_COMPANION_FILE_NAME,
	rotor_compile::macrodecl::MACRO_DECL_COMPANION_FILE_NAME,
];

/// What one clean found and did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanReport {
	/// The project directory.
	pub dir: String,
	/// One row per target that existed or failed, then the finished row.
	pub events: Vec<Event>,
	/// Whether a target could not be removed.
	pub failed: bool,
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
		let outcome = clean_target(target, dry_run);
		// After the removal, as Go renders it: a removed directory loses its trailing slash.
		let shown = display(&dir, target);
		match outcome {
			Err(error) => {
				events.push(Event {
					status: EventStatus::Failed,
					target: shown,
					detail: format!("cannot remove: {}", rotor_support::gostr::io_error_text(&error)),
					elapsed: Duration::ZERO,
				});
				failed = true;
			}
			Ok(None) => {}
			Ok(Some(count)) => events.push(Event {
				status: if dry_run {
					EventStatus::Planned
				} else {
					EventStatus::Removed
				},
				target: shown,
				detail: format!(
					"{} {}",
					ui::plural(count, "file"),
					if dry_run { "would remove" } else { "removed" }
				),
				elapsed: Duration::ZERO,
			}),
		}
	}
	if events.is_empty() {
		events.push(Event {
			status: EventStatus::Unchanged,
			target: "nothing to clean".to_owned(),
			detail: String::new(),
			elapsed: Duration::ZERO,
		});
	}
	events.push(Event {
		status: EventStatus::Finished,
		target: String::new(),
		detail: String::new(),
		elapsed: started.elapsed().max(Duration::from_nanos(1)),
	});
	Ok(CleanReport { dir, events, failed })
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
	let out = streams.out_styler();
	ui::banner(streams.out, out, &format!("clean  {}", gopath::base(&dir)));
	let report = clean(&path, command.types == Some(true), command.dry_run == Some(true)).map_err(Failure::Runtime)?;
	ui::events(streams.out, out, &report.events);
	let _ = writeln!(streams.out);
	if report.failed {
		return Err(Failure::Reported);
	}
	Ok(())
}
