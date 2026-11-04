//! `build --watch` and `check --watch` for one project (`cmd/rotor/watch.go`).
//!
//! The project tree is polled, as in Go: snapshots are pruned directory walks (`node_modules`,
//! dot-directories and the build's own output and include directories are skipped), taken every
//! 100 ms or ten times the last walk's cost, up to a second. After a change the watcher re-snapshots
//! on a 50 ms quiet timer, up to 500 ms, so an editor's "save all" lands in one rebuild. The
//! baseline is taken before each build, so an edit made while a build runs triggers the next one.
//!
//! Each rebuild is a cold build in its own program scope, so nothing from rebuild N reaches
//! rebuild N + 1.

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use rotor_compile::ProjectOptions;
use rotor_compile::progress::BuildProgress;
use rotor_support::gopath;
use sha2::Digest as _;

use crate::build::{Outcome, run_build_here};
use crate::check::{CheckPass, Render, check_pass_in};
use crate::term::Streams;
use crate::ui::{self, Paint, Tone};
use crate::{frames, live, report};

/// `watchMinInterval`
pub const MIN_INTERVAL: Duration = Duration::from_millis(100);
/// `watchMaxInterval`
pub const MAX_INTERVAL: Duration = Duration::from_secs(1);
/// `watchQuietPeriod`
pub const QUIET_PERIOD: Duration = Duration::from_millis(50);
/// `watchSettleCap`
pub const SETTLE_CAP: Duration = Duration::from_millis(500);
/// `watchHistoryLen`: the build times the sparkline shows.
pub const HISTORY_LEN: usize = 12;
/// `watchContentHashLimit`
const CONTENT_HASH_LIMIT: u64 = 1 << 20;

/// `fileStamp`: what polling compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileStamp {
	/// The file exists.
	pub exists: bool,
	/// Its modification time.
	pub mod_time: Option<SystemTime>,
	/// Its size.
	pub size: u64,
	/// The SHA-256 of a content-sensitive file's bytes.
	pub digest: Option<[u8; 32]>,
}

/// A snapshot: path to stamp.
pub type Stamps = HashMap<String, FileStamp>;

/// Ends a watch loop (Go's context).
#[derive(Clone, Default)]
pub struct Stop {
	inner: Arc<(Mutex<bool>, Condvar)>,
}

impl Stop {
	/// Stops every loop watching this token.
	#[cfg(test)]
	pub fn stop(&self) {
		*self.inner.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
		self.inner.1.notify_all();
	}

	/// Sleeps for `duration` unless stopped first; returns whether it was stopped.
	pub fn sleep(&self, duration: Duration) -> bool {
		let stopped = self.inner.0.lock().unwrap_or_else(PoisonError::into_inner);
		let (stopped, _) = self
			.inner
			.1
			.wait_timeout_while(stopped, duration, |stopped| !*stopped)
			.unwrap_or_else(PoisonError::into_inner);
		*stopped
	}
}

/// `watchStats`: per-session build counts, times and the last outcome, then the session's display
/// settings.
#[derive(Clone, Debug, Default)]
pub struct WatchStats {
	/// Builds so far, the initial one included.
	pub builds: usize,
	/// The most recent build times, oldest first, at most [`HISTORY_LEN`].
	pub history: Vec<Duration>,
	/// Whether the last build failed.
	pub last_failed: bool,
	/// The last build's error count.
	pub last_errors: usize,
	/// The frame cap; 0 shows all.
	pub max_errors: usize,
	/// Ring the bell on a fail/pass flip (stderr a terminal only).
	pub bell: bool,
	/// Clear the screen before each rebuild (a coloured terminal only).
	pub clear_screen: bool,
}

impl WatchStats {
	/// `record`
	pub fn record(&mut self, elapsed: Duration) {
		self.builds += 1;
		self.history.push(elapsed);
		if self.history.len() > HISTORY_LEN {
			self.history.drain(..self.history.len() - HISTORY_LEN);
		}
	}
}

/// `watchOptions`: the display settings of a styled session.
#[derive(Clone, Copy, Debug, Default)]
pub struct WatchOptions {
	/// The frame cap; 0 shows all.
	pub max_errors: usize,
	/// `--bell`
	pub bell: bool,
	/// `--clear`
	pub clear_screen: bool,
}

/// `treeWatcher`: polling snapshots of a project tree.
pub struct TreeWatcher {
	root: String,
	/// Watched outside the walk (the macro inputs).
	pub extra_files: Vec<String>,
	skip_dirs: Vec<String>,
	walk_cost: Duration,
}

impl TreeWatcher {
	/// `newTreeWatcher`
	#[must_use]
	pub fn new(root: &str, skip_dirs: &[String]) -> Self {
		let mut watcher = Self {
			root: root.to_owned(),
			extra_files: Vec::new(),
			skip_dirs: Vec::new(),
			walk_cost: Duration::ZERO,
		};
		watcher.set_skip_dirs(skip_dirs);
		watcher
	}

	/// `setSkipDirs`
	pub fn set_skip_dirs(&mut self, dirs: &[String]) {
		self.skip_dirs = dirs
			.iter()
			.filter(|dir| !dir.is_empty())
			.map(|dir| gopath::clean(dir))
			.collect();
	}

	/// The pruned directories.
	#[must_use]
	pub fn skip_dirs(&self) -> &[String] {
		&self.skip_dirs
	}

	/// `snapshot`
	pub fn snapshot(&mut self) -> Stamps {
		let started = std::time::Instant::now();
		let mut stamps = Stamps::new();
		self.walk(&self.root.clone(), &mut stamps);
		stamps.extend(snapshot_files(&self.extra_files));
		self.walk_cost = started.elapsed();
		stamps
	}

