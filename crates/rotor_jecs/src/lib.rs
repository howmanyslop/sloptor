//! A native port of rbxts-transformer-jecs: it caches `world.query(...)` calls and inlines query
//! iteration into archetype loops (Go's `internal/jecs`).
//!
//! It mirrors the JavaScript transformer function for function (see
//! `node_modules/rbxts-transformer-jecs/out/src`), so its output stays identical to the sidecar's.
//! Callers only route a project here when the installed transformer's sources hash to the version
//! this port was written against.
//!
//! One module per Go file: `transform.go`, `query.go` and `forof.go`.

pub mod forof;
pub mod query;
pub mod transform;

pub use transform::{Config, Input, TransformResult, needs_transform, transform};
