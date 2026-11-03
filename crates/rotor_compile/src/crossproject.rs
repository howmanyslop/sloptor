//! `crossproject.go`: cross-project write metadata is Stage 5b. Stage 5a ports only the alias
//! helper the project-reference import map uses.

use std::collections::HashMap;

use rotor_rojo::canonical_file_name;
use rotor_support::gopath;

/// `addProjectRealPathAliases`: also keys the paths under `dir` by `dir`'s real path, since
/// import lookups may see a file through its resolved path. One realpath per project.
pub(crate) fn add_project_real_path_aliases(
	paths: &mut HashMap<String, String>,
	dir: &str,
	use_case_sensitive_file_names: bool,
) {
	let spelled = canonical_file_name(dir, use_case_sensitive_file_names);
	let real = canonical_file_name(&crate::sidecar::realpath_or(dir), use_case_sensitive_file_names);
	if real == spelled {
		return;
	}
	let prefix = format!("{spelled}{}", gopath::NATIVE.separator());
	let aliases: Vec<(String, String)> = paths
		.iter()
		.filter_map(|(key, value)| {
			key.strip_prefix(&prefix)
				.map(|rest| (gopath::join(&[&real, rest]), value.clone()))
		})
		.collect();
	paths.extend(aliases);
}