	fn walk(&self, dir: &str, stamps: &mut Stamps) {
		let Ok(entries) = std::fs::read_dir(dir) else {
			return;
		};
		for entry in entries.flatten() {
			let name = entry.file_name().to_string_lossy().into_owned();
			let path = gopath::join(&[dir, &name]);
			let Ok(file_type) = entry.file_type() else {
				continue;
			};
			if file_type.is_dir() {
				if should_prune_dir(&name) || self.is_skip_dir(&path) {
					continue;
				}
				self.walk(&path, stamps);
				continue;
			}
			if is_junk_file(&name) || is_generated_file(&name) {
				continue;
			}
			// The enumeration's own metadata (an lstat), as Go's `entry.Info`.
			let Ok(metadata) = entry.metadata() else {
				continue;
			};
			stamps.insert(path.clone(), watch_stamp(&path, &metadata));
		}
	}

	fn is_skip_dir(&self, path: &str) -> bool {
		self.skip_dirs.iter().any(|dir| path.eq_ignore_ascii_case(dir))
	}

	/// `interval`: ten times the last walk, clamped.
	#[must_use]
	pub fn interval(&self) -> Duration {
		(self.walk_cost * 10).clamp(MIN_INTERVAL, MAX_INTERVAL)
	}

	/// Overrides the last walk's cost (tests).
	#[cfg(test)]
	pub(crate) const fn set_walk_cost(&mut self, cost: Duration) {
		self.walk_cost = cost;
	}
}

/// Files the build writes beside its inputs, which never trigger a rebuild.
fn is_generated_file(name: &str) -> bool {
	[
		"flamework.build",
		rotor_compile::rotortypes::ROTOR_TYPES_FILE_NAME,
		rotor_tsrs::project::ENV_DECL_FILE_NAME,
		rotor_tsrs::project::MACRO_DECL_FILE_NAME,
	]
	.iter()
	.any(|generated| name.eq_ignore_ascii_case(generated))
}

/// `shouldPruneDir`: dependency trees and dot-directories.
#[must_use]
pub fn should_prune_dir(name: &str) -> bool {
	name == "node_modules" || name.starts_with('.')
}

/// `isJunkFile`: editor write noise.
#[must_use]
pub fn is_junk_file(name: &str) -> bool {
	if name.starts_with(".#") || name == "4913" || name == ".DS_Store" || name == "Thumbs.db" || name.ends_with('~') {
		return true;
	}
	matches!(
		gopath::ext(name).to_ascii_lowercase().as_str(),
		".swp" | ".swo" | ".tmp"
	)
}

/// `diffStamps`: every path whose stamp differs, added or removed, sorted.
#[must_use]
pub fn diff_stamps(before: &Stamps, after: &Stamps) -> Vec<String> {
	let mut changed: Vec<String> = after
		.iter()
		.filter(|(path, stamp)| before.get(*path) != Some(*stamp))
		.map(|(path, _)| path.clone())
		.collect();
	changed.extend(before.keys().filter(|path| !after.contains_key(*path)).cloned());
	changed.sort();
	changed
}

/// `settleChanges`: re-snapshots on the quiet timer until a snapshot adds nothing (capped), and
/// returns the settled snapshot and every path that changed since `base`.
pub fn settle_changes(
	base: &Stamps,
	first: Stamps,
	snap: &mut dyn FnMut() -> Stamps,
	sleep: &mut dyn FnMut(Duration),
) -> (Stamps, Vec<String>) {
	let mut current = first;
	let mut waited = Duration::ZERO;
	while waited < SETTLE_CAP {
		sleep(QUIET_PERIOD);
		let next = snap();
		let quiet = diff_stamps(&current, &next).is_empty();
		current = next;
		if quiet {
			break;
		}
		waited += QUIET_PERIOD;
	}
	let changed = diff_stamps(base, &current);
	(current, changed)
}

/// `awaitChanges`: polls until a settled batch lands, returning the new baseline and the changed
/// paths, or `None` once stopped.
pub fn await_changes(
	stop: &Stop,
	interval: &mut dyn FnMut() -> Duration,
	snap: &mut dyn FnMut() -> Stamps,
	mut baseline: Stamps,
) -> Option<(Stamps, Vec<String>)> {
	loop {
		if stop.sleep(interval()) {
			return None;
		}
		let next = snap();
		if diff_stamps(&baseline, &next).is_empty() {
			continue;
		}
		let (settled, changed) = settle_changes(&baseline, next, snap, &mut |duration| {
			stop.sleep(duration);
		});
		baseline = settled;
		if !changed.is_empty() {
			return Some((baseline, changed));
		}
	}
}

/// `mergePreStamps`: files in both keep their pre-build stamp, so an edit made during the build
/// still diffs on the next tick.
#[must_use]
pub fn merge_pre_stamps(mut post: Stamps, pre: &Stamps) -> Stamps {
	for (path, stamp) in pre {
		if let Some(slot) = post.get_mut(path) {
			*slot = *stamp;
		}
	}
	post
}

/// `snapshotFiles`
#[must_use]
pub fn snapshot_files(files: &[String]) -> Stamps {
	files.iter().map(|file| (file.clone(), stat(file))).collect()
}

/// `pruneStamps`: drops entries under any of `skip_dirs`.
pub fn prune_stamps(stamps: &mut Stamps, skip_dirs: &[String]) {
	stamps.retain(|path, _| {
		let lower = path.to_lowercase();
		!skip_dirs.iter().any(|dir| {
			path.eq_ignore_ascii_case(dir)
				|| lower.starts_with(&format!("{}{}", dir.to_lowercase(), std::path::MAIN_SEPARATOR))
		})
	});
}

/// `stat`: follows symlinks.
fn stat(path: &str) -> FileStamp {
	std::fs::metadata(path).map_or_else(|_| FileStamp::default(), |metadata| watch_stamp(path, &metadata))
}

