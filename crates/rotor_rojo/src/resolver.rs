//! Port of `@roblox-ts/rojo-resolver` 1.1.0 (vendored at
//! `reference/rojo-resolver`; all line references are into its `src/*.ts`
//! files). This code decides require paths, so it is byte-parity-critical
//! downstream and quirks are ported verbatim.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use std::sync::LazyLock;

use rotor_support::gopath;
use tsrs_core::tspath::{EncodedRootLength, get_encoded_root_length};
use tsrs_vfs::osvfs;

use crate::config::{Tree, TreeEntry, config_from_json, validate_config};
use crate::json::{self, JsonValue};
use crate::prefetch::{DirectoryListings, Entry, EntryKind, prefetch_directory_listings, read_dir};
use crate::project_files::ProjectFiles;

// Extension constants (`RojoResolver.ts` L7-19, `constants.ts`).
pub const LUA_EXT: &str = ".lua";
pub const LUAU_EXT: &str = ".luau";
const JSON_EXT: &str = ".json";
const TOML_EXT: &str = ".toml";

pub const INIT_NAME: &str = "init";

const SERVER_SUB_EXT: &str = ".server";
const CLIENT_SUB_EXT: &str = ".client";
const MODULE_SUB_EXT: &str = "";

pub const ROJO_DEFAULT_NAME: &str = "default.project.json";
const ROJO_OLD_NAME: &str = "roblox-project.json";
const ROJO_PROJECT_SUFFIX: &str = ".project.json";

fn is_rojo_module_ext(ext: &str) -> bool {
	matches!(ext, LUAU_EXT | JSON_EXT | TOML_EXT)
}

fn is_rojo_script_ext(ext: &str) -> bool {
	ext == LUAU_EXT
}

/// Whether `name` matches `^.+\.project\.json$` (`RojoResolver.ts` L47).
pub fn is_rojo_project_file(name: &str) -> bool {
	name.strip_suffix(ROJO_PROJECT_SUFFIX)
		.is_some_and(|stem| !stem.is_empty() && !stem.contains('\n'))
}

/// Classifies an output file (`RojoResolver.ts` L51-56).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RbxType {
	/// A `ModuleScript`.
	ModuleScript,
	/// A server `Script` (`.server`).
	Script,
	/// A `LocalScript` (`.client`).
	LocalScript,
	/// A script with an unrecognized sub-extension.
	Unknown,
}

/// Describes isolated-container containment of an import edge
/// (`RojoResolver.ts` L87-92).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileRelation {
	/// Outside to outside: an absolute import.
	OutToOut,
	/// Outside to inside: an error.
	OutToIn,
	/// Inside to outside: an absolute import.
	InToOut,
	/// Inside to inside: a relative import.
	InToIn,
}

/// Describes server-only and client-only containers (`RojoResolver.ts`
/// L94-98).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkType {
	/// Neither server nor client only.
	Unknown,
	/// Client-only.
	Client,
	/// Server-only.
	Server,
}

/// A Roblox instance tree path (`RojoResolver.ts` L79).
pub type RbxPath = Vec<String>;

/// One segment of a [`RelativeRbxPath`]: either the parent sentinel or an
/// instance name (upstream `string | RbxPathParent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelativeRbxPathSegment {
	/// The `Parent` sentinel (`RojoResolver.ts` L160).
	Parent,
	/// A child by name.
	Name(String),
}

/// The result of [`relative`] (`RojoResolver.ts` L80).
pub type RelativeRbxPath = Vec<RelativeRbxPathSegment>;

/// Maps a filesystem subtree to an rbx path (`RojoResolver.ts` L82-85).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartitionInfo {
	/// Where the subtree lands in the instance tree.
	pub rbx_path: RbxPath,
	/// The subtree's filesystem root.
	pub fs_path: String,
}

/// Records one exact filesystem-to-Roblox mapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolverFilePathState {
	/// The mapped file.
	pub path: String,
	/// Where the file lands in the instance tree.
	pub rbx_path: RbxPath,
}

/// A deterministic, serializable snapshot of a [`RojoResolver`]. Partition
/// order is unchanged because it determines lookup precedence; exact file
/// mappings are sorted by path because their source is a map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolverState {
	/// Warnings accumulated while parsing configs.
	pub warnings: Vec<String>,
	/// The current rbx path (empty once a resolver is built).
	pub rbx_path: RbxPath,
	/// The partitions, front first.
	pub partitions: Vec<PartitionInfo>,
	/// The exact file mappings, sorted by path.
	pub file_paths: Vec<ResolverFilePathState>,
	/// The isolated containers.
	pub isolated_containers: Vec<RbxPath>,
	/// Whether the tree declares `$className` `DataModel`.
	pub is_game: bool,
	/// Every project file the walk read, sorted.
	pub walked_configs: Vec<String>,
	/// Every directory the walk listed, sorted.
	pub walked_directories: Vec<String>,
}

