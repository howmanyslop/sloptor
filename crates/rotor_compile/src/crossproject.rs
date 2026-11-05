//! `crossproject.go`: the solution metadata derived from every project's parsed config.
//!
//! Write roots order projects that share outputs, reference paths let a project import its
//! references' outputs, and the validations of `reference_validation` run over them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rotor_rojo::canonical_file_name;
use rotor_support::{fsutil, gopath};
use rotor_tsrs::tsoptions::outputpaths;
use rotor_tsrs::tspath;

use crate::compile::Error;
use crate::incremental_manifest::normalize_source_file_path;
use crate::project::{
	ProjectOutputHost, create_path_translator, is_path_descendant_of, project_source_files, resolve_include_path,
};
use crate::project_config::{ProjectConfig, parse_project_config};
use crate::reference_validation::{
	ReferenceRojoCache, ValidationError, project_root_dirs, validate_project_outputs,
	validate_projects_reference_rojo_paths, validate_solution_references,
};
use crate::solution::{SolutionGraph, SolutionProject};

/// `solutionWriteMetadata`
#[derive(Default)]
pub(crate) struct SolutionWriteMetadata {
	pub(crate) write_roots: HashMap<String, Vec<String>>,
	pub(crate) wait_only_dependencies: HashMap<String, Vec<String>>,
	pub(crate) restored_metadata_errs: HashMap<String, String>,
	pub(crate) validation_err: Option<ValidationError>,
	pub(crate) reference_outputs: HashMap<String, Vec<String>>,
}

/// `projectReferenceInfo`: one project's share of the solution metadata. It depends only on the
/// parsed config, never on file contents, so a text edit leaves it unchanged.
pub(crate) struct ProjectReferenceInfo {
	pub(crate) config: Option<Arc<ProjectConfig>>,
	pub(crate) err: String,
	pub(crate) write_roots: Vec<String>,
	/// Source and declaration files to Luau paths, for runtime lookup.
	paths: HashMap<String, String>,
	/// The same, keyed by authored spelling without lookup aliases.
	salt_paths: HashMap<String, String>,
	outputs: Vec<String>,
	emits: bool,
}

/// `crossProjectMetadata`: the project infos, cached so a watch cycle re-parses only the
/// invalidated projects' configs.
pub(crate) struct CrossProjectMetadata {
	pub(crate) infos: HashMap<String, ProjectReferenceInfo>,
	/// The inputs of the Rojo mount checks.
	pub(crate) references: Arc<ReferenceRojoCache>,
	/// Configs parsed, for tests.
	pub(crate) config_parses: usize,
	/// Solution metadata derivations, for tests.
	pub(crate) refreshes: usize,
}

/// `populateCrossProjectMetadata`
#[cfg(test)]
pub(crate) fn populate_cross_project_metadata(graph: &mut SolutionGraph) -> SolutionWriteMetadata {
	new_cross_project_metadata(graph).0
}

/// `newCrossProjectMetadata`
pub(crate) fn new_cross_project_metadata(graph: &mut SolutionGraph) -> (SolutionWriteMetadata, CrossProjectMetadata) {
	let mut cache = CrossProjectMetadata {
		infos: HashMap::new(),
		references: Arc::new(ReferenceRojoCache::new(None)),
		config_parses: 0,
		refreshes: 0,
	};
	let metadata = cache.rebuild(graph);
	(metadata, cache)
}

impl CrossProjectMetadata {
	/// `rebuild`: drops every cached info and derives the metadata from fresh parses.
	pub(crate) fn rebuild(&mut self, graph: &mut SolutionGraph) -> SolutionWriteMetadata {
		self.infos = HashMap::with_capacity(graph.projects.len());
		let projects = graph.projects.clone();
		self.parse(&projects);
		self.derive(graph, &projects)
	}

