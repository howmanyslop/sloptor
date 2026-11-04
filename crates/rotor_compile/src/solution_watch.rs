//! `solution_watch.go`: what a solution watch subscribes to, and the coordinator operations the
//! watch loop (`rotor_cli::solution_watch`) drives: watch sets, asset drains and reloads.

use std::collections::{BTreeMap, HashMap};

use rotor_support::{fsutil, gopath};

use crate::compile::Error;
use crate::crossproject::path_within_any_root;
use crate::output::{copy_item, try_remove_output};
use crate::package_layout::prepare_package_layout;
use crate::package_layout_watch::PackageWatchInputs;
use crate::project::{
	ProjectOptions, create_path_translator, get_root_dirs, new_project_program_with_options, project_source_files,
	resolve_include_path, resolve_rojo_config_path,
};
use crate::project_config::parse_project_config;
use crate::rbxts_options::read_rbxts_options_with_chain;
use crate::reference_validation::ValidationError;
use crate::solution::{SolutionCoordinator, SolutionProject, build_solution_graph};

/// `SolutionWatchSet`: the paths one project's watch subscribes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SolutionWatchSet {
	/// The project's config.
	pub project_path: String,
	/// Every config of the project's extends chain.
	pub ts_config_paths: Vec<String>,
	/// The source root directories.
	pub root_dirs: Vec<String>,
	/// The Rojo configs the project's resolver walked.
	pub rojo_configs: Vec<String>,
	/// The directories the resolver walked, minus every project's artifacts.
	pub rojo_directories: Vec<String>,
	/// The directories the project writes.
	pub artifact_directories: Vec<String>,
	/// The package layout's inputs, for a package build.
	pub package_inputs: Option<PackageWatchInputs>,
	/// The files `$env` and `$file` read.
	pub macro_files: Vec<String>,
}

/// `WatchAssetEvent`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchAssetEvent {
	/// The changed path.
	pub path: String,
	/// Whether it was deleted.
	pub deleted: bool,
}

/// `rojoWatchState`: a Rojo config's walked paths, canonicalized once and shared by every project
/// that uses the config.
#[derive(Clone, Default)]
pub(crate) struct RojoWatchState {
	pub(crate) configs: Vec<String>,
	directories: Vec<String>,
	canonical_directories: Vec<String>,
}

impl SolutionCoordinator {
	/// `WatchSets`: the paths to watch per project, each cached until the project is invalidated
	/// or the solution reloads. Derived from the parsed config; no program is built.
	pub fn watch_sets(&mut self) -> Vec<SolutionWatchSet> {
		let mut sets = Vec::with_capacity(self.graph.projects.len() + self.graph.coordinator_config_chains.len());
		let mut rojo_states = HashMap::new();
		let projects = self.graph.projects.clone();
		for project in &projects {
			let set = if let Some(set) = self.watch_sets.get(&project.config_path) {
				set.clone()
			} else {
				let set = self.project_watch_set(project, &mut rojo_states);
				self.watch_sets.insert(project.config_path.clone(), set.clone());
				set
			};
			sets.push(set);
		}
		for chain in &self.graph.coordinator_config_chains {
			sets.push(SolutionWatchSet {
				project_path: chain.project_path.clone(),
				ts_config_paths: canonical_watch_paths(&chain.config_paths),
				..SolutionWatchSet::default()
			});
		}
		filter_solution_watch_artifacts(&mut sets, &canonical_watch_directory);
		sets
	}