/// Returned by [`RojoResolver::from_state`] for a snapshot that no resolver
/// could have produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidResolverState(String);

impl std::fmt::Display for InvalidResolverState {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.0)
	}
}

impl std::error::Error for InvalidResolverState {}

/// Resolves filesystem paths to Roblox instance tree paths from a Rojo
/// project configuration.
#[derive(Clone, Debug, Default)]
pub struct RojoResolver {
	warnings: Vec<String>,
	rbx_path: RbxPath,
	partitions: Vec<PartitionInfo>,
	file_path_to_rbx_path_map: HashMap<String, RbxPath>,
	isolated_containers: Vec<RbxPath>,
	walked_configs: Vec<String>,
	walked_directories: Vec<String>,
	/// Whether the tree declares `$className` `DataModel`.
	pub is_game: bool,
}

/// The state that only matters while a resolver is being built.
struct Walker<'a> {
	resolver: RojoResolver,
	active_directories: HashSet<String>,
	/// Package imports retain default-project composition, but an unrelated
	/// named project must not take precedence over the package's output path.
	ignore_named_projects: bool,
	project_files: Option<&'a ProjectFiles>,
	/// When set, answers the walk's directory reads from a parallel prefetch
	/// of the same tree.
	listings: Option<DirectoryListings>,
}

fn rbx_paths(paths: &[&[&str]]) -> Vec<RbxPath> {
	paths
		.iter()
		.map(|path| path.iter().map(|&segment| segment.to_owned()).collect())
		.collect()
}

// Isolated and network container tables (`RojoResolver.ts` L64-74).
static DEFAULT_ISOLATED_CONTAINERS: LazyLock<Vec<RbxPath>> = LazyLock::new(|| {
	rbx_paths(&[
		&["StarterPack"],
		&["StarterGui"],
		&["StarterPlayer", "StarterPlayerScripts"],
		&["StarterPlayer", "StarterCharacterScripts"],
		&["StarterPlayer", "StarterCharacter"],
		&["PluginDebugService"],
	])
});
static CLIENT_CONTAINERS: LazyLock<Vec<RbxPath>> =
	LazyLock::new(|| rbx_paths(&[&["StarterPack"], &["StarterGui"], &["StarterPlayer"]]));
static SERVER_CONTAINERS: LazyLock<Vec<RbxPath>> =
	LazyLock::new(|| rbx_paths(&[&["ServerStorage"], &["ServerScriptService"]]));

/// Finds the Rojo config for a project directory: `default.project.json` wins,
/// else candidates by name with a warning when several exist
/// (`RojoResolver.ts` L164-183). The path is `None` when none is found.
///
/// Upstream `readdirSync` returns OS order; `read_dir` here sorts by name,
/// which makes candidate selection deterministic.
#[must_use]
pub fn find_rojo_config_file_path(project_path: &str) -> (Option<String>, Vec<String>) {
	let mut warnings = Vec::new();

	let default_path = gopath::join(&[project_path, ROJO_DEFAULT_NAME]);
	if path_exists(&default_path) {
		return (Some(default_path), warnings);
	}

	let mut candidates = Vec::new();
	if let Ok(entries) = read_dir(project_path) {
		for entry in entries {
			let file_name = entry.name;
			if file_name != ROJO_DEFAULT_NAME && (file_name == ROJO_OLD_NAME || is_rojo_project_file(&file_name)) {
				candidates.push(gopath::join(&[project_path, &file_name]));
			}
		}
	}

	if candidates.len() > 1 {
		warnings.push(format!("Multiple *.project.json files found, using {}", candidates[0]));
	}
	(candidates.into_iter().next(), warnings)
}

/// What can go wrong in [`parse_project_file`].
#[derive(Debug)]
pub enum ParseProjectFileError {
	/// The file could not be read; the text is Go's `open <path>: <error>`.
	Read(String),
	/// The file is not a valid Rojo project.
	Invalid(String),
}

impl std::fmt::Display for ParseProjectFileError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Read(text) => f.write_str(text),
			Self::Invalid(message) => write!(f, "invalid Rojo configuration: {message}"),
		}
	}
}

impl std::error::Error for ParseProjectFileError {}

