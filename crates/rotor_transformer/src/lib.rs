//! The TypeScript to Luau transformer: the port of Go's `internal/transformer`, itself the port
//! of roblox-ts's `TSTransformer`.
//!
//! [`State`] is one file's transform state; [`transform_source_file`] turns its source file into
//! the Luau statements `rotor_luau::render` prints. The core sits at the top level; the modules
//! `expressions`, `control`, `classes`, `macros`, `luau_types` and `extras` hold the rest
//! (see `SLICES.md` for where each Go file lives).

mod access;
mod assignment;
mod binary;
mod bitwise;
mod call;
mod conditional;
mod delete;
pub mod diagnostics;
mod diagservice;
mod dispatch;
mod ensureorder;
mod exports;
mod identifier;
mod ismethod;
mod literals;
mod logical;
mod loops;
mod loops_numeric;
mod macromanager;
mod methodassignment;
mod newexpression;
mod nodemodulespath;
mod optionalchain;
mod pointer;
mod prelude;
mod receivertype;
mod refwalker;
mod runtimelib;
mod selectsymlinkpath;
mod shared;
mod sourcefile;
mod sourcepositions;
mod spreadhelpers;
mod state;
mod statementlist;
mod statements;
mod truthiness;
mod types;
mod unary;
mod util;
mod valuetoidstr;
mod voidexpr;

// The modules ported in Stage 4b, one directory per group of Go files.
mod classes;
mod control;
mod expressions;
mod extras;
mod luau_types;
pub mod macros;

pub use diagnostics::Diagnostic;
pub use diagservice::DiagService;
pub use exports::{ExportShape, choose_export_shape};
pub use macros::FileResolver;
pub use nodemodulespath::{NodeModulesPathMapping, NodeModulesPathPattern};
pub use sourcefile::{header_comment, transform_source_file};
pub use state::{MultiState, ProjectType, RojoContext, State, StateOptions, TryUses};

#[cfg(test)]
mod tests;