	/// `projectWatchSet`: reads the cached config unless the project is invalidated and not
	/// refreshed yet. `rojo_states` shares Rojo parses between projects.
	pub(crate) fn project_watch_set(
		&mut self,
		project: &SolutionProject,
		rojo_states: &mut HashMap<String, RojoWatchState>,
	) -> SolutionWatchSet {
		let mut set = SolutionWatchSet {
			project_path: project.config_path.clone(),
			..SolutionWatchSet::default()
		};
		set.ts_config_paths = read_rbxts_options_with_chain(&project.config_path)
			.map(|(_, chain)| chain)
			.unwrap_or_default();
		if set.ts_config_paths.is_empty() {
			set.ts_config_paths = vec![project.config_path.clone()];
		}
		set.ts_config_paths = canonical_watch_paths(&set.ts_config_paths);
		let pending = self.pending_projects.contains(&project.config_path);
		let cached = self
			.metadata
			.infos
			.get(&project.config_path)
			.filter(|_| !pending)
			.and_then(|info| info.config.clone());
		let config = if cached.is_some() {
			cached
		} else {
			self.metadata.config_parses += 1;
			parse_project_config(&project.config_path).ok().map(std::sync::Arc::new)
		};
		let Some(config) = config else {
			return set;
		};
		let dir = config.get_current_directory().to_owned();
		set.root_dirs = get_root_dirs(&*config).unwrap_or_default();
		let translator = create_path_translator(&*config, !project.options.lua_extension);
		let options = crate::project::ProjectOutputHost::options(&*config);
		set.artifact_directories = solution_watch_artifact_directories(
			&dir,
			&translator.out_dir,
			&options.declaration_dir,
			&project.options.include_path,
		);
		if let Ok((Some(rojo_path), _)) = resolve_rojo_config_path(&dir, &project.options.rojo_config_path) {
			let state = rojo_states
				.entry(rojo_path.clone())
				.or_insert_with(|| new_rojo_watch_state(&self.metadata.references.resolver(&rojo_path)))
				.clone();
			set.rojo_configs.clone_from(&state.configs);
			set.rojo_directories = filter_rojo_watch_directories(&dir, &state, &set.artifact_directories);
		}
		if let Ok(Some(layout)) = prepare_package_layout(&dir, &options, &project.options) {
			set.package_inputs = Some(layout.watch_inputs(&dir, options.type_roots.as_deref().unwrap_or_default()));
		}
		set.macro_files = crate::macro_inputs::macro_watch_files(&project.config_path);
		set
	}

