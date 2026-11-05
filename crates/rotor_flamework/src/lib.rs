//! Native Flamework: the port of Go's `internal/flamework`.
//!
//! One module per Go file, under the Go file's name. Go's errors, which wrap sentinels with `%w`
//! and are matched with `errors.Is` and `errors.As`, are [`Error`]: the Go message, the sentinels
//! it wraps ([`ErrorKind`]) and the typed error inside it, if any ([`ErrorDetail`]).

pub mod artifacts;
pub mod artifacts_commit;
pub mod artifacts_identity;
pub mod artifacts_stage;
pub mod artifacts_temp;
pub mod build_info;
pub mod build_info_clone;
pub mod build_info_codec;
pub mod build_info_encode;
pub mod build_info_snapshot;
pub mod build_metadata;
pub mod glob;
pub mod guard;
pub mod guard_dedup;
pub mod guard_diagnostics;
pub mod guard_factory;
pub mod guard_types;
pub mod hashids;
pub mod id;
pub mod json_syntax;
pub mod metadata;
pub mod path;
pub mod plan;
pub mod project;
pub mod project_artifacts;
pub mod project_discovery;
pub mod runtime_config;
pub mod state;
pub mod transform;
pub mod transform_access;
pub mod transform_addpaths;
pub mod transform_analysis;
pub mod transform_analysis_build;
pub mod transform_analysis_discovery;
pub mod transform_binary;
pub mod transform_call;
pub mod transform_call_prefilter;
pub mod transform_class;
pub mod transform_class_diagnostics;
pub mod transform_class_diagnostics_constraints;
pub mod transform_component;
pub mod transform_decorators;
pub mod transform_decorators_imports;
pub mod transform_delete;
pub mod transform_diagnostics;
pub mod transform_expressions;
pub mod transform_guard_library;
pub mod transform_imports;
pub mod transform_inlining;
pub mod transform_intrinsics;
pub mod transform_macro_resolution;
pub mod transform_macro_types;
pub mod transform_macros;
pub mod transform_network;
pub mod transform_new;
pub mod transform_parallel;
pub mod transform_parameters;
pub mod transform_paths;
pub mod transform_reflection;
pub mod transform_reflection_metadata;
pub mod transform_reflection_methods;
pub mod transform_source_imports;
pub mod transform_state;
pub mod transform_symbols;
pub mod transform_unary;
pub mod transform_update;

#[cfg(test)]
mod tests;

use std::fmt;

pub use artifacts::{Artifact, persist_artifacts, prepare_artifacts};
pub use build_info::{BuildClass, BuildDecorator, BuildGlobs, BuildInfo, BuildInfoSnapshot, BuildMetadata};
pub use glob::GlobPaths;
pub use id::{IdentifierRequest, generate_identifier};
pub use metadata::Metadata;
pub use plan::{AnalysisState, ClassPlan, DecoratorPlan, FileAnalysis, FilePlan};
pub use project::{IdMode, Project, ProjectOptions, open_project};
pub use runtime_config::{RuntimeConfig, RuntimeLogLevel};
pub use state::{DeclarationIdentity, FLAMEWORK_VERSION};
pub use transform::transform;
pub use transform_diagnostics::Diagnostic;
pub use transform_macro_types::{GuardBuildResult, MacroImport, MacroRuntime};
pub use transform_state::{SourceMetadata, SourceTrace, TransformInput, TransformResult, TransformState};

