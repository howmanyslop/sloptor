//! Loads native compiler settings from `rotor.toml`.

mod decode;
mod flamework;
mod load;
mod model;
mod parallel_transformers;
mod schema;

pub use flamework::{ConfigError, FlameworkConfig, FlameworkOptimizations, FlameworkProfile};
pub use load::{CONFIG_FILE_NAME, LoadError, load};
pub use model::Config;
pub use parallel_transformers::{ParallelTransformerDependency, ParallelTransformerPlugin, ParallelTransformersConfig};
pub use schema::{SCHEMA, SCHEMA_DIRECTIVE, SCHEMA_FILE_NAME, SCHEMA_URL};

impl Config {
	/// Structural validation errors for compiler settings.
	#[must_use]
	pub fn validate(&self) -> Vec<ConfigError> {
		self.validate_flamework()
	}
}