/// `watchStamp`: content-sensitive files also carry a digest, so an edit that keeps the size and
/// the modification time is still seen.
fn watch_stamp(path: &str, metadata: &std::fs::Metadata) -> FileStamp {
	let mut stamp = FileStamp {
		exists: true,
		mod_time: metadata.modified().ok(),
		size: metadata.len(),
		digest: None,
	};
	if !watch_content_sensitive(path) || metadata.len() > CONTENT_HASH_LIMIT {
		return stamp;
	}
	if let Ok(contents) = std::fs::read(path) {
		stamp.digest = Some(sha2::Sha256::digest(&contents).into());
	}
	stamp
}

fn watch_content_sensitive(path: &str) -> bool {
	matches!(
		gopath::base(path).to_lowercase().as_str(),
		"rotor.toml"
			| "tsconfig.json"
			| "package.json"
			| "package-lock.json"
			| "pnpm-lock.yaml"
			| "yarn.lock"
			| "bun.lock"
			| "bun.lockb"
			| "flamework.json"
			| "flamework.build.backup"
	)
}

/// `guessedOutputDir`: the build's output directory, or `out` (or the first output's top
/// directory) before one is known.
#[must_use]
pub fn guessed_output_dir(project_dir: &str, result: Option<&rotor_compile::output::BuildResult>) -> String {
	if let Some(result) = result
		&& !result.output_dir.is_empty()
	{
		return result.output_dir.clone();
	}
	let Some(first) = result.and_then(|result| result.outputs.keys().next()) else {
		return gopath::join(&[project_dir, "out"]);
	};
	let first = gopath::clean(&gopath::from_slash(first));
	let top = first.split(std::path::MAIN_SEPARATOR).next().unwrap_or_default();
	gopath::join(&[project_dir, top])
}

/// `buildWatchReporter`: styled chrome or the `--json` stream. Each `build_start` is followed by
/// exactly one `build_end`.
pub trait BuildWatchReporter {
	/// Before each build; `None` for the initial one.
	fn build_start(&mut self, changed: Option<&[String]>);
	/// Runs one build; a styled session draws its progress line around it.
	fn run(&mut self, options: &ProjectOptions, build: &dyn Fn(&ProjectOptions) -> Outcome) -> Outcome {
		build(options)
	}
	/// After each build.
	fn build_end(&mut self, outcome: &Outcome);
	/// After each build, once the watched set is known; `files` counts it.
	fn idle(&mut self, _files: &dyn Fn() -> usize) {}
	/// Once, after the initial build. `files` is lazy because only the JSON stream reports it.
	fn watching(&mut self, files: &dyn Fn() -> usize);
}

/// `checkWatchReporter`
pub trait CheckWatchReporter {
	/// Before each check; `None` for the initial one.
	fn build_start(&mut self, changed: Option<&[String]>);
	/// Runs one check.
	fn run(&mut self, check: &dyn Fn(Render, Option<&BuildProgress>) -> CheckPass) -> CheckPass;
	/// After each check.
	fn build_end(&mut self, pass: &CheckPass);
	/// Once, after the initial check.
	fn watching(&mut self, files: &dyn Fn() -> usize);
}

/// `runBuildWatchLoop`: builds, then rebuilds on every settled change to the project tree until
/// `stop`. Runs on a large stack ([`rotor_tsrs::with_large_stack`]): every build runs on this
/// thread.
pub fn run_build_watch_loop(
	stop: &Stop,
	dir: &str,
	ts_config_path: &str,
	options: &ProjectOptions,
	include_dir: &str,
	reporter: &mut (dyn BuildWatchReporter + Send),
) {
	let mut watcher = TreeWatcher::new(dir, &[guessed_output_dir(dir, None), include_dir.to_owned()]);
	watcher.extra_files = rotor_compile::macro_inputs::macro_watch_files(ts_config_path);
	// The baseline is taken before the build, so a save during the build still differs from it.
	let mut baseline = watcher.snapshot();
	let mut changed: Option<Vec<String>> = None;
	loop {
		reporter.build_start(changed.as_deref());
		let outcome = reporter.run(options, &|options| run_build_here(dir, ts_config_path, options, false));
		reporter.build_end(&outcome);
		crate::memory::trim();
		// The build names the real output directory; prune the baseline to match, or the now
		// unwalked entries would read as deletions.
		watcher.set_skip_dirs(&[guessed_output_dir(dir, outcome.result.as_ref()), include_dir.to_owned()]);
		drop(outcome);
		watcher.extra_files = rotor_compile::macro_inputs::macro_watch_files(ts_config_path);
		for (path, stamp) in snapshot_files(&watcher.extra_files) {
			baseline.entry(path).or_insert(stamp);
		}
		prune_stamps(&mut baseline, watcher.skip_dirs());
		let count = baseline.len();
		reporter.idle(&|| count);
		if changed.is_none() {
			reporter.watching(&|| count);
		}
		let mut interval = MIN_INTERVAL;
		let watcher = std::cell::RefCell::new(&mut watcher);
		let next = await_changes(
			stop,
			&mut || {
				interval = watcher.borrow().interval();
				interval
			},
			&mut || watcher.borrow_mut().snapshot(),
			baseline,
		);
		let Some((next, paths)) = next else {
			return;
		};
		baseline = next;
		changed = Some(paths);
	}
}

