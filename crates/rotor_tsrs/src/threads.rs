//! Scoped threads that are reused across scopes and sessions.
//!
//! [`scope`] works like [`std::thread::scope`]: every job spawned in it has a thread of its own
//! and finishes before `scope` returns. [`scope_io`] uses a 2 MiB stack pool for file I/O rather
//! than the 512 MiB checker stack. Idle threads wait up to [`IDLE_TIMEOUT`] in one-shot builds
//! (and indefinitely when [`crate::project::long_lived`] is set) so consecutive scopes reuse live
//! threads instead of paying thread creation and 512 MiB stack teardown per scope.
//!
//! A reused checker thread does what its exit would have done between jobs (the tsrs thread state,
//! its arena handed on), and [`for_each_idle`] lets the session collect each one's allocator heap.

use std::any::Any;
use std::marker::PhantomData;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use crate::CHECKER_STACK_BYTES;

/// Stack size for pure I/O workers (`parallelwrite`, non-compiled file copy).
const IO_STACK_BYTES: usize = 2 << 20;

/// How long an idle worker thread waits for its next job in a one-shot build before exiting.
const IDLE_TIMEOUT: Duration = Duration::from_secs(1);

type Job = Box<dyn FnOnce() + Send + 'static>;

#[derive(Clone, Copy)]
enum Pool {
	Checker,
	Io,
}

struct IdleThread {
	thread: std::thread::ThreadId,
	jobs: Sender<Job>,
}

/// Idle checker threads waiting for a job.
static IDLE: Mutex<Vec<IdleThread>> = Mutex::new(Vec::new());
/// Idle I/O threads waiting for a job.
static IO_IDLE: Mutex<Vec<IdleThread>> = Mutex::new(Vec::new());

impl Pool {
	const fn idle(self) -> &'static Mutex<Vec<IdleThread>> {
		match self {
			Self::Checker => &IDLE,
			Self::Io => &IO_IDLE,
		}
	}

	const fn stack_size(self) -> usize {
		match self {
			Self::Checker => CHECKER_STACK_BYTES,
			Self::Io => IO_STACK_BYTES,
		}
	}

	const fn name(self) -> &'static str {
		match self {
			Self::Checker => "rotor-worker",
			Self::Io => "rotor-io-worker",
		}
	}
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `job` on an idle thread from `pool`, or on a new one.
fn run(pool: Pool, mut job: Job) -> std::io::Result<()> {
	loop {
		let Some(idle) = lock(pool.idle()).pop() else {
			break;
		};
		match idle.jobs.send(job) {
			Ok(()) => return Ok(()),
			Err(mpsc::SendError(returned)) => job = returned,
		}
	}
	let (sender, jobs) = mpsc::channel::<Job>();
	std::thread::Builder::new()
		.name(pool.name().to_owned())
		.stack_size(pool.stack_size())
		.spawn(move || worker(pool, job, &sender, &jobs))?;
	Ok(())
}

fn worker(pool: Pool, mut job: Job, sender: &Sender<Job>, jobs: &Receiver<Job>) {
	let thread = std::thread::current().id();
	loop {
		job();
		if matches!(pool, Pool::Checker) {
			// What the thread's exit would do, so the next job starts as on a new thread.
			tsrs_ast::stop_id_blocks();
			tsrs_core::sharedgraph::leave_overlay();
			tsrs_core::ptr::release_own_arena();
		}
		lock(pool.idle()).push(IdleThread {
			thread,
			jobs: sender.clone(),
		});
		if crate::project::long_lived() {
			let Ok(next) = jobs.recv() else {
				return;
			};
			job = next;
			continue;
		}
		if let Ok(next) = jobs.recv_timeout(IDLE_TIMEOUT) {
			job = next;
			continue;
		}
		let mut idle = lock(pool.idle());
		if let Some(at) = idle.iter().position(|entry| entry.thread == thread) {
			idle.swap_remove(at);
			return;
		}
		drop(idle);
		// `run()` already popped our entry and is sending a job.
		let Ok(next) = jobs.recv() else {
			return;
		};
		job = next;
	}
}

/// Runs `job` on a session thread without waiting for it ([`std::thread::spawn`] for a job that
/// owns everything it uses).
///
/// # Errors
///
/// No thread could be started for it.
pub fn spawn(job: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
	run(Pool::Checker, Box::new(job))
}