/// Reads and parses a Rojo project file into its top-level instance name and
/// tree.
///
/// This is the same order-preserving parse and validation
/// [`RojoResolver::from_path`] uses. It is public so callers that need the raw project tree, such as a
/// native instance-tree builder, can reuse it without constructing a resolver.
///
/// # Errors
///
/// Returns [`ParseProjectFileError::Read`] when the file cannot be read and
/// [`ParseProjectFileError::Invalid`] when it fails validation.
pub fn parse_project_file(rojo_config_file_path: &str) -> Result<(String, Tree), ParseProjectFileError> {
	let data = fs::read(rojo_config_file_path).map_err(|err| {
		ParseProjectFileError::Read(rotor_support::gostr::path_error("open", rojo_config_file_path, &err))
	})?;
	let config_json = json::parse(&data);
	validate_config(&config_json).map_err(ParseProjectFileError::Invalid)?;
	Ok(config_from_json(&config_json))
}

impl RojoResolver {
	fn new() -> Self {
		Self {
			isolated_containers: DEFAULT_ISOLATED_CONTAINERS.clone(),
			..Self::default()
		}
	}

	/// Warnings accumulated while parsing configs.
	#[must_use]
	pub fn warnings(&self) -> &[String] {
		&self.warnings
	}

	/// A deterministic, independent snapshot of the resolver.
	#[must_use]
	pub fn state(&self) -> ResolverState {
		let mut file_paths: Vec<ResolverFilePathState> = self
			.file_path_to_rbx_path_map
			.iter()
			.map(|(path, rbx_path)| ResolverFilePathState {
				path: path.clone(),
				rbx_path: rbx_path.clone(),
			})
			.collect();
		file_paths.sort_by(|a, b| a.path.cmp(&b.path));

		ResolverState {
			warnings: self.warnings.clone(),
			rbx_path: self.rbx_path.clone(),
			partitions: self.partitions.clone(),
			file_paths,
			isolated_containers: self.isolated_containers.clone(),
			is_game: self.is_game,
			walked_configs: sorted_strings(&self.walked_configs),
			walked_directories: sorted_strings(&self.walked_directories),
		}
	}

	/// Restores a resolver from a [`ResolverState`] snapshot.
	///
	/// # Errors
	///
	/// Returns [`InvalidResolverState`] when a partition or file mapping is
	/// empty, not canonical, not a Rojo module path, or duplicated.
	pub fn from_state(state: &ResolverState) -> Result<Self, InvalidResolverState> {
		validate_resolver_state(state)?;
		Ok(Self {
			warnings: state.warnings.clone(),
			rbx_path: state.rbx_path.clone(),
			partitions: state.partitions.clone(),
			file_path_to_rbx_path_map: state
				.file_paths
				.iter()
				.map(|file_path| (file_path.path.clone(), file_path.rbx_path.clone()))
				.collect(),
			isolated_containers: state.isolated_containers.clone(),
			walked_configs: sorted_strings(&state.walked_configs),
			walked_directories: sorted_strings(&state.walked_directories),
			is_game: state.is_game,
		})
	}

	/// Parses the given Rojo config file (`RojoResolver.ts` L197-201). The
	/// project name is not pushed onto rbx paths (`doNotPush`).
	#[must_use]
	pub fn from_path(rojo_config_file_path: &str) -> Self {
		let mut walker = Walker::new(None, false, None);
		let abs = gopath::abs(rojo_config_file_path).unwrap_or_else(|_| rojo_config_file_path.to_owned());
		walker.parse_config(&abs, true);
		walker.resolver
	}

	/// Creates a resolver for `ProjectType.Package` that forces all imports to
	/// be relative (`RojoResolver.ts` L203-211): one partition mapping
	/// `base_path` to the rbx root.
	#[must_use]
	pub fn synthetic(base_path: &str) -> Self {
		let mut walker = Walker::new(None, false, None);
		let tree = Tree {
			path: Some(base_path.to_owned()),
			..Tree::default()
		};
		walker.parse_tree(base_path, "", &tree, true);
		walker.resolver
	}

	/// Maps a type root for relocatable package imports. Unlike
	/// [`Self::synthetic`], it only discovers `default.project.json`; named
	/// projects still compose when explicitly referenced by a selected
	/// project's `$path`.
	#[must_use]
	pub fn package_imports(base_path: &str, project_files: &ProjectFiles) -> Self {
		let mut walker = Walker::new(Some(project_files), true, Some(prefetch_directory_listings(base_path)));
		let tree = Tree {
			path: Some(base_path.to_owned()),
			..Tree::default()
		};
		walker.parse_tree(base_path, "", &tree, true);
		walker.resolver
	}