/// `runCheckWatchLoop`: checks, then re-checks on every settled change to the watched files until
/// `stop`. Runs on a large stack ([`rotor_tsrs::with_large_stack`]): every check runs on this
/// thread.
pub fn run_check_watch_loop(
	stop: &Stop,
	dir: &str,
	checkers: Option<i64>,
	reporter: &mut (dyn CheckWatchReporter + Send),
) {
	let mut changed: Option<Vec<String>> = None;
	let mut stamps = Stamps::new();
	loop {
		reporter.build_start(changed.as_deref());
		let pass = reporter
			.run(&|render, progress| check_pass_in(dir, checkers, render, progress, &mut std::io::stderr(), true));
		// New files are stamped now; surviving files keep their pre-check stamps, so an edit
		// made while the check ran is still seen. Stamping before `build_end` means an edit made
		// after a consumer sees it is never absorbed.
		let files = pass.watch_files.clone();
		stamps = merge_pre_stamps(snapshot_files(&files), &stamps);
		reporter.build_end(&pass);
		drop(pass);
		crate::memory::trim();
		if changed.is_none() {
			reporter.watching(&|| files.len());
		}
		// Only the file list is kept, so an idle watcher holds no diagnostics.
		let next = await_changes(stop, &mut || MIN_INTERVAL, &mut || snapshot_files(&files), stamps);
		let Some((next, paths)) = next else {
			return;
		};
		stamps = next;
		changed = Some(paths);
	}
}

/// Ends a watch session on Ctrl+C (or SIGTERM, SIGHUP) with exit code 130, as the shell reports
/// Go's default signal exit. `clear_line` first clears a progress line that may be on screen, so
/// the prompt comes back on a clean line. Nothing else needs restoring: a session never changes
/// the terminal's modes.
pub fn exit_on_interrupt(clear_line: bool) {
	let _ = ctrlc::set_handler(move || {
		let mut err = std::io::stderr();
		if clear_line {
			let _ = err.write_all(b"\r\x1b[2K");
		}
		let _ = err.flush();
		let _ = std::io::stdout().flush();
		std::process::exit(130);
	});
}

/// The clock of a rebuild divider.
fn clock() -> String {
	jiff::Zoned::now().strftime("%H:%M:%S").to_string()
}

/// The width rules are drawn to.
fn rule_width() -> usize {
	terminal_size::terminal_size_of(std::io::stdout())
		.map_or(80, |(terminal_size::Width(width), _)| usize::from(width))
		.clamp(40, 96)
}

/// `watchChanges`: the timestamped divider of a rebuild, naming up to three changed files.
#[must_use]
pub fn divider(paint: Paint, changed: &[String], time: &str, width: usize) -> String {
	let mut names: Vec<String> = changed.iter().take(3).map(|path| gopath::base(path)).collect();
	if changed.len() > names.len() {
		names.push(format!("+{} more", changed.len() - names.len()));
	}
	let count = if changed.len() == 1 {
		"1 file changed".to_owned()
	} else {
		format!("{} files changed", ui::thousands(changed.len()))
	};
	let head = format!(
		"{} {time} {} {}  {} ",
		paint.muted("──"),
		paint.muted("·"),
		paint.cyan_plain(&count),
		names.join(", ")
	);
	let fill = width.saturating_sub(ui::visible_width(&head)).max(2);
	format!("{head}{}", paint.muted(&"─".repeat(fill)))
}

/// The idle line after a build: `Watching N files` (with the sparkline of recent build times on
/// a terminal) or, after a failure, `Waiting` with the error count.
#[must_use]
pub fn idle_line(paint: Paint, stats: &WatchStats, files: usize) -> String {
	let watched = format!(
		"{} {}",
		paint.bold(&ui::thousands(files)),
		if files == 1 { "file" } else { "files" }
	);
	let stop = paint.muted("Ctrl+C to stop");
	if stats.last_failed {
		let parts = [
			paint.red_plain(&ui::plural(stats.last_errors.max(1), "error")),
			paint.muted(&format!("watching {}", ui::plural(files, "file"))),
			stop,
		];
		return format!(
			"{} {}",
			ui::verb(paint, Tone::Failure, "Waiting"),
			parts.join(&paint.dot())
		);
	}
	let mut parts = vec![watched];
	if paint.color && stats.history.len() > 1 {
		let spark = spark(&stats.history);
		parts.push(format!(
			"{} {}",
			paint.muted(&format!("last {}", stats.history.len())),
			paint.cyan_plain(&spark)
		));
	}
	parts.push(stop);
	format!(
		"{} {}",
		ui::verb(paint, Tone::Info, "Watching"),
		parts.join(&paint.dot())
	)
}

/// `Styler.Spark`: each time on the ramp, scaled to the slowest.
fn spark(history: &[Duration]) -> String {
	const RAMP: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
	let max = history.iter().max().copied().unwrap_or_default();
	history
		.iter()
		.map(|elapsed| {
			if max.is_zero() || elapsed.is_zero() {
				return RAMP[0];
			}
			#[expect(
				clippy::cast_possible_truncation,
				clippy::cast_sign_loss,
				reason = "a rounded index into an eight-step ramp"
			)]
			let index = (elapsed.as_secs_f64() / max.as_secs_f64()).mul_add(7.0, 0.5) as usize;
			RAMP[index.min(7)]
		})
		.collect()
}

/// `clearForRebuild`: wipes the screen and scrollback before a rebuild, on a coloured terminal.
pub fn clear_for_rebuild(out: &mut dyn std::io::Write, paint: Paint, stats: &WatchStats) {
	if stats.clear_screen && paint.color {
		let _ = out.write_all(b"\x1b[2J\x1b[3J\x1b[H");
	}
}

/// Rings the bell on a fail/pass flip (not on the initial build), when asked and stderr is a
/// terminal. Records the outcome either way.
fn flip(stats: &mut WatchStats, failed: bool, errors: usize, err: &mut dyn std::io::Write, err_terminal: bool) {
	if stats.builds > 1 && failed != stats.last_failed && stats.bell && err_terminal {
		let _ = err.write_all(b"\x07");
		let _ = err.flush();
	}
	stats.last_failed = failed;
	stats.last_errors = if failed { errors.max(1) } else { 0 };
}

