//! `parallelwrite.go`: the write worker pool. Output writes are independent file I/O, so they fan
//! out across workers (Windows writes are far slower per syscall than macOS/APFS).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::compile::Error;

/// `writeConcurrencyCap`
pub(crate) const WRITE_CONCURRENCY_CAP: usize = 256;
/// `writeConcurrencyDefault`
pub(crate) const WRITE_CONCURRENCY_DEFAULT: usize = 8;
/// `writeSyncCutoff`: fewer jobs than this run sequentially.
pub(crate) const WRITE_SYNC_CUTOFF: usize = 16;

/// One write job.
pub(crate) type Job<'a> = Box<dyn FnOnce() -> Result<(), Error> + Send + 'a>;

/// `writeWorkers`: `RBXTSC_WRITE_CONCURRENCY`, then `ROTOR_WRITE_WORKERS`, then 8.
pub(crate) fn write_workers() -> usize {
	write_workers_from(|name| std::env::var(name).ok())
}

/// [`write_workers`] over an environment lookup.
pub(crate) fn write_workers_from(lookup: impl Fn(&str) -> Option<String>) -> usize {
	for name in ["RBXTSC_WRITE_CONCURRENCY", "ROTOR_WRITE_WORKERS"] {
		if let Some(workers) = lookup(name).and_then(|raw| parse_write_concurrency(&raw)) {
			return workers;
		}
	}
	WRITE_CONCURRENCY_DEFAULT
}

/// `parseWriteConcurrency`: a positive finite number, floored, capped at 256.
pub(crate) fn parse_write_concurrency(raw: &str) -> Option<usize> {
	if raw.is_empty() {
		return None;
	}
	let value: f64 = parse_go_float(raw)?;
	if value <= 0.0 || !value.is_finite() {
		return None;
	}
	let value = value.floor();
	if value < 1.0 {
		return None;
	}
	if value >= 256.0 {
		return Some(WRITE_CONCURRENCY_CAP);
	}
	#[expect(
		clippy::cast_possible_truncation,
		clippy::cast_sign_loss,
		reason = "the value is a floored number in 1..256"
	)]
	Some(value as usize)
}

/// `strconv.ParseFloat(raw, 64)`: Rust's float grammar plus Go's `0x` hex floats and `_`
/// separators are rare enough in an environment variable that the shared decimal grammar is used;
/// Go's `Inf`/`NaN` spellings parse and are then rejected as non-finite.
fn parse_go_float(raw: &str) -> Option<f64> {
	raw.parse::<f64>().ok()
}

/// `parallelize`: runs `jobs` on up to `workers` workers. After the first error the queued jobs
/// are skipped; running ones finish. Returns the first error.
pub(crate) fn parallelize(workers: usize, jobs: Vec<Job<'_>>) -> Result<(), Error> {
	let workers = workers.min(WRITE_CONCURRENCY_CAP);
	if workers <= 1 || jobs.len() < WRITE_SYNC_CUTOFF {
		for job in jobs {
			job()?;
		}
		return Ok(());
	}
	let queue: Vec<Mutex<Option<Job<'_>>>> = jobs.into_iter().map(|job| Mutex::new(Some(job))).collect();
	let next = AtomicUsize::new(0);
	let first: Mutex<Option<Error>> = Mutex::new(None);
	std::thread::scope(|scope| {
		for _ in 0..workers {
			scope.spawn(|| {
				loop {
					let index = next.fetch_add(1, Ordering::Relaxed);
					let Some(slot) = queue.get(index) else {
						return;
					};
					let Some(job) = slot.lock().unwrap_or_else(PoisonError::into_inner).take() else {
						continue;
					};
					if first.lock().unwrap_or_else(PoisonError::into_inner).is_some() {
						continue;
					}
					if let Err(error) = job() {
						let mut first = first.lock().unwrap_or_else(PoisonError::into_inner);
						if first.is_none() {
							*first = Some(error);
						}
					}
				}
			});
		}
	});
	first
		.into_inner()
		.unwrap_or_else(PoisonError::into_inner)
		.map_or(Ok(()), Err)
}
