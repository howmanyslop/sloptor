//! The result block of a build: the success or failure lines of one project or a solution, the
//! diagnostic frames, and the `--verbose` timing report. The watch loop (`watch.rs`) prints the same
//! blocks after each rebuild.

use std::io::Write;
use std::time::Duration;

use rotor_compile::progress::{BuildProgress, ProjectEvent, ProjectStatus};
use rotor_compile::timings::BuildTimings;

use crate::build::Outcome;
use crate::frames::{self, Frame, Sources};
use crate::term::Streams;
use crate::ui::{self, Paint, Tone};

/// What the result block describes.
pub struct Shown<'a> {
	/// The tsconfig built.
	pub config_path: &'a str,
	/// A solution (`-b`) build.
	pub solution: bool,
	/// The frame cap; 0 shows all.
	pub max_errors: usize,
}

/// The frames of `outcome`'s diagnostics.
#[must_use]
pub fn outcome_frames(outcome: &Outcome) -> Vec<Frame> {
	let mut sources = Sources::new(None);
	let mut frames: Vec<Frame> = outcome
		.diagnostics
		.iter()
		.map(|diagnostic| sources.frame(diagnostic))
		.collect();
	if frames.is_empty()
		&& let Some(error) = &outcome.error
	{
		frames.push(sources.frame(&rotor_compile::compile::DiagnosticInfo {
			message: error.clone(),
			..rotor_compile::compile::DiagnosticInfo::default()
		}));
	}
	frames
}

/// Prints the result block of a build; returns whether it failed.
pub fn build(streams: &mut Streams, shown: &Shown, outcome: &Outcome, progress: &BuildProgress) -> bool {
	let out = streams.out_paint();
	let err = streams.err_paint();
	let mut frames = outcome_frames(outcome);
	let events = progress.finished_since(0);
	if shown.solution {
		// A blocked project has its own line; its "blocked by failed dependency" error repeats it.
		frames.retain(|frame| !(frame.location.is_none() && frame.message.contains(" blocked by failed dependency ")));
		if !events.is_empty() {
			let _ = writeln!(streams.out);
			let _ = streams.out.flush();
		}
	}
	let (errors, warnings) = frames::tally(&frames);
	let mut text = String::new();
	frames::render_all(&mut text, err, &frames, shown.max_errors);
	if !frames.is_empty() {
		let _ = streams.err.write_all(text.as_bytes());
	}
	if shown.solution {
		if outcome.error.is_some() {
			solution_failed(streams.err, err, &events, outcome.elapsed, errors, warnings);
		} else {
			solution_finished(streams.out, out, &events, outcome.elapsed, warnings);
		}
		return outcome.error.is_some();
	}
	let name = ui::project_name(shown.config_path);
	if outcome.error.is_some() {
		let mut parts = vec![format!(
			"{} in {}",
			err.bold(&name),
			err.bold(&ui::duration(outcome.elapsed))
		)];
		parts.extend(ui::counts(err, errors, warnings));
		let located = frames::files(&frames);
		if located > 0 {
			parts.push(err.muted(&ui::plural(located, "file")));
		}
		ui::verb_line(
			streams.err,
			err,
			Tone::Failure,
			"Failed",
			&ui::summary_parts(err, &parts),
		);
		return true;
	}
	let result = outcome.result.clone().unwrap_or_default();
	if result.wrote_rotor_types {
		ui::verb_line(
			streams.out,
			out,
			Tone::Info,
			"Generated",
			&format!("rotor.d.ts {}", out.muted("(editor types for sloptor macros)")),
		);
	}
	let total = result.outputs.len();
	let written = result
		.emitted_files
		.iter()
		.filter(|path| {
			std::path::Path::new(path)
				.extension()
				.is_some_and(|extension| extension == "luau" || extension == "lua")
		})
		.count();
	ui::verb_line(
		streams.out,
		out,
		Tone::Success,
		"Compiled",
		&format!(
			"{} {}",
			out.bold(&name),
			out.muted(&format!("({})", ui::config_note(shown.config_path)))
		),
	);
	let mut parts = vec![
		format!(
			"{} in {}",
			out.bold(&ui::plural(total, "file")),
			out.bold(&ui::duration(outcome.elapsed))
		),
		format!("{} written", out.bold(&ui::thousands(written))),
		format!("{} unchanged", ui::thousands(total.saturating_sub(written))),
	];
	if let Some(rate) = ui::rate(total, outcome.elapsed) {
		parts.push(out.muted(&rate));
	}
	parts.extend(ui::counts(out, 0, warnings));
	ui::verb_line(
		streams.out,
		out,
		Tone::Success,
		"Finished",
		&ui::summary_parts(out, &parts),
	);
	false
}

fn status_counts(events: &[ProjectEvent]) -> [usize; 4] {
	let mut counts = [0; 4];
	for event in events {
		counts[match event.status {
			ProjectStatus::Built => 0,
			ProjectStatus::Fresh => 1,
			ProjectStatus::Failed => 2,
			ProjectStatus::Blocked => 3,
		}] += 1;
	}
	counts
}