	/// Creates a resolver from an in-memory tree (`RojoResolver.ts` L213-217).
	#[must_use]
	pub fn from_tree(base_path: &str, tree: &Tree) -> Self {
		let mut walker = Walker::new(None, false, None);
		walker.parse_tree(base_path, "", tree, true);
		walker.resolver
	}

	/// Resolves an output file path to its rbx path (`RojoResolver.ts`
	/// L316-337): exact map hit first, then the first matching partition
	/// (LIFO). Returns `None` when no Rojo data covers the file.
	#[must_use]
	pub fn get_rbx_path_from_file_path(&self, file_path: &str) -> Option<RbxPath> {
		let abs = gopath::abs(file_path).ok()?;
		let file_path = convert_to_luau(&abs);

		if let Some(rbx_path) = self.file_path_to_rbx_path_map.get(&file_path) {
			return Some(rbx_path.clone());
		}

		let ext = extname(&file_path);
		for partition in &self.partitions {
			if is_path_descendant_of(&file_path, &partition.fs_path) {
				let stripped = strip_rojo_exts(&file_path);
				let Ok(relative_path) = gopath::rel(&partition.fs_path, stripped) else {
					continue;
				};
				let mut relative_parts: Vec<&str> = if !relative_path.is_empty() && relative_path != "." {
					relative_path.split(gopath::NATIVE.separator()).collect()
				} else {
					Vec::new()
				};
				if is_rojo_script_ext(ext) && relative_parts.last() == Some(&INIT_NAME) {
					relative_parts.pop();
				}
				let mut result = Vec::with_capacity(partition.rbx_path.len() + relative_parts.len());
				result.extend(partition.rbx_path.iter().cloned());
				result.extend(relative_parts.into_iter().map(str::to_owned));
				return Some(result);
			}
		}
		None
	}

	/// Classifies a file by sub-extension (`RojoResolver.ts` L339-349).
	#[must_use]
	pub fn get_rbx_type_from_file_path(&self, file_path: &str) -> RbxType {
		let file_path = convert_to_luau(file_path);
		let ext = extname(&file_path);
		let base = gopath::base(&file_path);
		let subext = extname(base.strip_suffix(ext).unwrap_or(&base));
		if is_rojo_script_ext(ext) {
			return match subext {
				MODULE_SUB_EXT => RbxType::ModuleScript,
				SERVER_SUB_EXT => RbxType::Script,
				CLIENT_SUB_EXT => RbxType::LocalScript,
				_ => RbxType::Unknown,
			};
		}
		// Non-script exts cannot use .server, .client, etc.
		RbxType::ModuleScript
	}

	/// The first container that prefixes `rbx_path`; only games have
	/// containers (`RojoResolver.ts` L351-361).
	fn get_container<'a>(&self, from: &'a [RbxPath], rbx_path: &[String]) -> Option<&'a RbxPath> {
		if self.is_game {
			return from.iter().find(|container| array_starts_with(rbx_path, container));
		}
		None
	}

	/// Classifies an import edge against the isolated containers
	/// (`RojoResolver.ts` L363-380).
	#[must_use]
	pub fn get_file_relation(&self, file_rbx_path: &[String], module_rbx_path: &[String]) -> FileRelation {
		let file_container = self.get_container(&self.isolated_containers, file_rbx_path);
		let module_container = self.get_container(&self.isolated_containers, module_rbx_path);
		match (file_container, module_container) {
			(Some(file), Some(module)) if file == module => FileRelation::InToIn,
			(Some(_) | None, Some(_)) => FileRelation::OutToIn,
			(Some(_), None) => FileRelation::InToOut,
			(None, None) => FileRelation::OutToOut,
		}
	}

	/// Whether `rbx_path` is inside an isolated container (`RojoResolver.ts`
	/// L382-384).
	#[must_use]
	pub fn is_isolated(&self, rbx_path: &[String]) -> bool {
		self.get_container(&self.isolated_containers, rbx_path).is_some()
	}

	/// Reports server-only and client-only containment; server containers are
	/// checked before client (`RojoResolver.ts` L386-394).
	#[must_use]
	pub fn get_network_type(&self, rbx_path: &[String]) -> NetworkType {
		if self.get_container(&SERVER_CONTAINERS, rbx_path).is_some() {
			return NetworkType::Server;
		}
		if self.get_container(&CLIENT_CONTAINERS, rbx_path).is_some() {
			return NetworkType::Client;
		}
		NetworkType::Unknown
	}

	/// The partition list, front = highest precedence (`RojoResolver.ts`
	/// L420-422).
	#[must_use]
	pub fn partitions(&self) -> &[PartitionInfo] {
		&self.partitions
	}
}

