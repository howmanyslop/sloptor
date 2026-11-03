//! The `rotor-rs` binary, the Rust port of the rotor CLI.

use std::io::Write as _;
use std::process::ExitCode;

#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

fn main() -> ExitCode {
	let banner = concat!("rotor-rs ", env!("CARGO_PKG_VERSION"), "\n");
	match std::io::stdout().write_all(banner.as_bytes()) {
		Ok(()) => ExitCode::SUCCESS,
		Err(_) => ExitCode::FAILURE,
	}
}