	/// `DrainAssets`: copies or removes the non-compiled assets the events name.
	///
	/// # Errors
	///
	/// The project's config does not parse, or a copy fails.
	pub fn drain_assets(&mut self, ts_config_path: &str, events: &[WatchAssetEvent]) -> Result<(), Error> {
		let Some(state) = self.project_state(ts_config_path) else {
			return Ok(());
		};
		let config = parse_project_config(&state.project.config_path).map_err(|failure| failure.error)?;
		let translator = create_path_translator(&config, !state.project.options.lua_extension);
		let mut asset_directories = get_root_dirs(&config).unwrap_or_default();
		let canonical_source_directories = canonical_watch_paths(&asset_directories);
		let mut artifacts = Vec::new();
		for set in self.watch_sets() {
			artifacts.extend(canonical_watch_paths(&set.artifact_directories));
			if set.project_path == state.project.config_path {
				asset_directories.extend(set.rojo_directories);
			}
		}
		let excluded_artifacts: Vec<String> = artifacts
			.into_iter()
			.filter(|artifact| {
				!canonical_source_directories
					.iter()
					.any(|source| path_within_any_root(source, std::slice::from_ref(artifact)))
			})
			.collect();
		let mut changes: BTreeMap<String, bool> = BTreeMap::new();
		for event in events {
			let canonical_path = canonical_watch_directory(&event.path);
			if path_within_any_root(&canonical_path, &excluded_artifacts) {
				continue;
			}
			changes.insert(rebase_watch_asset_path(&event.path, &asset_directories), event.deleted);
		}
		let source_map = crate::project::ProjectOutputHost::options(&config).source_map.is_true();
		for (path, deleted) in changes {
			if !path_within_any_root(&path, &asset_directories) || watch_compilable_path(&path) {
				continue;
			}
			let deleted =
				deleted || std::fs::metadata(&path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
			if deleted {
				try_remove_output(&translator, &translator.get_output_path(&path), source_map);
				continue;
			}
			copy_item(&translator, &path)?;
		}
		Ok(())
	}

	/// `ReloadForWatch`: rediscovers the graph and returns the removal of the outputs a changed
	/// Lua extension left behind.
	///
	/// # Errors
	///
	/// The graph does not build, a remaining project's config does not parse, or the reload
	/// fails validation.
	pub fn reload_for_watch(
		&mut self,
		ts_config_path: &str,
		entry: &ProjectOptions,
	) -> Result<impl FnOnce() -> Result<(), Error> + use<>, ValidationError> {
		let graph = build_solution_graph(ts_config_path, entry)?;
		let lua_extensions: HashMap<String, bool> = graph
			.projects
			.iter()
			.map(|project| (project.config_path.clone(), project.options.lua_extension))
			.collect();
		// Only a changed Lua extension leaves stale outputs, so only those projects need the
		// program that lists their emitted sources. A project gone from the new graph may have no
		// config left, so it is skipped.
		let mut previous: HashMap<String, (bool, Vec<String>)> = HashMap::new();
		for project in &self.graph.projects {
			let Some(&lua_extension) = lua_extensions.get(&project.config_path) else {
				continue;
			};
			if lua_extension == project.options.lua_extension {
				parse_project_config(&project.config_path).map_err(|failure| failure.error)?;
				continue;
			}
			let outputs = solution_watch_output_paths(project)?;
			previous.insert(project.config_path.clone(), (project.options.lua_extension, outputs));
		}
		self.reload_graph(graph, entry)?;
		let mut stale_outputs = Vec::new();
		for project in &self.graph.projects {
			let Some((lua_extension, paths)) = previous.get(&project.config_path) else {
				continue;
			};
			if *lua_extension == project.options.lua_extension {
				continue;
			}
			stale_outputs.extend(paths.iter().cloned());
		}
		stale_outputs.sort();
		Ok(move || {
			for path in stale_outputs {
				remove_if_exists(&path, "remove stale solution watch output")?;
				remove_if_exists(&format!("{path}.map"), "remove stale solution watch source map")?;
			}
			Ok(())
		})
	}
}

fn remove_if_exists(path: &str, what: &str) -> Result<(), Error> {
	match std::fs::remove_file(path) {
		Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(Error::message(format!(
			"{what} {}: {}",
			rotor_support::gostr::quote(path),
			rotor_support::gostr::path_error("remove", path, &error)
		))),
		_ => Ok(()),
	}
}

/// `filterSolutionWatchArtifacts`: an inherited Rojo config can walk another project's generated
/// files. The cached sets stay project-local; solution ownership applies to the returned Rojo
/// directories. Source roots remain explicit inputs.
pub(crate) fn filter_solution_watch_artifacts(sets: &mut [SolutionWatchSet], canonicalize: &dyn Fn(&str) -> String) {
	// Resolve repeated inherited paths once per call; a later call must see filesystem changes.
	let mut canonical_paths: HashMap<String, String> = HashMap::new();
	let mut canonical = |path: &str| -> String {
		let path = gopath::clean(&gopath::from_slash(path));
		canonical_paths
			.entry(path.clone())
			.or_insert_with(|| canonicalize(&path))
			.clone()
	};
	let mut artifacts = Vec::new();
	let mut seen = std::collections::HashSet::new();
	for set in sets.iter() {
		for path in &set.artifact_directories {
			let resolved = canonical(path);
			if seen.insert(resolved.clone()) {
				artifacts.push(resolved);
			}
		}
	}
	for set in sets.iter_mut() {
		set.rojo_directories
			.retain(|path| !path_within_any_root(&canonical(path), &artifacts));
	}
}

/// `solutionWatchArtifactDirectories`
pub(crate) fn solution_watch_artifact_directories(
	project_dir: &str,
	output_dir: &str,
	declaration_dir: &str,
	include_path: &str,
) -> Vec<String> {
	let mut directories = vec![gopath::clean(&gopath::from_slash(output_dir))];
	if !declaration_dir.is_empty() {
		append_distinct_watch_directory(&mut directories, declaration_dir);
	}
	if let Ok(include_dir) = resolve_include_path(project_dir, include_path) {
		append_distinct_watch_directory(&mut directories, &include_dir);
	}
	append_distinct_watch_directory(
		&mut directories,
		&gopath::join(&[&gopath::from_slash(project_dir), ".rotor"]),
	);
	directories
}

fn append_distinct_watch_directory(directories: &mut Vec<String>, directory: &str) {
	let directory = gopath::clean(&gopath::from_slash(directory));
	if !directories.contains(&directory) {
		directories.push(directory);
	}
}

/// `canonicalWatchDirectory`: the symlink-resolved spelling, resolving the longest existing
/// parent of a missing path. On Darwin the result is NFC, the path domain the watcher reports.
pub(crate) fn canonical_watch_directory(directory: &str) -> String {
	let directory = gopath::clean(&gopath::from_slash(directory));
	let canonical = fsutil::eval_symlinks(&directory)
		.unwrap_or_else(|_| gopath::join(&[&canonical_watch_directory_parent(&directory), &gopath::base(&directory)]));
	if cfg!(target_os = "macos") {
		return icu_normalizer::ComposingNormalizerBorrowed::new_nfc()
			.normalize(&canonical)
			.into_owned();
	}
	canonical
}

/// `canonicalWatchPaths`
pub(crate) fn canonical_watch_paths(paths: &[String]) -> Vec<String> {
	paths.iter().map(|path| canonical_watch_directory(path)).collect()
}

fn canonical_watch_directory_parent(directory: &str) -> String {
	let parent = gopath::dir(directory);
	if parent == directory {
		return directory.to_owned();
	}
	fsutil::eval_symlinks(&parent).unwrap_or_else(|_| canonical_watch_directory(&parent))
}

/// `rebaseWatchAssetPath`: the event path spelled under the asset directory that holds it.
pub(crate) fn rebase_watch_asset_path(path: &str, asset_directories: &[String]) -> String {
	let path = gopath::clean(&gopath::from_slash(path));
	let canonical_path = canonical_watch_directory(&path);
	let up = format!("..{}", gopath::NATIVE.separator());
	for directory in asset_directories {
		let Ok(relative) = gopath::rel(&canonical_watch_directory(directory), &canonical_path) else {
			continue;
		};
		if relative == ".." || relative.starts_with(&up) {
			continue;
		}
		return gopath::join(&[&gopath::clean(&gopath::from_slash(directory)), &relative]);
	}
	path
}

/// `newRojoWatchState`
pub(crate) fn new_rojo_watch_state(resolver: &rotor_rojo::RojoResolver) -> RojoWatchState {
	let state = resolver.state();
	RojoWatchState {
		configs: canonical_watch_paths(&state.walked_configs),
		canonical_directories: canonical_watch_paths(&state.walked_directories),
		directories: state.walked_directories,
	}
}

/// `filterRojoWatchDirectories`
pub(crate) fn filter_rojo_watch_directories(
	project_dir: &str,
	state: &RojoWatchState,
	artifact_directories: &[String],
) -> Vec<String> {
	let canonical_project_dir = canonical_watch_directory(project_dir);
	state
		.directories
		.iter()
		.zip(&state.canonical_directories)
		.map(|(directory, canonical)| {
			rebase_rojo_watch_directory(directory, canonical, project_dir, &canonical_project_dir)
		})
		.filter(|directory| !path_within_any_root(directory, artifact_directories))
		.collect()
}

fn rebase_rojo_watch_directory(
	directory: &str,
	canonical_directory: &str,
	project_dir: &str,
	canonical_project_dir: &str,
) -> String {
	gopath::rel(canonical_project_dir, canonical_directory).map_or_else(
		|_| gopath::clean(&gopath::from_slash(directory)),
		|relative| gopath::join(&[&gopath::clean(&gopath::from_slash(project_dir)), &relative]),
	)
}

/// `solutionWatchOutputPaths`: the output path of every emitted source of `project`.
fn solution_watch_output_paths(project: &SolutionProject) -> Result<Vec<String>, Error> {
	let (_, program) = new_project_program_with_options(
		&gopath::dir(&project.config_path),
		&project.config_path,
		&ProjectOptions::default(),
	)
	.map_err(|failure| failure.error)?;
	let translator = create_path_translator(program, !project.options.lua_extension);
	Ok(project_source_files(program)
		.iter()
		.map(|file| translator.get_output_path(file.file_name()))
		.collect())
}

/// `watchCompilablePath`
#[expect(
	clippy::case_sensitive_file_extension_comparisons,
	reason = "Go's strings.HasSuffix is case-sensitive"
)]
pub(crate) fn watch_compilable_path(path: &str) -> bool {
	path.ends_with(".d.ts") || path.ends_with(".ts") || path.ends_with(".tsx")
}