fn validate_resolver_state(state: &ResolverState) -> Result<(), InvalidResolverState> {
	use rotor_support::gostr::quote;
	let invalid = |message: String| InvalidResolverState(format!("invalid resolver state: {message}"));
	for (index, partition) in state.partitions.iter().enumerate() {
		if partition.fs_path.is_empty() {
			return Err(invalid(format!("partition {index} has an empty filesystem path")));
		}
		if gopath::clean(&partition.fs_path) != partition.fs_path {
			return Err(invalid(format!(
				"partition {index} has a non-canonical filesystem path {}",
				quote(&partition.fs_path)
			)));
		}
	}

	let mut file_paths = HashSet::with_capacity(state.file_paths.len());
	for (index, file_path) in state.file_paths.iter().enumerate() {
		if file_path.path.is_empty() {
			return Err(invalid(format!("file mapping {index} has an empty path")));
		}
		if gopath::clean(&file_path.path) != file_path.path {
			return Err(invalid(format!(
				"file mapping {index} has a non-canonical path {}",
				quote(&file_path.path)
			)));
		}
		if convert_to_luau(&file_path.path) != file_path.path || !is_rojo_module_ext(extname(&file_path.path)) {
			return Err(invalid(format!(
				"file mapping {index} has an invalid Rojo module path {}",
				quote(&file_path.path)
			)));
		}
		if !file_paths.insert(file_path.path.as_str()) {
			return Err(invalid(format!("duplicate file mapping {}", quote(&file_path.path))));
		}
	}
	Ok(())
}

impl<'a> Walker<'a> {
	fn new(
		project_files: Option<&'a ProjectFiles>,
		ignore_named_projects: bool,
		listings: Option<DirectoryListings>,
	) -> Self {
		Self {
			resolver: RojoResolver::new(),
			active_directories: HashSet::new(),
			ignore_named_projects,
			project_files,
			listings,
		}
	}

	fn warn(&mut self, message: String) {
		self.resolver.warnings.push(message);
	}

	/// Reads and validates a project file and walks its tree
	/// (`RojoResolver.ts` L225-241). Where upstream's `realpathSync` would
	/// throw for a missing file, this falls through to the "Path does not
	/// exist" warning; a `JSON.parse` failure produces the "Invalid
	/// configuration!" warning (the `try`/`finally` validates `undefined`)
	/// and, unlike upstream, does not re-throw.
	fn parse_config(&mut self, rojo_config_file_path: &str, do_not_push: bool) {
		let real_path = realpath_or(rojo_config_file_path);
		self.walk_config(&real_path);
		if self.ignore_named_projects {
			self.walk_config(rojo_config_file_path);
		}
		if !path_exists(&real_path) {
			self.warn(format!("RojoResolver: Path does not exist \"{rojo_config_file_path}\""));
			return;
		}
		let config_json = self.project_files.map_or_else(
			|| Arc::new(fs::read(&real_path).map_or(JsonValue::Invalid, |data| json::parse(&data))),
			|project_files| project_files.read(&real_path),
		);
		if let Err(message) = validate_config(&config_json) {
			self.warn(format!("RojoResolver: Invalid configuration! {message}"));
			return;
		}
		let (name, tree) = config_from_json(&config_json);
		self.parse_tree(&gopath::dir(rojo_config_file_path), &name, &tree, do_not_push);
	}

	/// Walks a tree node (`RojoResolver.ts` L243-259).
	fn parse_tree(&mut self, base_path: &str, name: &str, tree: &Tree, do_not_push: bool) {
		if !do_not_push {
			self.resolver.rbx_path.push(name.to_owned());
		}

		if let Some(path) = &tree.path {
			self.parse_path(&resolve_path(base_path, path));
		}

		if tree.class_name == "DataModel" {
			self.resolver.is_game = true;
		}

		for TreeEntry { name, tree } in &tree.children {
			self.parse_tree(base_path, name, tree, false);
		}

		if !do_not_push {
			self.resolver.rbx_path.pop();
		}
	}

