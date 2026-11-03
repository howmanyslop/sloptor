//! `sidecar_roots.go`: the root set an incremental round trip asks the worker for.
//!
//! The files being compiled, plus every file that can change the checker's answers about them
//! without being imported. TypeScript pulls each root's import closure in on its own, so a
//! per-file transformer sees the same answers it would in a full build.
//!
//! HAZARD: a transformer that enumerates `program.getSourceFiles()` sees fewer files than in a
//! full build. The known plugins are all per-file.

use std::collections::HashSet;

use rotor_support::gopath;
use rotor_tsrs::ast::{self, SourceFile};
use rotor_tsrs::{P, tspath};

/// `narrowedSidecarRoots`: empty for a full build, which leaves the worker's roots alone.
pub(crate) fn narrowed_sidecar_roots(compile_files: &[P<SourceFile>], project_files: &[P<SourceFile>]) -> Vec<String> {
	if compile_files.is_empty() || compile_files.len() >= project_files.len() {
		return Vec::new();
	}
	let mut roots = Vec::with_capacity(compile_files.len());
	let mut seen = HashSet::with_capacity(compile_files.len());
	let mut add = |name: &str| {
		let path = gopath::from_slash(name).into_owned();
		if seen.insert(path.clone()) {
			roots.push(path);
		}
	};
	for file in compile_files {
		add(file.file_name());
	}
	for file in project_files {
		if contributes_ambient_declarations(*file) {
			add(file.file_name());
		}
	}
	roots
}

/// `contributesAmbientDeclarations`: a declaration file, a global script, or a module that opens
/// `declare global`.
pub(crate) fn contributes_ambient_declarations(source_file: P<SourceFile>) -> bool {
	if tspath::is_declaration_file_name(source_file.file_name()) {
		return true;
	}
	if !ast::is_external_module(source_file) {
		return true;
	}
	// A string-literal name augments a named module, which only an importer sees; an identifier
	// name is `global`.
	source_file
		.module_augmentations()
		.iter()
		.any(|augmentation| !ast::is_string_literal(*augmentation))
}