/// A Go sentinel error (`ErrX = errors.New(...)`), matched with [`Error::is`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
	/// `ErrPathEscape`
	PathEscape,
	/// `ErrInvalidGlob`
	InvalidGlob,
	/// `ErrInvalidBuildInfo`
	InvalidBuildInfo,
	/// `ErrDuplicateIdentifier`
	DuplicateIdentifier,
	/// `ErrDuplicateBuildClass`
	DuplicateBuildClass,
	/// `ErrInvalidIDMode`
	InvalidIdMode,
	/// `ErrNilStringHashes`
	NilStringHashes,
	/// `ErrPackageNotFound`
	PackageNotFound,
	/// `ErrInvalidProject`
	InvalidProject,
	/// `ErrInvalidPackage`
	InvalidPackage,
	/// `ErrInvalidRuntimeConfig`
	InvalidRuntimeConfig,
	/// `ErrDuplicateFile`
	DuplicateFile,
	/// `ErrInvalidFileID`
	InvalidFileId,
	/// `ErrDuplicateClass`
	DuplicateClass,
	/// `ErrInvalidTransformInput`
	InvalidTransformInput,
	/// `ErrAmbiguousFilePlan`
	AmbiguousFilePlan,
	/// `ErrUnsupportedGuardType`
	UnsupportedGuardType,
	/// `ErrInvalidClassDeclaration`
	InvalidClassDeclaration,
	/// `ErrUnplannedClass`
	UnplannedClass,
	/// `ErrInvalidDecorator`
	InvalidDecorator,
	/// `ErrUnplannedDecorator`
	UnplannedDecorator,
	/// `ErrUnsupportedType`
	UnsupportedType,
	/// `ErrCircularDependency`
	CircularDependency,
	/// `ErrInvalidMacro`
	InvalidMacro,
	/// `ErrMacroDiagnostic`
	MacroDiagnostic,
	/// `ErrGuardBuilderAbsent`
	GuardBuilderAbsent,
	/// `ErrDynamicObfuscatedAccess`
	DynamicObfuscatedAccess,
	/// `ErrDirectAttributeUpdate`
	DirectAttributeUpdate,
	/// `ErrInvalidMacroRandomIndex`
	InvalidMacroRandomIndex,
	/// `os.ErrNotExist` (`fs.ErrNotExist`)
	NotExist,
}

impl ErrorKind {
	/// The sentinel's `errors.New` text.
	#[must_use]
	pub const fn text(self) -> &'static str {
		match self {
			Self::PathEscape => "flamework: path escapes project root",
			Self::InvalidGlob => "flamework: invalid glob",
			Self::InvalidBuildInfo => "flamework: invalid flamework.build",
			Self::DuplicateIdentifier => "flamework: duplicate identifier",
			Self::DuplicateBuildClass => "flamework: duplicate build class",
			Self::InvalidIdMode => "invalid Flamework ID generation mode",
			Self::NilStringHashes => "flamework string hash map is nil",
			Self::PackageNotFound => "flamework package.json not found",
			Self::InvalidProject => "invalid Flamework project",
			Self::InvalidPackage => "invalid Flamework package.json",
			Self::InvalidRuntimeConfig => "flamework: invalid flamework.json",
			Self::DuplicateFile => "duplicate Flamework file analysis",
			Self::InvalidFileId => "invalid Flamework file ID",
			Self::DuplicateClass => "duplicate Flamework class",
			Self::InvalidTransformInput => "invalid Flamework transform input",
			Self::AmbiguousFilePlan => "ambiguous Flamework file plan",
			Self::UnsupportedGuardType => "unsupported Flamework guard type",
			Self::InvalidClassDeclaration => "invalid Flamework class declaration",
			Self::UnplannedClass => "unplanned Flamework class",
			Self::InvalidDecorator => "invalid Flamework decorator",
			Self::UnplannedDecorator => "unplanned Flamework decorator",
			Self::UnsupportedType => "unsupported Flamework reflected type",
			Self::CircularDependency => "circular Flamework constructor dependency",
			Self::InvalidMacro => "invalid Flamework macro",
			Self::MacroDiagnostic => "flamework macro diagnostic",
			Self::GuardBuilderAbsent => "flamework guard builder is not configured",
			Self::DynamicObfuscatedAccess => "flamework key-obfuscated object must be accessed directly",
			Self::DirectAttributeUpdate => "flamework attribute update requires an instance access",
			Self::InvalidMacroRandomIndex => "invalid Flamework macro random index",
			Self::NotExist => "file does not exist",
		}
	}
}