	/// Handles one `$path` target (`RojoResolver.ts` L261-282): module-ext
	/// files map exactly; directories containing `default.project.json` nest;
	/// all else becomes a partition, unshifted so later and deeper partitions
	/// match first.
	fn parse_path(&mut self, item_path: &str) {
		let item_path = convert_to_luau(item_path);

		// A $path that points directly at a *.project.json file composes that
		// project's tree at the current Roblox path. Its own project name is
		// not pushed, matching Rojo's nested-project mount semantics. This
		// check must precede the module-ext one because project files also
		// have a .json extension.
		if is_rojo_project_file(&gopath::base(&item_path)) {
			self.parse_config(&item_path, true);
			return;
		}

		let real_path = if path_exists(&item_path) {
			realpath_or(&item_path)
		} else {
			item_path.clone()
		};
		let ext = extname(&item_path);
		if is_rojo_module_ext(ext) {
			let rbx_path = self.resolver.rbx_path.clone();
			self.resolver.file_path_to_rbx_path_map.insert(item_path, rbx_path);
			return;
		}
		let is_directory = fs::metadata(&real_path).is_ok_and(|info| info.is_dir());
		if is_directory {
			self.walk_directory(&real_path);
			if self.ignore_named_projects {
				self.walk_directory(&item_path);
			}
		}
		if is_directory && self.dir_contains(&real_path, ROJO_DEFAULT_NAME) {
			let Some(key) = self.enter_directory(&real_path) else {
				return;
			};
			self.parse_config(&gopath::join(&[&item_path, ROJO_DEFAULT_NAME]), true);
			self.active_directories.remove(&key);
		} else {
			let partition = PartitionInfo {
				fs_path: item_path.clone(),
				rbx_path: self.resolver.rbx_path.clone(),
			};
			self.resolver.partitions.insert(0, partition);

			if is_directory {
				self.search_directory(&item_path, "", &mut HashSet::new());
			}
		}
	}

	/// Recursively discovers nested project files inside a partition
	/// (`RojoResolver.ts` L284-314). A `default.project.json` found while
	/// walking is parsed with its name pushed (`doNotPush` false), replacing
	/// the directory's own name. Each `$path` scan visits a real directory
	/// once, so only the first implicit alias discovers its nested project
	/// mappings. Independent explicit `$path` mounts retain separate
	/// discovery contexts.
	fn search_directory(&mut self, directory: &str, item: &str, visited: &mut HashSet<String>) {
		let real_path = realpath_or(directory);
		self.search_directory_at(directory, &real_path, item, visited);
	}

	/// [`Self::search_directory`] for a directory whose physical path is
	/// already known. A child that is a plain directory sits at its parent's
	/// physical path plus its name, so only links pay for a realpath.
	fn search_directory_at(&mut self, directory: &str, real_path: &str, item: &str, visited: &mut HashSet<String>) {
		if self.ignore_named_projects {
			// Retain logical junction paths too: replacing a package link
			// changes its layout even though the old physical directory still
			// exists.
			self.walk_directory(directory);
		}
		let Some(key) = self.enter_directory(real_path) else {
			return;
		};
		self.search_entered_directory(directory, real_path, item, visited, &key);
		self.active_directories.remove(&key);
	}

	fn search_entered_directory(
		&mut self,
		directory: &str,
		real_path: &str,
		item: &str,
		visited: &mut HashSet<String>,
		key: &str,
	) {
		if !visited.insert(key.to_owned()) {
			return;
		}
		self.walk_directory(real_path);
		let Some(entries) = self.read_dir(real_path) else {
			return;
		};

		if entries.iter().any(|entry| entry.name == ROJO_DEFAULT_NAME) {
			self.parse_config(&gopath::join(&[directory, ROJO_DEFAULT_NAME]), false);
			return;
		}

		if !item.is_empty() {
			self.resolver.rbx_path.push(item.to_owned());
		}

		// *.project.json (regular files). The reference stats realpathSync(child)
		// for every entry; the directory scan already carries the entry kind,
		// so only reparse points (junctions/symlinks) need a syscall, where
		// following the link is load-bearing for pnpm installs. Plain files and
		// directories, the overwhelming majority of a source tree, need no
		// extra stat.
		for entry in entries.iter() {
			let child = &entry.name;
			if self.ignore_named_projects || child == ROJO_DEFAULT_NAME || !is_rojo_project_file(child) {
				continue;
			}
			if entry_kind(directory, entry).1 {
				self.parse_config(&gopath::join(&[directory, child]), false);
			}
		}

		// folders
		for entry in entries.iter() {
			if entry_kind(directory, entry).0 {
				let child = gopath::join(&[directory, &entry.name]);
				let child_real = gopath::join(&[real_path, &entry.name]);
				if entry.kind == EntryKind::Dir {
					self.search_directory_at(&child, &child_real, &entry.name, visited);
				} else {
					let child_real = self.realpath(&child_real);
					self.search_directory_at(&child, &child_real, &entry.name, visited);
				}
			}
		}

		if !item.is_empty() {
			self.resolver.rbx_path.pop();
		}
	}

