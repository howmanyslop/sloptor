//! The runtime policy (`cmd/rotor/runtime_policy*.go`, `internal/admission`), mapped to Rust.
//!
//! Go sizes `GOMAXPROCS`, `GOGC` and `GOMEMLIMIT` per build and holds a machine-wide build slot, so
//! parallel builds (an nx run, a CI matrix) split the machine instead of each claiming all of it.
//! Rust has no garbage collector, so the slot share sizes the one knob that matters: the tsrs
//! worker pool (`RAYON_NUM_THREADS`). The slot share also reaches the Node
//! sidecar's shard count, as in Go.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Overrides the slot count; 0 disables admission (`admission.MaxBuildsEnv`).
pub const MAX_BUILDS_ENV: &str = "ROTOR_MAX_BUILDS";

const GIB: u64 = 1 << 30;
const SLOT_MEMORY: u64 = 6 * GIB;
const FALLBACK_MEMORY: u64 = 8 * GIB;
const POLL: Duration = Duration::from_millis(100);
const WAIT_NOTICE: Duration = Duration::from_secs(1);

/// `DefaultSlots`: one build per 6 GiB of memory, at most one per two CPUs.
#[must_use]
pub fn default_slots(memory: u64, cpus: usize) -> usize {
	usize::try_from(memory / SLOT_MEMORY)
		.unwrap_or(usize::MAX)
		.min(cpus / 2)
		.max(1)
}

/// `Slots`: [`default_slots`] unless `ROTOR_MAX_BUILDS` names a count; 0 disables admission.
#[must_use]
pub fn slots(memory: u64, cpus: usize) -> usize {
	std::env::var(MAX_BUILDS_ENV)
		.ok()
		.and_then(|raw| raw.parse::<usize>().ok())
		.unwrap_or_else(|| default_slots(memory, cpus))
}

/// The worker threads one of `share` concurrent builds gets: `GOMAXPROCS`'s `max(2, cpus/share)`.
#[must_use]
pub fn worker_threads(cpus: usize, share: usize) -> usize {
	(cpus / share.max(1)).max(2)
}

/// `cgroupMemoryLimitPaths`: the cgroup v1 and v2 limit files `/proc/self/cgroup` names.
#[cfg(any(target_os = "linux", test))]
#[must_use]
pub fn cgroup_memory_limit_paths(cgroups: &str) -> Vec<String> {
	// Linux paths, whichever host runs the test.
	use rotor_support::gopath::Flavor;

	let mut paths = Vec::new();
	for line in cgroups.lines() {
		let fields: Vec<&str> = line.trim().splitn(3, ':').collect();
		let [hierarchy, controllers, path] = fields[..] else {
			continue;
		};
		let relative = Flavor::Unix.clean(path).trim_start_matches('/').to_owned();
		if hierarchy == "0" && controllers.is_empty() {
			paths.push(Flavor::Unix.join(&["/sys/fs/cgroup", &relative, "memory.max"]));
			continue;
		}
		if controllers.split(',').any(|controller| controller == "memory") {
			paths.push(Flavor::Unix.join(&["/sys/fs/cgroup/memory", &relative, "memory.limit_in_bytes"]));
			paths.push(Flavor::Unix.join(&["/sys/fs/cgroup", &relative, "memory.limit_in_bytes"]));
		}
	}
	paths
}

/// `effectiveMemory` on Linux: `MemTotal`, lowered by any cgroup limit.
#[cfg(target_os = "linux")]
fn effective_memory() -> u64 {
	let mut host = std::fs::read_to_string("/proc/meminfo")
		.ok()
		.and_then(|info| {
			info.lines()
				.find_map(|line| line.strip_prefix("MemTotal:"))
				.and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
		})
		.map_or(FALLBACK_MEMORY, |kib| kib * 1024);
	let mut paths = vec![
		"/sys/fs/cgroup/memory.max".to_owned(),
		"/sys/fs/cgroup/memory/memory.limit_in_bytes".to_owned(),
	];
	if let Ok(cgroups) = std::fs::read_to_string("/proc/self/cgroup") {
		paths.extend(cgroup_memory_limit_paths(&cgroups));
	}
	for path in paths {
		if let Some(limit) = std::fs::read_to_string(path)
			.ok()
			.and_then(|text| text.trim().parse::<u64>().ok())
			.filter(|&limit| limit > 0 && limit < host)
		{
			host = limit;
		}
	}
	host
}

