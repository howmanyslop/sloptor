//! Rojo project resolution and output-path translation.
//!
//! Ports `@roblox-ts/rojo-resolver` 1.1.0 and `@roblox-ts/path-translator`
//! 1.1.0. The resolver decides require paths, so quirks are ported verbatim.

mod config;
mod json;
mod pathtranslator;
mod prefetch;
mod project_files;
mod resolver;
mod resolver_cache;
#[cfg(test)]
mod test_support;

pub use config::{Tree, TreeEntry, validate_config};
pub use json::{JsonValue, parse as parse_json};
pub use pathtranslator::PathTranslator;
pub use project_files::ProjectFiles;
pub use resolver::{
	FileRelation, InvalidResolverState, NetworkType, ParseProjectFileError, PartitionInfo, RbxPath, RbxPathLookup,
	RbxType, RelativeRbxPath, RelativeRbxPathSegment, ResolverFilePathState, ResolverState, RojoResolver,
	canonical_file_name, find_rojo_config_file_path, parse_project_file, relative,
};
pub use resolver_cache::RojoResolverCache;