	/// Marks `real_path` as being walked, or returns `None` when it already
	/// is, which breaks directory-link cycles.
	fn enter_directory(&mut self, real_path: &str) -> Option<String> {
		let key = canonical_file_name(real_path, osvfs::fs().use_case_sensitive_file_names());
		self.active_directories.insert(key.clone()).then_some(key)
	}

	fn walk_config(&mut self, path: &str) {
		self.resolver.walked_configs.push(gopath::clean(path));
	}

	fn walk_directory(&mut self, path: &str) {
		self.resolver.walked_directories.push(gopath::clean(path));
	}

	/// The listing of `physical`, from the prefetch when there is one.
	fn read_dir(&self, physical: &str) -> Option<Arc<[Entry]>> {
		self.listings.as_ref().map_or_else(
			|| read_dir(physical).ok().map(Arc::from),
			|listings| listings.read_dir(physical),
		)
	}

	fn realpath(&self, path: &str) -> String {
		self.listings
			.as_ref()
			.map_or_else(|| realpath_or(path), |listings| listings.realpath(path))
	}

	/// `fs.readdirSync(dir).includes(name)`: an exact, case-sensitive
	/// entry-name match, answered from the prefetch when the walk has one.
	fn dir_contains(&self, dir: &str, name: &str) -> bool {
		self.read_dir(dir)
			.is_some_and(|entries| entries.iter().any(|entry| entry.name == name))
	}
}

fn sorted_strings(values: &[String]) -> Vec<String> {
	let mut sorted = values.to_vec();
	sorted.sort();
	sorted.dedup();
	sorted
}

/// Reports whether a directory entry resolves to a directory or a regular
/// file, matching the reference's `statSync(realpathSync(child))` calls. The
/// directory scan populates the entry kind itself, so the unambiguous
/// plain-file and plain-directory cases are decided for free; any reparse
/// point (a Windows junction or symlink, which pnpm relies on) falls back to a
/// following `stat` so behavior is byte-identical.
pub fn entry_kind(directory: &str, entry: &Entry) -> (bool, bool) {
	match entry.kind {
		EntryKind::Dir => (true, false),
		EntryKind::File => (false, true),
		EntryKind::Other => fs::metadata(gopath::join(&[directory, &entry.name]))
			.map_or((false, false), |info| (info.is_dir(), info.is_file())),
	}
}

/// Computes the path from `rbx_from` to `rbx_to`: one `Parent` per remaining
/// `from` segment past the first difference, then the `to` tail
/// (`RojoResolver.ts` L396-418).
#[must_use]
pub fn relative(rbx_from: &[String], rbx_to: &[String]) -> RelativeRbxPath {
	let max_length = rbx_from.len().max(rbx_to.len());
	// Out-of-range reads compare as undefined in JS: any in-range value
	// differs from it.
	let diff_index = (0..max_length)
		.find(|&i| i >= rbx_from.len() || i >= rbx_to.len() || rbx_from[i] != rbx_to[i])
		.unwrap_or(max_length);

	let mut result = Vec::new();
	if diff_index < rbx_from.len() {
		result.extend((0..rbx_from.len() - diff_index).map(|_| RelativeRbxPathSegment::Parent));
	}
	result.extend(
		rbx_to
			.iter()
			.skip(diff_index)
			.map(|name| RelativeRbxPathSegment::Name(name.clone())),
	);
	result
}

// --- helpers (`RojoResolver.ts` L100-158) ---

/// Removes a module ext and, for script exts, a `.server`/`.client`
/// sub-extension beneath it (`RojoResolver.ts` L100-112).
fn strip_rojo_exts(file_path: &str) -> &str {
	let ext = extname(file_path);
	if !is_rojo_module_ext(ext) {
		return file_path;
	}
	let stripped = &file_path[..file_path.len() - ext.len()];
	if is_rojo_script_ext(ext) {
		let subext = extname(stripped);
		if subext == SERVER_SUB_EXT || subext == CLIENT_SUB_EXT {
			return &stripped[..stripped.len() - subext.len()];
		}
	}
	stripped
}

fn array_starts_with(a: &[String], b: &[String]) -> bool {
	let min_length = a.len().min(b.len());
	a[..min_length] == b[..min_length]
}

