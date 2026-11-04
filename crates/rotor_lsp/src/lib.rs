//! The port of `internal/lsp`: sloptor's language server. It publishes the diagnostics only
//! sloptor can produce (transformer rejections, comment-directive refusals, internal compiler
//! errors) into an editor over stdio.
//!
//! The editor's own TypeScript server already reports TypeScript errors, so by default this server
//! stays quiet about those (`initializationOptions.typeErrors` turns them on). The compile step is
//! injected through [`Options`], so the crate owns the protocol, scheduling and position mapping
//! and nothing else.
//!
//! Every message is byte-identical to Go's: `tsgo/jsonrpc` framing, `lsproto` field order and
//! `go-json-experiment` string escaping (`protocol.rs`), and `lsconv`'s positions and file URIs
//! (`lsconv.rs`).

pub mod jsonrpc;
pub mod lsconv;
mod params;
pub mod protocol;
mod publish;
mod scheduler;
mod server;

pub use server::{Cancel, Census, DEFAULT_DEBOUNCE, FindTsConfig, Options, SERVER_NAME, serve};

#[cfg(test)]
use tempfile as _;