/// The styled `build --watch` (and `--build --watch`) reporter. The initial build prints the
/// one-shot result block; a rebuild prints its divider, then one result line, and every build ends
/// with the idle line.
pub struct StyledBuildReporter<'a, 'b> {
	streams: &'a mut Streams<'b>,
	stats: WatchStats,
	config_path: String,
	progress: Arc<BuildProgress>,
	err_terminal: bool,
}

impl<'a, 'b> StyledBuildReporter<'a, 'b> {
	/// A reporter for `config_path`'s session.
	pub fn new(streams: &'a mut Streams<'b>, config_path: &str, options: WatchOptions) -> Self {
		Self {
			streams,
			stats: WatchStats {
				max_errors: options.max_errors,
				bell: options.bell,
				clear_screen: options.clear_screen,
				..WatchStats::default()
			},
			config_path: config_path.to_owned(),
			progress: Arc::new(BuildProgress::new(false)),
			err_terminal: crate::term::process_terminals().1,
		}
	}

	fn rebuild_line(&mut self, outcome: &Outcome) {
		let out = self.streams.out_paint();
		let rebuild = self.stats.builds - 1;
		let result = outcome.result.clone().unwrap_or_default();
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
		let mut parts = vec![
			format!(
				"{} in {}",
				out.bold(&ui::plural(written, "file")),
				out.bold(&ui::duration(outcome.elapsed))
			),
			format!("{} written", out.bold(&ui::thousands(written))),
		];
		// An incremental rebuild's outputs are only what it compiled, so this can be 0.
		let unchanged = total.saturating_sub(written);
		if unchanged > 0 {
			parts.push(format!("{} unchanged", ui::thousands(unchanged)));
		}
		let (_, warnings) = frames::tally(&report::outcome_frames(outcome));
		parts.extend(ui::counts(out, 0, warnings));
		parts.push(out.muted(&format!("rebuild #{rebuild}")));
		let _ = writeln!(self.streams.out);
		ui::verb_line(
			self.streams.out,
			out,
			Tone::Success,
			"Rebuilt",
			&ui::summary_parts(out, &parts),
		);
	}

	fn failure_block(&mut self, outcome: &Outcome) -> usize {
		let err = self.streams.err_paint();
		let frames = report::outcome_frames(outcome);
		let (errors, warnings) = frames::tally(&frames);
		let mut text = String::from("\n");
		frames::render_all(&mut text, err, &frames, self.stats.max_errors);
		let _ = self.streams.err.write_all(text.as_bytes());
		let mut parts = vec![format!(
			"{} in {}",
			err.bold(&format!("rebuild #{}", self.stats.builds - 1)),
			err.bold(&ui::duration(outcome.elapsed))
		)];
		parts.extend(ui::counts(err, errors, warnings));
		let located = frames::files(&frames);
		if located > 0 {
			parts.push(err.muted(&ui::plural(located, "file")));
		}
		ui::verb_line(
			self.streams.err,
			err,
			Tone::Failure,
			"Failed",
			&ui::summary_parts(err, &parts),
		);
		let _ = self.streams.err.flush();
		errors
	}
}

impl BuildWatchReporter for StyledBuildReporter<'_, '_> {
	fn build_start(&mut self, changed: Option<&[String]>) {
		let Some(changed) = changed else {
			return;
		};
		let out = self.streams.out_paint();
		clear_for_rebuild(self.streams.out, out, &self.stats);
		let line = divider(out, changed, &clock(), rule_width());
		let _ = write!(self.streams.out, "\n{line}\n");
		let _ = self.streams.out.flush();
	}

	fn run(&mut self, options: &ProjectOptions, build: &dyn Fn(&ProjectOptions) -> Outcome) -> Outcome {
		self.progress = Arc::new(BuildProgress::new(false));
		let mut options = options.clone();
		options.progress = Some(Arc::clone(&self.progress));
		let view = live::View {
			progress_line: live::progress_line_allowed(self.streams),
			project_lines: false,
			solution: false,
			out: self.streams.out_paint(),
		};
		let progress = Arc::clone(&self.progress);
		live::run(self.streams, &progress, view, || build(&options))
	}

	fn build_end(&mut self, outcome: &Outcome) {
		self.stats.record(outcome.elapsed);
		let failed = outcome.error.is_some();
		let initial = self.stats.builds == 1;
		let errors = if initial {
			let shown = report::Shown {
				config_path: &self.config_path,
				solution: false,
				max_errors: self.stats.max_errors,
			};
			report::build(self.streams, &shown, outcome, &self.progress);
			frames::tally(&report::outcome_frames(outcome)).0
		} else if failed {
			self.failure_block(outcome)
		} else {
			self.rebuild_line(outcome);
			0
		};
		flip(&mut self.stats, failed, errors, self.streams.err, self.err_terminal);
	}

	fn idle(&mut self, files: &dyn Fn() -> usize) {
		let out = self.streams.out_paint();
		let line = idle_line(out, &self.stats, files());
		let _ = writeln!(self.streams.out, "{line}");
		let _ = self.streams.out.flush();
	}

	fn watching(&mut self, _files: &dyn Fn() -> usize) {}
}

/// The styled `check --watch` reporter.
pub struct StyledCheckReporter<'a, 'b> {
	streams: &'a mut Streams<'b>,
	stats: WatchStats,
	name: String,
	err_terminal: bool,
}

impl<'a, 'b> StyledCheckReporter<'a, 'b> {
	/// A reporter for the project at `dir`.
	pub fn new(streams: &'a mut Streams<'b>, dir: &str) -> Self {
		Self {
			streams,
			stats: WatchStats::default(),
			name: ui::project_name(&gopath::join(&[dir, "tsconfig.json"])),
			err_terminal: crate::term::process_terminals().1,
		}
	}
}