/// Mirrors `RojoResolver.ts` L124-126, including the quirk that a direct child
/// whose name starts with `..` is treated as outside.
fn is_path_descendant_of(file_path: &str, dir_path: &str) -> bool {
	if dir_path == file_path {
		return true;
	}
	// filepath.Rel allocates a "../.." string for every non-descendant, and
	// this runs once per partition. Windows stays on it because it compares
	// paths case-insensitively there.
	#[cfg(not(windows))]
	if is_clean_absolute(file_path) && is_clean_absolute(dir_path) {
		let Some(rel) = file_path.strip_prefix(dir_path) else {
			return false;
		};
		let rel = if dir_path == "/" {
			rel
		} else {
			let Some(rel) = rel.strip_prefix('/') else {
				return false;
			};
			rel
		};
		return !rel.starts_with("..");
	}
	gopath::rel(dir_path, file_path).is_ok_and(|rel| !rel.starts_with(".."))
}

/// Whether `p` is rooted and `Clean` would leave it unchanged, which it does
/// without allocating.
#[cfg(not(windows))]
fn is_clean_absolute(p: &str) -> bool {
	p.starts_with('/') && gopath::clean_cow(p) == p
}

/// Renames a `.lua` path to `.luau` (`RojoResolver.ts` L154-158).
fn convert_to_luau(file_path: &str) -> String {
	match file_path.strip_suffix(LUA_EXT) {
		Some(stem) if extname(file_path) == LUA_EXT => format!("{stem}{LUAU_EXT}"),
		_ => file_path.to_owned(),
	}
}

/// Mirrors Node `path.extname`: the basename's last `.`-suffix, unless the dot
/// is its first character (empty for dotfiles and for `.` and `..`).
fn extname(p: &str) -> &str {
	let base = base_name(p);
	if base == ".." {
		return "";
	}
	match base.rfind('.') {
		Some(idx) if idx > 0 => &base[idx..],
		_ => "",
	}
}

/// The final element of `p` as a slice of `p`, which is what `filepath.Base`
/// returns apart from its `.` and separator special cases.
fn base_name(p: &str) -> &str {
	let trimmed = p.trim_end_matches(is_separator);
	let start = gopath::NATIVE.volume_name_len(trimmed);
	let tail = &trimmed[start..];
	let name = tail.rfind(is_separator).map_or(tail, |i| &tail[i + 1..]);
	if p.is_empty() {
		"."
	} else if name.is_empty() {
		// Only separators: Go returns the separator, which has no extension.
		"/"
	} else {
		name
	}
}

const fn is_separator(c: char) -> bool {
	c == '/' || (cfg!(windows) && c == '\\')
}

/// Mirrors Node `path.resolve(basePath, p)` for an absolute `base_path`.
fn resolve_path(base_path: &str, p: &str) -> String {
	if gopath::is_abs(p) {
		gopath::clean(p)
	} else {
		gopath::join(&[base_path, p])
	}
}

fn path_exists(p: &str) -> bool {
	fs::metadata(p).is_ok()
}

/// Mirrors the reference's `fs.realpathSync` calls (`RojoResolver.ts`
/// L226/263/285/298/307), falling back to the input on failure. Node's
/// realpath resolves Windows junctions, which is pnpm's `node_modules` link
/// strategy, but `filepath.EvalSymlinks` does not: since Go 1.23 a junction
/// is neither a symlink nor a regular directory to `Lstat`, so it fails on any
/// path through one, which silently hid packages' nested `default.project.json`
/// files from the resolver (wrong rbxPaths for pnpm installs). The tsrs `osvfs`
/// realpath is modeled on Node's `fs.realpath.native` and already returns the
/// input unchanged on failure.
pub fn realpath_or(p: &str) -> String {
	let slashed = gopath::to_slash(p);
	// The vfs asserts that its argument is rooted.
	if get_encoded_root_length(&slashed) == EncodedRootLength::Relative {
		return p.to_owned();
	}
	gopath::from_slash(&osvfs::fs().realpath(&slashed)).into_owned()
}

/// Ports `Shared/util/getCanonicalFileName.ts`: identity on a case-sensitive
/// filesystem, lowercase otherwise.
///
/// Paths are normalized to the OS separator first (upstream call sites pass
/// `path.normalize` output).
#[must_use]
pub fn canonical_file_name(file_path: &str, use_case_sensitive_file_names: bool) -> String {
	let file_path = gopath::clean(&gopath::from_slash(file_path));
	if use_case_sensitive_file_names {
		return file_path;
	}
	file_path.to_lowercase()
}

#[cfg(test)]
mod descendant_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod walk_tests;
