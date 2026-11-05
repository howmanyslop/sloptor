//! `parallelwrite_test.go`. The environment is passed as a lookup rather than set with
//! `t.Setenv`: the variables are process-wide and the Rust tests run in parallel.

use crate::parallelwrite::write_workers_from;

#[test]
fn write_workers() {
	let tests = [
		("rbxtsc override wins", "12", "3", "16", 12),
		("rotor override wins over threadpool", "", "3", "16", 3),
		("threadpool does not select workers", "", "", "16", 8),
		("default", "", "", "", 8),
		("explicit cap", "256", "", "", 256),
		("over cap", "99999", "", "", 256),
		("floors valid override", "3.9", "", "", 3),
		("invalid rbxtsc falls through to rotor", "invalid", "3", "", 3),
		("invalid rbxtsc falls through to default", "invalid", "", "16", 8),
		("zero rbxtsc falls through to default", "0", "", "16", 8),
		("negative rbxtsc falls through to default", "-1", "", "16", 8),
		("nan rbxtsc falls through to default", "NaN", "", "16", 8),
		("infinite rbxtsc falls through to default", "Inf", "", "16", 8),
		("invalid rotor falls through to default", "", "invalid", "16", 8),
		("zero rotor falls through to default", "", "0", "16", 8),
		("negative rotor falls through to default", "", "-1", "16", 8),
		("nan rotor falls through to default", "", "NaN", "16", 8),
		("infinite rotor falls through to default", "", "Inf", "16", 8),
	];
	for (name, rbxtsc, rotor, threadpool, want) in tests {
		let got = write_workers_from(|variable| {
			let value = match variable {
				"RBXTSC_WRITE_CONCURRENCY" => rbxtsc,
				"ROTOR_WRITE_WORKERS" => rotor,
				"UV_THREADPOOL_SIZE" => threadpool,
				_ => "",
			};
			Some(value.to_owned())
		});
		assert_eq!(got, want, "{name}: writeWorkers()");
	}
}