impl CheckWatchReporter for StyledCheckReporter<'_, '_> {
	fn build_start(&mut self, changed: Option<&[String]>) {
		if let Some(changed) = changed {
			let out = self.streams.out_paint();
			let line = divider(out, changed, &clock(), rule_width());
			let _ = write!(self.streams.out, "\n{line}\n");
			let _ = self.streams.out.flush();
		}
	}

	fn run(&mut self, check: &dyn Fn(Render, Option<&BuildProgress>) -> CheckPass) -> CheckPass {
		let progress = Arc::new(BuildProgress::new(false));
		let view = live::View {
			progress_line: live::progress_line_allowed(self.streams),
			project_lines: false,
			solution: false,
			out: self.streams.out_paint(),
		};
		live::run(self.streams, &progress, view, || check(Render::Frames, Some(&progress)))
	}

	fn build_end(&mut self, pass: &CheckPass) {
		self.stats.record(pass.elapsed);
		let out = self.streams.out_paint();
		let mut text = String::new();
		frames::render_all(&mut text, out, &pass.frames, 0);
		if !pass.frames.is_empty() {
			let _ = writeln!(self.streams.out);
		}
		let _ = self.streams.out.write_all(text.as_bytes());
		let (errors, warned) = frames::tally(&pass.frames);
		let mut parts = vec![
			out.bold(&self.name),
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
		if self.stats.builds > 1 {
			parts.push(out.muted(&format!("rebuild #{}", self.stats.builds - 1)));
		}
		let (tone, word) = if pass.errors > 0 {
			(Tone::Failure, "Failed")
		} else {
			(Tone::Success, "Checked")
		};
		// A rebuild's result sits a blank line below its divider; frames already end with one.
		if self.stats.builds > 1 && pass.frames.is_empty() {
			let _ = writeln!(self.streams.out);
		}
		ui::verb_line(self.streams.out, out, tone, word, &ui::summary_parts(out, &parts));
		flip(
			&mut self.stats,
			pass.errors > 0,
			pass.errors,
			self.streams.err,
			self.err_terminal,
		);
		let line = idle_line(out, &self.stats, pass.watch_files.len());
		let _ = writeln!(self.streams.out, "{line}");
		let _ = self.streams.out.flush();
	}

	fn watching(&mut self, _files: &dyn Fn() -> usize) {}
}

#[cfg(test)]
mod tests {
	//! `watch_test.go` and `watch_flamework_test.go`

	use std::path::Path;
	use std::time::Duration;

	use super::{
		FileStamp, MAX_INTERVAL, MIN_INTERVAL, QUIET_PERIOD, SETTLE_CAP, Stamps, TreeWatcher, WatchStats, diff_stamps,
		is_junk_file, merge_pre_stamps, prune_stamps, settle_changes, should_prune_dir,
	};
	use crate::ui::Paint;

	fn write(path: &Path, content: &str) {
		std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
		std::fs::write(path, content).expect("write");
	}

	fn text(path: &Path) -> String {
		path.to_string_lossy().into_owned()
	}

	fn stamp(size: u64) -> FileStamp {
		FileStamp {
			exists: true,
			size,
			..FileStamp::default()
		}
	}

	fn stamps(entries: &[(&str, FileStamp)]) -> Stamps {
		entries
			.iter()
			.map(|(path, stamp)| ((*path).to_owned(), *stamp))
			.collect()
	}

	#[test]
	fn tree_watcher_snapshot_prunes_and_detects_added_files() {
		let dir = tempfile::tempdir().expect("temp dir");
		let root = dir.path();
		let src_file = root.join("src/main.ts");
		write(&src_file, "export {};\n");
		write(&root.join("out/main.luau"), "-- generated\n");
		write(&root.join("include/RuntimeLib.luau"), "-- runtime\n");
		write(&root.join("node_modules/@rbxts/types/package.json"), "{}\n");
		write(&root.join(".git/HEAD"), "ref: refs/heads/master\n");
		write(&root.join(".vscode/settings.json"), "{}\n");
		write(&root.join("src/main.ts.swp"), "vim\n");
		write(&root.join("package.json"), "{}\n");

		let mut watcher = TreeWatcher::new(&text(root), &[text(&root.join("out")), text(&root.join("include"))]);
		let before = watcher.snapshot();
		for (path, want) in [
			(src_file, true),
			(root.join("package.json"), true),
			(root.join("out/main.luau"), false),
			(root.join("include/RuntimeLib.luau"), false),
			(root.join("node_modules/@rbxts/types/package.json"), false),
			(root.join(".git/HEAD"), false),
			(root.join(".vscode/settings.json"), false),
			(root.join("src/main.ts.swp"), false),
		] {
			assert_eq!(before.contains_key(&text(&path)), want, "snapshot[{}]", path.display());
		}

		let added = root.join("src/extra.json");
		write(&added, "{}\n");
		assert_eq!(diff_stamps(&before, &watcher.snapshot()), [text(&added)]);
	}

	#[test]
	fn tree_watcher_snapshot_ignores_flamework_build() {
		let dir = tempfile::tempdir().expect("temp dir");
		let root = dir.path();
		let source = root.join("src/main.ts");
		let artifact = root.join("flamework.build");
		write(&source, "export {}\n");
		write(&artifact, "{\"version\":1}\n");
		let mut watcher = TreeWatcher::new(&text(root), &[]);
		let before = watcher.snapshot();
		write(&artifact, "{\"version\":2}\n");
		write(&source, "export const value = 1\n");
		assert_eq!(diff_stamps(&before, &watcher.snapshot()), [text(&source)]);
	}