fn solution_finished(out: &mut dyn Write, paint: Paint, events: &[ProjectEvent], elapsed: Duration, warnings: usize) {
	let [built, fresh, _, _] = status_counts(events);
	let mut breakdown = Vec::new();
	if built > 0 {
		breakdown.push(format!("{built} built"));
	}
	if fresh > 0 {
		breakdown.push(format!("{fresh} fresh"));
	}
	ui::verb_line(
		out,
		paint,
		Tone::Success,
		"Finished",
		&format!(
			"{} {} in {}",
			paint.bold(&ui::plural(events.len(), "project")),
			paint.muted(&format!("({})", breakdown.join(", "))),
			paint.bold(&ui::duration(elapsed))
		),
	);
	let files: usize = events.iter().map(|event| event.files).sum();
	let written: usize = events.iter().map(|event| event.written).sum();
	if files == 0 {
		ui::continuation(out, &paint.muted("every project is up to date"));
		return;
	}
	let mut parts = vec![
		format!("{} compiled", paint.bold(&ui::plural(files, "file"))),
		format!("{} written", paint.bold(&ui::thousands(written))),
		format!("{} unchanged", ui::thousands(files.saturating_sub(written))),
	];
	if let Some(rate) = ui::rate(files, elapsed) {
		parts.push(paint.muted(&rate));
	}
	ui::continuation(out, &ui::summary_parts(paint, &parts));
	if warnings > 0 {
		ui::continuation(out, &ui::summary_parts(paint, &ui::counts(paint, 0, warnings)));
	}
}

fn solution_failed(
	out: &mut dyn Write,
	paint: Paint,
	events: &[ProjectEvent],
	elapsed: Duration,
	errors: usize,
	warnings: usize,
) {
	let [built, fresh, failed, blocked] = status_counts(events);
	let mut breakdown = Vec::new();
	for (count, word) in [
		(failed, "failed"),
		(blocked, "blocked"),
		(built, "built"),
		(fresh, "fresh"),
	] {
		if count > 0 {
			breakdown.push(format!("{count} {word}"));
		}
	}
	let what = if events.is_empty() {
		paint.bold("the solution")
	} else {
		format!(
			"{} {}",
			paint.bold(&ui::plural(events.len(), "project")),
			paint.muted(&format!("({})", breakdown.join(", ")))
		)
	};
	let mut parts = vec![format!("{what} in {}", paint.bold(&ui::duration(elapsed)))];
	parts.extend(ui::counts(paint, errors, warnings));
	ui::verb_line(out, paint, Tone::Failure, "Failed", &ui::summary_parts(paint, &parts));
}

/// The `--verbose` report: work per phase, then the slowest files and (for a solution) projects.
pub fn verbose(
	out: &mut dyn Write,
	paint: Paint,
	timings: Option<&BuildTimings>,
	progress: &BuildProgress,
	solution: bool,
) {
	let Some(timings) = timings else {
		return;
	};
	let report = timings.report();
	let stages = report.stages;
	let phases: [(&str, i64); 5] = [
		(
			"Program load",
			stages.initial_program_ms + stages.overlay_program_ms + stages.project_context_ms,
		),
		("Checks", stages.semantic_diagnostics_ms),
		(
			"Transform",
			stages.native_transform_render_ms
				+ stages.sidecar_session_wait_ms
				+ stages.sidecar_preparation_ms
				+ stages.sidecar_round_trip_ms
				+ stages.sidecar_response_decode_ms,
		),
		("Emit", stages.declaration_emit_ms),
		(
			"Write",
			stages.compiled_output_writes_ms
				+ stages.declaration_emit_writes_ms
				+ stages.include_copy_ms
				+ stages.non_compiled_copy_ms
				+ stages.cleanup_ms
				+ stages.incremental_selection_ms
				+ stages.incremental_manifest_ms
				+ stages.persistence_ms,
		),
	];
	let total: i64 = phases.iter().map(|(_, ms)| ms).sum();
	let _ = writeln!(out);
	let _ = writeln!(
		out,
		"  {} {}",
		paint.bold("Phases"),
		paint.muted(if solution {
			"(work, summed over projects)"
		} else {
			"(work)"
		})
	);
	for (name, ms) in phases {
		#[expect(
			clippy::cast_precision_loss,
			clippy::cast_possible_truncation,
			clippy::cast_sign_loss,
			reason = "a rounded share for display"
		)]
		let share = if total > 0 {
			(ms as f64 * 100.0 / total as f64).round() as usize
		} else {
			0
		};
		let time = ui::duration(Duration::from_millis(u64::try_from(ms).unwrap_or(0)));
		let line = format!(
			"    {} {}  {}  {}",
			ui::pad(name, 14),
			ui::pad_left(&time, 8),
			ui::pad_left(&paint.muted(&format!("{share}%")), 4),
			paint.pink(&"━".repeat(share.div_ceil(3)))
		);
		let _ = writeln!(out, "{}", line.trim_end());
	}
	let total = ui::duration(Duration::from_millis(u64::try_from(total).unwrap_or(0)));
	let _ = writeln!(out, "    {} {}", ui::pad("", 14), ui::pad_left(&paint.bold(&total), 8));

	let files = progress.slowest_files(5);
	if !files.is_empty() {
		let _ = writeln!(out);
		let _ = writeln!(out, "  {} {}", paint.bold("Slowest files"), paint.muted("(transform)"));
		for file in files {
			let _ = writeln!(
				out,
				"    {} {}",
				ui::pad(&ui::rel_for_display(&file.file_name), 48),
				ui::pad_left(&ui::duration(file.elapsed), 8)
			);
		}
	}
	if solution {
		let mut projects = report.projects;
		projects.sort_by_key(|project| std::cmp::Reverse(project.build_wall_ms));
		projects.truncate(5);
		if !projects.is_empty() {
			let _ = writeln!(out);
			let _ = writeln!(out, "  {}", paint.bold("Slowest projects"));
			for project in projects {
				let time = ui::duration(Duration::from_millis(u64::try_from(project.build_wall_ms).unwrap_or(0)));
				let _ = writeln!(
					out,
					"    {} {}",
					ui::pad(&ui::project_name(&project.config_path), 48),
					ui::pad_left(&time, 8)
				);
			}
		}
	}
}
