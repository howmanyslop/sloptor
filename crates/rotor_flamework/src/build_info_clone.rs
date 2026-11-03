//! `build_info_clone.go`: deep copies of build-info parts.

use std::collections::BTreeMap;

use crate::build_info::{BuildClass, BuildGlobs, BuildMetadata, GoStringSlicesMap, GoStrings};
use crate::runtime_config::{RuntimeConfig, parse_runtime_config};

/// `cloneRuntimeConfig`: a round trip through JSON, as Go does.
pub(crate) fn clone_runtime_config(config: Option<&RuntimeConfig>) -> Option<RuntimeConfig> {
	let data = config?.marshal_json();
	parse_runtime_config(data.as_bytes()).ok()
}

pub(crate) fn clone_metadata(metadata: Option<&BuildMetadata>) -> Option<BuildMetadata> {
	let metadata = metadata?;
	Some(BuildMetadata {
		config: clone_runtime_config(metadata.config.as_ref()),
		globs: metadata.globs.as_ref().map(|globs| BuildGlobs {
			paths: clone_string_slices_map(globs.paths.as_ref()),
			origins: clone_string_slices_map(globs.origins.as_ref()),
		}),
	})
}

/// `cloneStringSlicesMap`. `append([]string(nil), values...)` is nil for an empty slice, so an
/// empty match list becomes nil.
pub(crate) fn clone_string_slices_map(source: Option<&BTreeMap<String, GoStrings>>) -> GoStringSlicesMap {
	Some(
		source?
			.iter()
			.map(|(key, values)| (key.clone(), go_append_nil(values.as_deref().unwrap_or_default())))
			.collect(),
	)
}

/// `append([]string(nil), values...)`
pub(crate) fn go_append_nil(values: &[String]) -> GoStrings {
	if values.is_empty() { None } else { Some(values.to_vec()) }
}

pub(crate) fn clone_string_pointer(value: Option<&str>) -> Option<String> {
	value.map(str::to_owned)
}

pub(crate) fn clone_build_class(class: &BuildClass) -> BuildClass {
	class.clone()
}