	#[test]
	fn diff_stamps_reports_removed_and_modified_files() {
		let dir = tempfile::tempdir().expect("temp dir");
		let root = dir.path();
		let gone = root.join("src/gone.ts");
		let edited = root.join("src/edited.ts");
		write(&gone, "export {};\n");
		write(&edited, "export {};\n");
		let mut watcher = TreeWatcher::new(&text(root), &[]);
		let before = watcher.snapshot();
		std::fs::remove_file(&gone).expect("remove");
		write(&edited, "export {}; // edited\n");
		assert_eq!(diff_stamps(&before, &watcher.snapshot()), [text(&edited), text(&gone)]);
	}

	#[test]
	fn junk_files() {
		for (name, want) in [
			("main.ts", false),
			("default.project.json", false),
			("main.ts.swp", true),
			("main.ts.swo", true),
			("scratch.tmp", true),
			("main.ts~", true),
			("4913", true),
			(".#main.ts", true),
			(".DS_Store", true),
			("Thumbs.db", true),
		] {
			assert_eq!(is_junk_file(name), want, "{name}");
		}
	}

	#[test]
	fn pruned_dirs() {
		for (name, want) in [
			("src", false),
			("server", false),
			("node_modules", true),
			(".git", true),
			(".vscode", true),
		] {
			assert_eq!(should_prune_dir(name), want, "{name}");
		}
	}

	/// An editor's "save all": the first tick sees one file, two more land while settling, and the
	/// settled batch reports all three.
	#[test]
	fn settle_changes_batches_write_bursts() {
		let (a, b) = (stamp(1), stamp(2));
		let base = stamps(&[("a.ts", a), ("b.ts", a), ("c.ts", a)]);
		let first = stamps(&[("a.ts", b), ("b.ts", a), ("c.ts", a)]);
		let burst = [
			stamps(&[("a.ts", b), ("b.ts", b), ("c.ts", a)]),
			stamps(&[("a.ts", b), ("b.ts", b), ("c.ts", b)]),
			stamps(&[("a.ts", b), ("b.ts", b), ("c.ts", b)]),
			stamps(&[("a.ts", b), ("b.ts", b), ("c.ts", b)]),
		];
		let mut calls = 0;
		let (settled, changed) = settle_changes(
			&base,
			first,
			&mut || {
				let snapshot = burst[calls].clone();
				calls += 1;
				snapshot
			},
			&mut |_| {},
		);
		assert_eq!(changed, ["a.ts", "b.ts", "c.ts"]);
		assert_eq!(calls, 3, "stop on the first quiet tick");
		assert_eq!(settled, burst[2]);
	}

	/// The settle loop gives up after its cap even when every snapshot differs.
	#[test]
	fn settle_changes_caps_runaway_churn() {
		let mut n = 0;
		let mut snap = || {
			n += 1;
			stamps(&[("a.ts", stamp(n))])
		};
		let first = snap();
		let (_, changed) = settle_changes(&Stamps::new(), first, &mut snap, &mut |_| {});
		assert_eq!(changed.len(), 1);
		let max_ticks = u64::try_from(SETTLE_CAP.as_millis() / QUIET_PERIOD.as_millis()).expect("ticks");
		assert!(n <= max_ticks + 1, "snapshot calls = {n}, want <= {}", max_ticks + 1);
	}

	#[test]
	fn merge_pre_stamps_keeps_pre_build_stamps_for_survivors() {
		let (pre, post) = (stamp(1), stamp(2));
		let merged = merge_pre_stamps(
			stamps(&[("a.ts", post), ("new.ts", post)]),
			&stamps(&[("a.ts", pre), ("gone.ts", pre)]),
		);
		assert_eq!(merged["a.ts"], pre);
		assert_eq!(merged["new.ts"], post);
		assert!(!merged.contains_key("gone.ts"), "gone.ts must not be resurrected");
	}

	#[test]
	fn prune_stamps_drops_entries_under_skip_dirs() {
		let sep = std::path::MAIN_SEPARATOR;
		let path = |parts: &[&str]| parts.join(&sep.to_string());
		let out = path(&["proj", "dist"]);
		let mut entries = stamps(&[
			(&path(&["proj", "src", "a.ts"]), stamp(0)),
			(&path(&["proj", "dist", "a.luau"]), stamp(0)),
			(&path(&["proj", "dist", "sub", "b.luau"]), stamp(0)),
			(&path(&["proj", "distance.ts"]), stamp(0)),
			(&path(&["proj", "include", "r.lu"]), stamp(0)),
		]);
		prune_stamps(&mut entries, &[out, path(&["proj", "include"])]);
		let mut kept: Vec<&String> = entries.keys().collect();
		kept.sort();
		assert_eq!(kept, [&path(&["proj", "distance.ts"]), &path(&["proj", "src", "a.ts"])]);
	}

	#[test]
	fn tree_watcher_interval_clamps() {
		let dir = tempfile::tempdir().expect("temp dir");
		let mut watcher = TreeWatcher::new(&text(dir.path()), &[]);
		for (cost, want) in [
			(Duration::ZERO, MIN_INTERVAL),
			(Duration::from_millis(5), MIN_INTERVAL),
			(Duration::from_millis(30), Duration::from_millis(300)),
			(Duration::from_secs(1), MAX_INTERVAL),
		] {
			watcher.set_walk_cost(cost);
			assert_eq!(watcher.interval(), want, "{cost:?}");
		}
	}

	#[test]
	fn watch_stats_record_caps_history() {
		let mut stats = WatchStats::default();
		for index in 0..super::HISTORY_LEN + 5 {
			stats.record(Duration::from_millis(index as u64));
		}
		assert_eq!(stats.builds, super::HISTORY_LEN + 5);
		assert_eq!(stats.history.len(), super::HISTORY_LEN);
		assert_eq!(
			stats.history.last(),
			Some(&Duration::from_millis((super::HISTORY_LEN + 4) as u64))
		);
	}

