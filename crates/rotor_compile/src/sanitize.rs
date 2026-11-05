//! `sanitize.go`.
//!
//! The TypeScript 7 compatibility rewrites of tsconfig files and the compiler-types iteration
//! interfaces. The rewrites and the file system that applies them to every config of the extends
//! chain live in `rotor_tsrs` (`tsconfig` and the program loader's file system), the one crate that
//! may reach tsrs; these are their Go names.

pub use rotor_tsrs::tsconfig::{
	is_compiler_types_dts_path, is_tsconfig_path, rewrite_iterable_arity, sanitize_tsconfig, strip_jsonc,
};
