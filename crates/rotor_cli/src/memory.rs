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