/// `effectiveMemory` on macOS: `hw.memsize`.
#[cfg(target_os = "macos")]
#[expect(unsafe_code, reason = "sysctlbyname is the only source of the machine's memory size")]
fn effective_memory() -> u64 {
	let mut memory: u64 = 0;
	let mut size = std::mem::size_of::<u64>();
	// SAFETY: the name is NUL-terminated, and `memory` and `size` describe a writable u64.
	let status = unsafe {
		libc::sysctlbyname(
			c"hw.memsize".as_ptr(),
			(&raw mut memory).cast(),
			&raw mut size,
			std::ptr::null_mut(),
			0,
		)
	};
	if status == 0 && memory > 0 {
		memory
	} else {
		FALLBACK_MEMORY
	}
}

/// `effectiveMemory` elsewhere: Go's fallback.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const fn effective_memory() -> u64 {
	FALLBACK_MEMORY
}

/// `admission.Dir`: the per-user directory of the slot files.
#[must_use]
pub fn slot_dir() -> Option<PathBuf> {
	let cache = if cfg!(target_os = "macos") {
		std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"))
	} else if cfg!(windows) {
		std::env::var_os("LocalAppData").map(PathBuf::from)
	} else {
		std::env::var_os("XDG_CACHE_HOME")
			.filter(|dir| !dir.is_empty())
			.map(PathBuf::from)
			.or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
	}?;
	Some(cache.join("rotor").join("slots"))
}

/// One held build slot; dropping it frees the slot, and so does the process exiting.
#[derive(Debug)]
pub struct Lease {
	dir: PathBuf,
	slots: usize,
	index: usize,
	_file: File,
}

fn slot_path(dir: &Path, index: usize) -> PathBuf {
	dir.join(format!("slot-{index}.lock"))
}

/// The slot file locked, or `None` while another build holds it.
fn try_lock(path: &Path) -> std::io::Result<Option<File>> {
	let file = OpenOptions::new()
		.read(true)
		.write(true)
		.create(true)
		.truncate(false)
		.open(path)?;
	match file.try_lock() {
		Ok(()) => Ok(Some(file)),
		Err(TryLockError::WouldBlock) => Ok(None),
		Err(TryLockError::Error(error)) => Err(error),
	}
}

impl Lease {
	/// `Acquire`: takes a free slot in `dir`, waiting for one; `on_wait` runs once when the wait
	/// passes a second.
	///
	/// # Errors
	///
	/// The directory or a slot file cannot be opened.
	pub fn acquire(dir: &Path, slots: usize, on_wait: &mut dyn FnMut()) -> std::io::Result<Self> {
		std::fs::create_dir_all(dir)?;
		let started = Instant::now();
		let mut noticed = false;
		loop {
			for index in 0..slots {
				if let Some(file) = try_lock(&slot_path(dir, index))? {
					return Ok(Self {
						dir: dir.to_path_buf(),
						slots,
						index,
						_file: file,
					});
				}
			}
			if !noticed && started.elapsed() >= WAIT_NOTICE {
				noticed = true;
				on_wait();
			}
			std::thread::sleep(POLL);
		}
	}

	/// `Active`: the held slots, this one included.
	#[must_use]
	pub fn active(&self) -> usize {
		1 + (0..self.slots)
			.filter(|&index| index != self.index)
			.filter(|&index| matches!(try_lock(&slot_path(&self.dir, index)), Ok(None)))
			.count()
	}
}