	/// `TestReportBuildPassPersistentErrorBanner`: a failed build leaves the idle line in an error
	/// state with its count, and the next success clears it.
	#[test]
	fn idle_line_keeps_the_error_state_until_a_pass() {
		let plain = Paint { color: false };
		let mut stats = WatchStats::default();
		stats.record(Duration::from_millis(50));
		let mut sink = Vec::new();
		super::flip(&mut stats, true, 1, &mut sink, false);
		let line = super::idle_line(plain, &stats, 12);
		assert!(line.contains("Waiting") && line.contains("1 error"), "{line}");
		assert!(stats.last_failed);
		stats.record(Duration::from_millis(40));
		super::flip(&mut stats, false, 0, &mut sink, false);
		assert!(!stats.last_failed);
		assert_eq!(stats.last_errors, 0);
		let line = super::idle_line(plain, &stats, 12);
		assert_eq!(line, "    Watching 12 files  ·  Ctrl+C to stop");
		assert!(sink.is_empty(), "no bell without --bell");
	}

	/// `TestClearForRebuild`
	#[test]
	fn clear_for_rebuild_needs_the_flag_and_colour() {
		let enabled = WatchStats {
			clear_screen: true,
			..WatchStats::default()
		};
		let mut out = Vec::new();
		super::clear_for_rebuild(&mut out, Paint { color: true }, &enabled);
		assert!(String::from_utf8_lossy(&out).contains("\x1b[2J"));
		let mut out = Vec::new();
		super::clear_for_rebuild(&mut out, Paint { color: true }, &WatchStats::default());
		assert!(out.is_empty(), "--no-clear");
		let mut out = Vec::new();
		super::clear_for_rebuild(&mut out, Paint { color: false }, &enabled);
		assert!(out.is_empty(), "not interactive");
	}

	#[test]
	fn bell_rings_only_on_a_flip_after_the_first_build() {
		let mut stats = WatchStats {
			bell: true,
			..WatchStats::default()
		};
		let mut sink = Vec::new();
		stats.record(Duration::ZERO);
		super::flip(&mut stats, true, 1, &mut sink, true);
		assert!(sink.is_empty(), "the initial build has nothing to flip from");
		stats.record(Duration::ZERO);
		super::flip(&mut stats, true, 1, &mut sink, true);
		assert!(sink.is_empty(), "still failing");
		stats.record(Duration::ZERO);
		super::flip(&mut stats, false, 0, &mut sink, true);
		assert_eq!(sink, b"\x07");
		stats.record(Duration::ZERO);
		super::flip(&mut stats, true, 1, &mut sink, false);
		assert_eq!(sink, b"\x07", "stderr is not a terminal");
	}

	#[test]
	fn divider_names_three_files_then_counts_the_rest() {
		let plain = Paint { color: false };
		let changed: Vec<String> = ["src/a/Hud.tsx", "src/types.ts", "c.ts", "d.ts", "e.ts"]
			.iter()
			.map(|path| (*path).to_owned())
			.collect();
		let line = super::divider(plain, &changed, "14:02:31", 80);
		assert!(
			line.starts_with("── 14:02:31 · 5 files changed  Hud.tsx, types.ts, c.ts, +2 more ─"),
			"{line}"
		);
		assert_eq!(crate::ui::visible_width(&line), 80);
		// A head wider than the rule still ends in a short rule.
		let line = super::divider(plain, &changed, "14:02:31", 40);
		assert!(line.ends_with(" ──"), "{line}");
		let line = super::divider(plain, &changed[..1], "14:03:05", 40);
		assert!(line.starts_with("── 14:03:05 · 1 file changed  Hud.tsx ─"), "{line}");
	}

	/// `TestTreeWatcherSnapshotWatchesFlameworkInputsExactlyOnce`: the Flamework inputs form one
	/// settled batch, and the generated `flamework.build` stays out of it.
	#[test]
	fn tree_watcher_snapshot_watches_flamework_inputs_exactly_once() {
		let dir = tempfile::tempdir().expect("temp dir");
		let root = dir.path();
		for (relative, contents) in [
			("rotor.toml", "[flamework]\nidGenerationMode = \"full\"\n"),
			("flamework.json", "{}\n"),
			("package.json", "{\"name\":\"fixture\"}\n"),
			("flamework.build.backup", "keep watching this\n"),
			("flamework.build", "{\"version\":1}\n"),
			("src/server/main.ts", "export {}\n"),
			("node_modules/pkg/package.json", "{}\n"),
		] {
			write(&root.join(relative), contents);
		}
		let mut watcher = TreeWatcher::new(&text(root), &[]);
		let baseline = watcher.snapshot();
		assert!(
			!baseline
				.get(&text(&root.join("flamework.build")))
				.is_some_and(|stamp| stamp.exists),
			"the generated artifact entered the baseline"
		);
		assert_eq!(
			"[flamework]\nidGenerationMode = \"full\"\n".len(),
			"[flamework]\nidGenerationMode = \"tiny\"\n".len(),
			"the configuration edits must keep the size"
		);
		write(&root.join("rotor.toml"), "[flamework]\nidGenerationMode = \"tiny\"\n");
		write(&root.join("package.json"), "{\"name\":\"fixture\",\"version\":\"2\"}\n");
		write(&root.join("flamework.build.backup"), "changed input\n");
		write(&root.join("flamework.build"), "{\"version\":2}\n");
		let first = watcher.snapshot();
		let watcher = std::cell::RefCell::new(watcher);
		let (_, changed) = settle_changes(&baseline, first, &mut || watcher.borrow_mut().snapshot(), &mut |_| {});
		assert_eq!(
			changed,
			[
				text(&root.join("flamework.build.backup")),
				text(&root.join("package.json")),
				text(&root.join("rotor.toml")),
			]
		);
	}
}