	/// `parse`: re-reads the configs of `projects`; whether any of their metadata changed.
	pub(crate) fn parse(&mut self, projects: &[SolutionProject]) -> bool {
		let mut changed = false;
		for project in projects {
			let info = new_project_reference_info(project);
			self.config_parses += 1;
			if !info.equal(self.infos.get(&project.config_path)) {
				changed = true;
			}
			self.infos.insert(project.config_path.clone(), info);
		}
		changed
	}

	/// `derive`: the solution metadata from the cached infos. Stores each project's reference
	/// paths in `graph`; the Rojo mount check covers the direct references of `consumers` only.
	pub(crate) fn derive(&mut self, graph: &mut SolutionGraph, consumers: &[SolutionProject]) -> SolutionWriteMetadata {
		self.refreshes += 1;
		let mut configs: HashMap<String, &dyn ProjectOutputHost> = HashMap::with_capacity(graph.projects.len());
		let mut metadata = SolutionWriteMetadata::default();
		for project in &graph.projects {
			let Some(info) = self.infos.get(&project.config_path) else {
				continue;
			};
			metadata
				.write_roots
				.insert(project.config_path.clone(), info.write_roots.clone());
			if !info.err.is_empty() {
				metadata
					.restored_metadata_errs
					.insert(project.config_path.clone(), info.err.clone());
				continue;
			}
			if let Some(config) = &info.config {
				configs.insert(project.config_path.clone(), &**config);
			}
		}
		let references: HashMap<String, Vec<String>> = graph
			.projects
			.iter()
			.map(|project| (project.config_path.clone(), project.references.clone()))
			.collect();
		for project in &mut graph.projects {
			let mut paths = HashMap::new();
			let mut salt_paths = HashMap::new();
			let mut outputs = Vec::new();
			let mut visited = HashSet::new();
			for reference in &project.references {
				self.visit_reference(
					reference,
					&references,
					&mut visited,
					&mut paths,
					&mut salt_paths,
					&mut outputs,
				);
			}
			project.options.project_reference_paths = Some(Arc::new(paths));
			project.options.project_reference_salt_paths = Some(Arc::new(salt_paths));
			if let Some(own) = self.infos.get(&project.config_path)
				&& own.emits
			{
				outputs.extend(own.outputs.iter().cloned());
				let include_path =
					match resolve_include_path(&gopath::dir(&project.config_path), &project.options.include_path) {
						Ok(include_path) => include_path,
						Err(error) => {
							metadata.validation_err = Some(error.into());
							return metadata;
						}
					};
				outputs.push(gopath::join(&[&include_path, "RuntimeLib.lua"]));
				metadata.reference_outputs.insert(project.config_path.clone(), outputs);
			}
		}
		metadata.wait_only_dependencies = derive_wait_only_dependencies(&graph.projects, &metadata.write_roots);
		metadata.validation_err = validate_solution_references(graph, &configs).err();
		if metadata.validation_err.is_none() {
			metadata.validation_err = validate_projects_reference_rojo_paths(
				graph,
				consumers,
				&metadata.reference_outputs,
				Some(&self.references),
			)
			.err()
			.map(Into::into);
		}
		if metadata.validation_err.is_none() {
			metadata.validation_err = validate_project_outputs(graph, &configs).err().map(Into::into);
		}
		metadata
	}

	fn visit_reference(
		&self,
		config_path: &str,
		references: &HashMap<String, Vec<String>>,
		visited: &mut HashSet<String>,
		paths: &mut HashMap<String, String>,
		salt_paths: &mut HashMap<String, String>,
		outputs: &mut Vec<String>,
	) {
		if !visited.insert(config_path.to_owned()) {
			return;
		}
		if let Some(next) = references.get(config_path) {
			for reference in next {
				self.visit_reference(reference, references, visited, paths, salt_paths, outputs);
			}
		}
		if let Some(info) = self.infos.get(config_path) {
			paths.extend(info.paths.iter().map(|(key, value)| (key.clone(), value.clone())));
			salt_paths.extend(info.salt_paths.iter().map(|(key, value)| (key.clone(), value.clone())));
			outputs.extend(info.outputs.iter().cloned());
		}
	}
}

