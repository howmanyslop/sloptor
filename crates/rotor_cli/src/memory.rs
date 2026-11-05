//! Giving memory back between the passes of a long-lived session (watch mode, the language
//! server, the build API).
//!
//! Every pass frees its programs (`rotor_tsrs::project::ProgramScope`), but two allocators keep
//! what those frees emptied:
//!
//! - mimalloc keeps emptied pages in the heap of the thread that allocated them, where a worker
//!   thread that never allocates in that size class again leaves them for good;
//! - tsrs keeps the slab each thread last carved region chunks from (its used part dead once the
//!   pass's regions are freed), and up to 64 released 1 MiB slabs committed for reuse.
//!
//! After a pass, each thread that compiled (the tsrs worker pool, the idle checker threads and
//! rotor's session threads, all kept for the session) collects its heap and hands its slab on,
//! and the cached slabs are decommitted, so the session's footprint stays where one pass puts it.

/// Collects the calling thread's heap and every kept thread's, then decommits tsrs's cached slabs.
pub fn trim() {
	collect();
	rotor_tsrs::compiler::worker_pool().broadcast(|_| collect());
	rotor_tsrs::compiler::checkerpool::for_each_idle_checker_thread(collect);
	rotor_tsrs::threads::for_each_idle(collect);
	rotor_tsrs::tscore::arena::release_slab_cache();
}

fn collect() {
	rotor_tsrs::tscore::arena::release_thread_slabs();
	// SAFETY: `mi_collect` takes no pointers and only returns free pages of mimalloc's own heaps
	// to the OS; it is safe to call from any thread at any time.
	#[expect(unsafe_code, reason = "mimalloc's collect has no safe binding")]
	unsafe {
		libmimalloc_sys2::mi_collect(true);
	}
}

/// Turns off mimalloc's teardown at process exit, before `main` (and before a test harness's).
///
/// mimalloc tears its process state down from a C destructor at `exit` (`mi_process_done`),
/// while other threads can still run: a checker thread ending at its idle timeout, a session
/// thread ending after a one-shot pass, the file loader's drop helper, a test's own threads. A
/// thread that allocates or ends after the teardown starts again on deleted state, and a debug
/// build's mimalloc assertion aborts the process, usually with nothing on stderr (mimalloc stops
/// writing output once it is done). A process needs no teardown, as the system reclaims its
/// memory, and `destroy_on_exit` 2 skips it (`_mi_auto_process_done`).
extern "C" fn skip_mimalloc_exit_teardown() {
	// SAFETY: sets one of mimalloc's options, which takes no pointers and may be set before
	// mimalloc initializes.
	#[expect(unsafe_code, reason = "mimalloc's options have no safe binding")]
	unsafe {
		libmimalloc_sys2::mi_option_set(libmimalloc_sys2::mi_option_destroy_on_exit, 2);
	}
}

/// Runs [`skip_mimalloc_exit_teardown`] as a static initializer, so the test harnesses that
/// share this crate's allocator get it too.
// SAFETY: the section is the platform's list of initializers to run before `main`, and the
// entry is a plain `extern "C" fn()` that only sets an option.
#[expect(unsafe_code, reason = "a static initializer needs its link section")]
#[used]
#[cfg_attr(target_vendor = "apple", unsafe(link_section = "__DATA,__mod_init_func"))]
#[cfg_attr(
	any(target_os = "linux", target_os = "android", target_os = "freebsd"),
	unsafe(link_section = ".init_array")
)]
#[cfg_attr(windows, unsafe(link_section = ".CRT$XCU"))]
static SKIP_MIMALLOC_EXIT_TEARDOWN: extern "C" fn() = skip_mimalloc_exit_teardown;

#[cfg(test)]
mod tests {
	/// The initializer ran before the test harness: mimalloc will not tear itself down at exit
	/// under threads that are still running.
	#[test]
	fn mimalloc_exit_teardown_is_off_before_main() {
		// SAFETY: reads one of mimalloc's options.
		#[expect(unsafe_code, reason = "mimalloc's options have no safe binding")]
		let value = unsafe { libmimalloc_sys2::mi_option_get(libmimalloc_sys2::mi_option_destroy_on_exit) };
		assert_eq!(value, 2);
	}
}
