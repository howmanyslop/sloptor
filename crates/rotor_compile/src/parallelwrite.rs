//! `parallelwrite.go`: the write worker pool. Output writes are independent file I/O, so they fan
//! out across workers (Windows writes are far slower per syscall than macOS/APFS).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::compile::Error;

/// `writeConcurrencyCap`
pub(crate) const WRITE_CONCURRENCY_CAP: usize = 256;
/// `writeConcurrencyDefault`
pub(crate) const WRITE_CONCURRENCY_DEFAULT: usize = 8;
/// `writeSyncCutoff`: fewer jobs than this run sequentially.
pub(crate) const WRITE_SYNC_CUTOFF: usize = 16;

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

/// Runs `work` over `items` using up to `workers` worker threads when `items.len() >= cutoff`,
/// accumulating into per-worker `A` values and returning them without per-item heap or mutex
/// overhead.
pub(crate) fn parallel_fold_with_cutoff<T: Sync, A: Default + Send>(
	workers: usize,
	cutoff: usize,
	items: &[T],
	work: impl Fn(&mut A, &T) -> Result<(), Error> + Sync,
) -> Result<Vec<A>, Error> {
	let workers = workers.min(WRITE_CONCURRENCY_CAP);
	if workers <= 1 || items.len() < cutoff || items.len() <= 1 {
		let mut acc = A::default();
		for item in items {
			work(&mut acc, item)?;
		}
		return Ok(vec![acc]);
	}
	let worker_count = workers.min(items.len());
	let next = AtomicUsize::new(0);
	let failed = AtomicBool::new(false);
	let first: Mutex<Option<Error>> = Mutex::new(None);
	let run_worker = || {
		let mut acc = A::default();
		loop {
			if failed.load(Ordering::Relaxed) {
				break;
			}
			let index = next.fetch_add(1, Ordering::Relaxed);
			let Some(item) = items.get(index) else {
				break;
			};
			if let Err(error) = work(&mut acc, item) {
				failed.store(true, Ordering::Relaxed);
				let mut guard = first.lock().unwrap_or_else(PoisonError::into_inner);
				if guard.is_none() {
					*guard = Some(error);
				}
				drop(guard);
				break;
			}
		}
		acc
	};
	let results = rotor_tsrs::threads::scope_io(|scope| {
		let handles: Vec<_> = (1..worker_count).map(|_| scope.spawn(run_worker)).collect();
		let mut results = Vec::with_capacity(worker_count);
		results.push(run_worker());
		for handle in handles {
			results.push(
				handle
					.join()
					.unwrap_or_else(|payload| std::panic::resume_unwind(payload)),
			);
		}
		results
	});
	if let Some(error) = first.into_inner().unwrap_or_else(PoisonError::into_inner) {
		return Err(error);
	}
	Ok(results)
}