/// `newProjectReferenceInfo`
fn new_project_reference_info(project: &SolutionProject) -> ProjectReferenceInfo {
	let mut info = ProjectReferenceInfo {
		config: None,
		err: String::new(),
		write_roots: Vec::new(),
		paths: HashMap::new(),
		salt_paths: HashMap::new(),
		outputs: Vec::new(),
		emits: false,
	};
	let config = match parse_project_config(&project.config_path) {
		Ok(config) => Arc::new(config),
		Err(failure) => {
			info.err = if failure.diagnostics.is_empty() {
				failure.error.to_string()
			} else {
				failure.messages().join("; ")
			};
			return info;
		}
	};
	let options = config.options();
	if !options.out_dir.is_empty() {
		let translator = create_path_translator(&*config, !project.options.lua_extension);
		append_canonical_write_root(&mut info.write_roots, &translator.out_dir);
		if options.get_emit_declarations() && !options.declaration_dir.is_empty() {
			append_canonical_write_root(&mut info.write_roots, &gopath::from_slash(&options.declaration_dir));
		}
	}
	if project.options.emit_include_files
		&& let Ok(include_path) =
			resolve_include_path(&gopath::dir(&project.config_path), &project.options.include_path)
	{
		append_canonical_write_root(&mut info.write_roots, &include_path);
	}
	if project_root_dirs(&*config).is_empty() || options.out_dir.is_empty() {
		info.config = Some(config);
		return info;
	}
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let translator = create_path_translator(&*config, !project.options.lua_extension);
	info.emits = true;
	for file_name in config.file_names() {
		if tspath::is_declaration_file_name(file_name)
			&& !is_path_descendant_of(&gopath::from_slash(file_name), &translator.root_dir)
		{
			continue;
		}
		let import_path = translator.get_import_path(file_name, false);
		info.outputs.push(import_path.clone());
		info.paths
			.insert(canonical_file_name(file_name, case_sensitive), import_path.clone());
		info.salt_paths
			.insert(normalize_source_file_path(file_name), import_path.clone());
		#[expect(
			clippy::case_sensitive_file_extension_comparisons,
			reason = "Go's strings.HasSuffix is case-sensitive"
		)]
		let json = file_name.ends_with(".json");
		if !tspath::is_declaration_file_name(file_name) && !json {
			let declaration_path = outputpaths::get_output_declaration_file_name_worker(file_name, &options, &*config);
			info.paths.insert(
				canonical_file_name(&declaration_path, case_sensitive),
				import_path.clone(),
			);
			info.salt_paths
				.insert(normalize_source_file_path(&declaration_path), import_path);
		}
	}
	add_project_real_path_aliases(&mut info.paths, &gopath::dir(&project.config_path), case_sensitive);
	info.config = Some(config);
	info
}

impl ProjectReferenceInfo {
	/// `equal`: everything the solution metadata reads, and the options the validations check.
	fn equal(&self, other: Option<&Self>) -> bool {
		let Some(other) = other else {
			return false;
		};
		if self.err != other.err
			|| self.emits != other.emits
			|| self.write_roots != other.write_roots
			|| self.outputs != other.outputs
			|| self.paths != other.paths
			|| self.salt_paths != other.salt_paths
			|| self.config.is_none() != other.config.is_none()
		{
			return false;
		}
		match (&self.config, &other.config) {
			(Some(left), Some(right)) => *left.options() == *right.options(),
			_ => true,
		}
	}
}