/// Runs `f` on every idle session thread (both checker and I/O pools) and waits for all of them
/// (the session's trim between passes).
pub fn for_each_idle(f: impl Fn() + Send + Sync + 'static) {
	let mut idle = std::mem::take(&mut *lock(&IDLE));
	idle.extend(std::mem::take(&mut *lock(&IO_IDLE)));
	let f = Arc::new(f);
	let (done, finished) = mpsc::channel::<()>();
	let mut sent = 0;
	for thread in idle {
		let (f, done) = (Arc::clone(&f), done.clone());
		if thread
			.jobs
			.send(Box::new(move || {
				f();
				let _ = done.send(());
			}))
			.is_ok()
		{
			sent += 1;
		}
	}
	drop(done);
	for _ in 0..sent {
		if finished.recv().is_err() {
			break;
		}
	}
}

/// What a scope waits for: its running jobs, and the first panic among them.
#[derive(Default)]
struct Pending {
	state: Mutex<(usize, Option<Box<dyn Any + Send>>)>,
	done: Condvar,
}

/// A scope's handle for spawning jobs ([`std::thread::Scope`]).
pub struct Scope<'scope, 'env: 'scope> {
	pool: Pool,
	pending: Arc<Pending>,
	_lifetime: PhantomData<&'scope mut &'scope ()>,
	_env: PhantomData<&'env mut &'env ()>,
}

/// A spawned job's result ([`std::thread::ScopedJoinHandle`]).
pub struct ScopedJoinHandle<'scope, T> {
	result: Option<Receiver<std::thread::Result<T>>>,
	pending: Arc<Pending>,
	_scope: PhantomData<&'scope ()>,
}

impl<T> ScopedJoinHandle<'_, T> {
	/// Waits for the job.
	///
	/// # Errors
	///
	/// The job's panic.
	pub fn join(mut self) -> std::thread::Result<T> {
		self.result
			.take()
			.and_then(|result| result.recv().ok())
			.unwrap_or_else(|| Err(Box::new("rotor-worker: the job's thread ended without a result")))
	}
}

impl<T> Drop for ScopedJoinHandle<'_, T> {
	/// An unjoined job's panic goes to [`scope`], as with std's scoped threads.
	fn drop(&mut self) {
		if let Some(Ok(Err(payload))) = self.result.take().map(|result| result.try_recv()) {
			self.pending.record(payload);
		}
	}
}

impl Pending {
	fn record(&self, payload: Box<dyn Any + Send>) {
		let mut state = lock(&self.state);
		if state.1.is_none() {
			state.1 = Some(payload);
		}
	}
}

impl<'scope> Scope<'scope, '_> {
	/// Runs `f` on a thread of its own ([`std::thread::Scope::spawn`]).
	///
	/// # Panics
	///
	/// No thread could be started for it, as std's.
	pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
	where
		F: FnOnce() -> T + Send + 'scope,
		T: Send + 'scope,
	{
		self.try_spawn(f)
			.unwrap_or_else(|error| std::panic::resume_unwind(Box::new(error)))
	}

	/// Runs `f` on a thread of its own with the checker's stack size
	/// ([`std::thread::Builder::spawn_scoped`]).
	///
	/// # Errors
	///
	/// No thread could be started for it.
	pub fn try_spawn<F, T>(&'scope self, f: F) -> std::io::Result<ScopedJoinHandle<'scope, T>>
	where
		F: FnOnce() -> T + Send + 'scope,
		T: Send + 'scope,
	{
		let (sender, result) = mpsc::channel();
		let pending = Arc::clone(&self.pending);
		lock(&pending.state).0 += 1;
		let job: Box<dyn FnOnce() + Send + 'scope> = Box::new(move || {
			let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
			match sender.send(outcome) {
				Ok(()) => {}
				// The handle was dropped: an unjoined panic goes to `scope`, as with std's.
				Err(mpsc::SendError(Err(payload))) => pending.record(payload),
				// A result nobody joins is dropped here. If its drop panicked, the count below would
				// never come down and `scope` would wait forever, so abort, as std does.
				Err(mpsc::SendError(Ok(value))) => {
					if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(value))).is_err() {
						let _ = std::io::Write::write_all(
							&mut std::io::stderr(),
							b"fatal runtime error: thread result panicked on drop, aborting\n",
						);
						std::process::abort();
					}
				}
			}
			let mut state = lock(&pending.state);
			state.0 -= 1;
			drop(state);
			pending.done.notify_all();
		});
		// SAFETY: the job borrows only what outlives `'scope`, and `scope` does not return (nor
		// unwind) before every job it spawned has finished (`Pending` counts them; a job that
		// could not be started is uncounted below), so the job never runs after its borrows end.
		#[expect(unsafe_code, reason = "a scoped job on a thread that outlives the scope")]
		let job: Job = unsafe { std::mem::transmute::<Box<dyn FnOnce() + Send + 'scope>, Job>(job) };
		if let Err(error) = run(self.pool, job) {
			lock(&self.pending.state).0 -= 1;
			return Err(error);
		}
		Ok(ScopedJoinHandle {
			result: Some(result),
			pending: Arc::clone(&self.pending),
			_scope: PhantomData,
		})
	}
}