/// Sets the tsrs worker pool's size. Must run before the pool starts and before any other thread
/// exists, which is why only [`admit`] calls it, first thing in a build.
#[expect(
	unsafe_code,
	reason = "the environment is written once, before the build starts any thread"
)]
fn set_worker_threads(threads: usize) {
	// SAFETY: called on the main thread before the build, its render thread or the tsrs worker
	// pool exist, so nothing reads the environment concurrently.
	unsafe { std::env::set_var("RAYON_NUM_THREADS", threads.to_string()) };
}

/// Sizes a one-shot build to the machine it shares (`admitBuild`): takes a build slot, then sizes
/// the worker pool by the share. A caller's `RAYON_NUM_THREADS` wins, then `GOMAXPROCS`. Admission
/// fails open: without a slot directory the build runs unshared. `notice` prints the wait notice;
/// `verbose` logs the slot.
pub fn admit(notice: &mut dyn FnMut(&str)) -> Option<Lease> {
	let memory = effective_memory();
	let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
	let slots = slots(memory, cpus);
	let lease = (slots > 0).then(slot_dir).flatten().and_then(|dir| {
		let message =
			format!("waiting for a build slot: {slots} builds already running (set {MAX_BUILDS_ENV} to change)");
		Lease::acquire(&dir, slots, &mut || notice(&message))
			.map_err(|error| {
				rotor_support::logservice::write_line_if_verbose(&[&format!("admission: disabled: {error}")]);
			})
			.ok()
	});
	let share = lease.as_ref().map_or(1, Lease::active);
	if let Some(lease) = &lease {
		rotor_support::logservice::write_line_if_verbose(&[&format!(
			"admission: slot {}/{}, sharing with {}",
			lease.index + 1,
			lease.slots,
			share - 1
		)]);
	}
	if std::env::var_os("RAYON_NUM_THREADS").is_none() {
		let threads = std::env::var("GOMAXPROCS")
			.ok()
			.and_then(|raw| raw.parse::<usize>().ok())
			.filter(|&threads| threads > 0)
			.unwrap_or_else(|| worker_threads(cpus, share));
		set_worker_threads(threads);
	}
	rotor_compile::sidecar_shards::set_build_share(i32::try_from(share).unwrap_or(i32::MAX));
	lease
}

#[cfg(test)]
mod tests {
	use super::{GIB, Lease, cgroup_memory_limit_paths, default_slots, worker_threads};

	#[test]
	fn slots_and_workers_split_the_machine() {
		assert_eq!(default_slots(64 * GIB, 16), 8);
		assert_eq!(default_slots(16 * GIB, 16), 2);
		assert_eq!(default_slots(4 * GIB, 1), 1);
		assert_eq!(worker_threads(16, 1), 16);
		assert_eq!(worker_threads(16, 4), 4);
		assert_eq!(worker_threads(16, 16), 2);
		assert_eq!(worker_threads(14, 0), 14);
	}

	#[test]
	fn cgroup_paths_cover_v1_and_v2() {
		let paths = cgroup_memory_limit_paths("0::/user.slice/app\n4:memory,cpu:/docker/abc\n2:cpu:/x\n");
		assert_eq!(
			paths,
			[
				"/sys/fs/cgroup/user.slice/app/memory.max",
				"/sys/fs/cgroup/memory/docker/abc/memory.limit_in_bytes",
				"/sys/fs/cgroup/docker/abc/memory.limit_in_bytes",
			]
		);
	}

	#[test]
	fn leases_hold_slots_until_dropped() {
		let dir = tempfile::tempdir().expect("temp dir");
		let first = Lease::acquire(dir.path(), 2, &mut || {}).expect("first slot");
		let second = Lease::acquire(dir.path(), 2, &mut || {}).expect("second slot");
		assert_ne!(first.index, second.index);
		assert_eq!(first.active(), 2);
		drop(second);
		assert_eq!(first.active(), 1);
	}
}
