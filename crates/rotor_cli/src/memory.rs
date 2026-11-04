//! Giving memory back between the passes of a long-lived session (watch mode, the language
//! server, the build API).
//!
//! Every pass frees its programs (`rotor_tsrs::project::ProgramScope`), but mimalloc keeps the
//! pages those frees emptied in the heap of the thread that allocated them, where a worker thread
//! that never allocates in that size class again leaves them for good. After a pass, each thread
//! that compiled collects its heap, so the session's footprint stays where one pass puts it.

/// Collects the calling thread's heap and every tsrs worker's.
pub fn trim() {
	collect();
	rotor_tsrs::compiler::worker_pool().broadcast(|_| collect());
}

fn collect() {
	// SAFETY: `mi_collect` takes no pointers and only returns free pages of mimalloc's own heaps
	// to the OS; it is safe to call from any thread at any time.
	#[expect(unsafe_code, reason = "mimalloc's collect has no safe binding")]
	unsafe {
		libmimalloc_sys2::mi_collect(true);
	}
}