/// [`std::thread::scope`] on pooled checker-stack threads.
///
/// # Panics
///
/// Resumes the panic of a job that was not joined, after every job finished.
pub fn scope<'env, F, T>(f: F) -> T
where
	F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
	scope_in(Pool::Checker, f)
}

/// [`std::thread::scope`] on pooled 2 MiB-stack I/O threads.
///
/// # Panics
///
/// Resumes the panic of a job that was not joined, after every job finished.
pub fn scope_io<'env, F, T>(f: F) -> T
where
	F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
	scope_in(Pool::Io, f)
}

fn scope_in<'env, F, T>(pool: Pool, f: F) -> T
where
	F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
{
	let scope = Scope {
		pool,
		pending: Arc::new(Pending::default()),
		_lifetime: PhantomData,
		_env: PhantomData,
	};
	let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&scope)));
	let mut state = lock(&scope.pending.state);
	while state.0 > 0 {
		state = scope.pending.done.wait(state).unwrap_or_else(PoisonError::into_inner);
	}
	let unjoined = state.1.take();
	drop(state);
	match result {
		Err(payload) => std::panic::resume_unwind(payload),
		Ok(_) if unjoined.is_some() => std::panic::resume_unwind(unjoined.unwrap_or_else(|| Box::new(()))),
		Ok(value) => value,
	}
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::{AtomicUsize, Ordering};

	use super::scope;

	#[test]
	fn jobs_borrow_the_scope_and_finish_before_it_returns() {
		let counter = AtomicUsize::new(0);
		let values = vec![1, 2, 3];
		let sums: Vec<usize> = scope(|scope| {
			let handles: Vec<_> = (0..4)
				.map(|index| {
					let (counter, values) = (&counter, &values);
					scope.spawn(move || {
						counter.fetch_add(1, Ordering::Relaxed);
						values.iter().sum::<usize>() + index
					})
				})
				.collect();
			handles
				.into_iter()
				.map(|handle| handle.join().unwrap_or_default())
				.collect()
		});
		assert_eq!(sums, [6, 7, 8, 9]);
		// An unjoined job finishes before the scope returns too.
		scope(|scope| {
			scope.spawn(|| counter.fetch_add(1, Ordering::Relaxed));
		});
		assert_eq!(counter.load(Ordering::Relaxed), 5);
	}

	#[test]
	fn a_joined_panic_is_the_joiners_and_an_unjoined_one_the_scopes() {
		let joined = scope(|scope| scope.spawn(|| std::panic::resume_unwind(Box::new("joined"))).join());
		assert!(joined.is_err());
		let unjoined = std::panic::catch_unwind(|| {
			scope(|scope| {
				scope.spawn(|| std::panic::resume_unwind(Box::new("unjoined")));
			});
		});
		assert!(unjoined.is_err(), "the scope resumes an unjoined job's panic");
	}

	#[test]
	fn io_pool_reuses_threads_across_scopes() {
		let first = super::scope_io(|scope| scope.spawn(|| std::thread::current().id()).join().ok());
		// The scope returns once the job finishes, a moment before its worker files itself as idle.
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
		while super::lock(super::Pool::Io.idle()).is_empty() && std::time::Instant::now() < deadline {
			std::thread::yield_now();
		}
		let second = super::scope_io(|scope| scope.spawn(|| std::thread::current().id()).join().ok());
		assert!(first.is_some());
		assert_eq!(first, second, "sequential io scopes should reuse the idle thread");
	}
}