/// `projectRestoredDeclarations`: the declaration outputs a satisfied project must have restored.
/// It builds the program, since the emitted set includes sources only imports reach.
pub(crate) fn project_restored_declarations(project: &SolutionProject) -> Result<Vec<String>, Error> {
	let dir = gopath::dir(&project.config_path);
	let options = crate::project::ProjectOptions {
		ts_config_path: project.config_path.clone(),
		..crate::project::ProjectOptions::default()
	};
	let (_, program) =
		crate::project::new_project_program_with_options(&dir, &project.config_path, &options).map_err(|failure| {
			if failure.diagnostics.is_empty() {
				failure.error
			} else {
				Error::message(failure.messages().join("; "))
			}
		})?;
	let options = program.options();
	if options.out_dir.is_empty() || !options.get_emit_declarations() {
		return Ok(Vec::new());
	}
	Ok(project_source_files(program)
		.iter()
		.map(|file| {
			gopath::from_slash(&outputpaths::get_output_declaration_file_name_worker(
				file.file_name(),
				&options,
				program,
			))
			.into_owned()
		})
		.collect())
}

/// `appendCanonicalWriteRoot`
fn append_canonical_write_root(roots: &mut Vec<String>, path: &str) {
	let canonical = canonical_write_root(path);
	if !roots.contains(&canonical) {
		roots.push(canonical);
	}
}

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

/// `canonicalWriteRoot`: the absolute, symlink-resolved spelling of `path`, resolving its
/// longest existing ancestor when the path does not exist yet.
pub(crate) fn canonical_write_root(path: &str) -> String {
	let mut path = gopath::clean(&gopath::from_slash(path));
	if let Ok(absolute) = gopath::abs(&path) {
		path = gopath::clean(&absolute);
	}
	let mut missing: Vec<String> = Vec::new();
	loop {
		if let Ok(mut resolved) = fsutil::eval_symlinks(&path) {
			for part in missing.iter().rev() {
				resolved = gopath::join(&[&resolved, part]);
			}
			return gopath::clean(&resolved);
		}
		let parent = gopath::dir(&path);
		if parent == path {
			return path;
		}
		missing.push(gopath::base(&path));
		path = parent;
	}
}

/// `deriveWaitOnlyDependencies`: a later project waits for every earlier one whose write roots
/// overlap its own.
pub(crate) fn derive_wait_only_dependencies(
	projects: &[SolutionProject],
	write_roots: &HashMap<String, Vec<String>>,
) -> HashMap<String, Vec<String>> {
	let empty = Vec::new();
	let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
	for later_index in 1..projects.len() {
		let later = &projects[later_index];
		for earlier in &projects[..later_index] {
			if write_roots_overlap(
				write_roots.get(&earlier.config_path).unwrap_or(&empty),
				write_roots.get(&later.config_path).unwrap_or(&empty),
			) {
				dependencies
					.entry(later.config_path.clone())
					.or_default()
					.push(earlier.config_path.clone());
			}
		}
	}
	dependencies
}

/// `writeRootsOverlap`
pub(crate) fn write_roots_overlap(left: &[String], right: &[String]) -> bool {
	left.iter().any(|left_root| {
		right
			.iter()
			.any(|right_root| write_root_contains(left_root, right_root) || write_root_contains(right_root, left_root))
	})
}

/// `writeRootContains`
fn write_root_contains(root: &str, path: &str) -> bool {
	let (root, path) = if rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names() {
		(root.to_owned(), path.to_owned())
	} else {
		(root.to_lowercase(), path.to_lowercase())
	};
	within(&root, &path)
}

/// A `filepath.Rel` that neither fails nor climbs out.
fn within(root: &str, path: &str) -> bool {
	gopath::rel(root, path)
		.is_ok_and(|relative| relative != ".." && !relative.starts_with(&format!("..{}", gopath::NATIVE.separator())))
}

/// `pathWithinAnyRoot`
pub(crate) fn path_within_any_root(file_name: &str, root_dirs: &[String]) -> bool {
	let file_name = gopath::from_slash(file_name);
	root_dirs
		.iter()
		.any(|root_dir| within(&gopath::from_slash(root_dir), &file_name))
}
