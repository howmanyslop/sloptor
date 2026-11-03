//! The experimental `parallelTransformers` section.

/// An experimental, project-owned attestation. Unsupported or incomplete
/// attestations select the ordinary serial path.
///
/// `config` is the complete plugin options object encoded as JSON, including
/// `transform`. The order must equal the entire initial external stage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParallelTransformersConfig {
	/// How many workers to use.
	pub workers: i64,
	/// Whether the project attests that its batches are independent.
	pub batch_independent: bool,
	/// The TypeScript package the plugins were attested against.
	pub typescript: String,
	/// The plugins, in order.
	pub plugins: Vec<ParallelTransformerPlugin>,
	/// The patched or local dependencies on the plugins' load paths.
	pub dependencies: Vec<ParallelTransformerDependency>,
}

/// One plugin of a [`ParallelTransformersConfig`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParallelTransformerPlugin {
	/// The complete plugin options object as JSON.
	pub config: String,
	/// The SHA-256 of the plugin.
	pub sha256: String,
}

/// A dependency of a [`ParallelTransformersConfig`].
///
/// `from` selects the plugin whose normal Node resolution path owns this
/// patched or local dependency. The owner lists every such load-path
/// dependency.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParallelTransformerDependency {
	/// The zero-based index of the owning plugin.
	pub from: i64,
	/// The module specifier.
	pub specifier: String,
	/// The SHA-256 of the dependency.
	pub sha256: String,
}