/// The typed error an [`Error`] carries, Go's `errors.As` targets.
#[derive(Clone, Debug)]
pub enum ErrorDetail {
	/// `upstreamBoundaryError`: a message upstream Flamework raises as an exception.
	UpstreamBoundary,
	/// `*jsonSyntaxError`
	JsonSyntax,
	/// `*ProjectDiagnosticError`
	ProjectDiagnostics(Vec<transform_diagnostics::Diagnostic>),
	/// `*MacroError`
	Macro(transform_macro_types::MacroError),
	/// `*GuardGenerationError`
	Guard(guard_diagnostics::GuardGenerationError),
	/// `*CircularDependencyError`: the cycle's internal IDs.
	CircularDependency(Vec<String>),
}

/// A Go `error` of this package: its message, the sentinels it wraps and its typed error.
#[derive(Clone, Debug)]
pub struct Error {
	message: String,
	kinds: Vec<ErrorKind>,
	detail: Option<Box<ErrorDetail>>,
}

impl Error {
	/// `errors.New(message)` or `fmt.Errorf` without `%w`.
	#[must_use]
	pub fn msg(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
			kinds: Vec::new(),
			detail: None,
		}
	}

	/// A bare sentinel.
	#[must_use]
	pub fn sentinel(kind: ErrorKind) -> Self {
		Self {
			message: kind.text().to_owned(),
			kinds: vec![kind],
			detail: None,
		}
	}

	/// `fmt.Errorf("%w<rest>", kind)`: the sentinel's text followed by `rest`.
	#[must_use]
	pub fn kinded(kind: ErrorKind, rest: &str) -> Self {
		Self {
			message: format!("{}{rest}", kind.text()),
			kinds: vec![kind],
			detail: None,
		}
	}

	/// An error with a typed detail.
	#[must_use]
	pub fn with_detail(message: impl Into<String>, detail: ErrorDetail) -> Self {
		Self {
			message: message.into(),
			kinds: Vec::new(),
			detail: Some(Box::new(detail)),
		}
	}

	/// `fmt.Errorf("<prefix>: %w", self)`.
	#[must_use]
	pub fn wrap(mut self, prefix: &str) -> Self {
		self.message = format!("{prefix}: {}", self.message);
		self
	}

	/// `fmt.Errorf("%w: %w", kind, self)`.
	#[must_use]
	pub fn under(mut self, kind: ErrorKind) -> Self {
		self.message = format!("{}: {}", kind.text(), self.message);
		self.kinds.push(kind);
		self
	}

	/// Replaces the message, keeping what it wraps (`fmt.Errorf` with a `%w` of `self`).
	#[must_use]
	pub fn reword(mut self, message: String) -> Self {
		self.message = message;
		self
	}

	/// Adds a wrapped sentinel.
	#[must_use]
	pub fn also(mut self, kind: ErrorKind) -> Self {
		self.kinds.push(kind);
		self
	}

	/// `errors.Is(err, kind)`.
	#[must_use]
	pub fn is(&self, kind: ErrorKind) -> bool {
		self.kinds.contains(&kind)
	}

	/// The typed error, Go's `errors.As`.
	#[must_use]
	pub fn detail(&self) -> Option<&ErrorDetail> {
		self.detail.as_deref()
	}

	/// The Go message (`err.Error()`).
	#[must_use]
	pub fn message(&self) -> &str {
		&self.message
	}

	/// An I/O error as Go's `*PathError` would read: `<op> <path>: <text>`.
	#[must_use]
	pub fn io(op: &str, path: &str, error: &std::io::Error) -> Self {
		let mut out = Self::msg(rotor_support::gostr::path_error(op, path, error));
		if error.kind() == std::io::ErrorKind::NotFound {
			out.kinds.push(ErrorKind::NotExist);
		}
		out
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for Error {}

/// Go's `filepath.Abs`, as a `Result` with Go's message.
pub(crate) fn abs(path: &str) -> Result<String, std::io::Error> {
	rotor_support::gopath::abs(path)
}
