//! `package_layout_watch.go`: the inputs a package layout reads, for the watch to subscribe to.

use std::collections::{BTreeSet, HashMap, HashSet};

use rotor_support::gopath;

use crate::package_layout::PackageLayout;
use crate::solution::SolutionCoordinator;

/// `PackageWatchInputs`: layout inputs, never passthrough assets or solution configuration.
/// Directories need nonrecursive watches for discovery changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageWatchInputs {
	/// The files the layout read, sorted.
	pub files: Vec<String>,
	/// The directories to watch, sorted.
	pub directories: Vec<String>,
	pub(crate) fingerprint: String,
}

fn is_dir(path: &str) -> bool {
	std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn is_alias(path: &str) -> bool {
	std::fs::symlink_metadata(path).is_ok_and(|metadata| {
		let file_type = metadata.file_type();
		file_type.is_symlink() || cfg!(windows) && !(file_type.is_file() || file_type.is_dir())
	})
}

impl PackageLayout {
	/// `watchInputs`
	pub(crate) fn watch_inputs(&self, dir: &str, type_roots: &[String]) -> PackageWatchInputs {
		let mut directories = BTreeSet::new();
		let mut files = BTreeSet::new();
		let mut checked_ancestors = HashSet::new();
		let add_directory = |directories: &mut BTreeSet<String>, mut path: String| loop {
			if is_dir(&path) {
				directories.insert(path);
				return;
			}
			let parent = gopath::dir(&path);
			if parent == path {
				return;
			}
			path = parent;
		};
		let watch_aliases = |checked: &mut HashSet<String>, directories: &mut BTreeSet<String>, mut path: String| loop {
			if !checked.insert(path.clone()) {
				return;
			}
			let parent = gopath::dir(&path);
			if parent == path {
				return;
			}
			// Junctions are irregular on Windows. Watch the entry's parent: subscriptions inside
			// its old target survive retargets.
			if is_alias(&path) {
				add_directory(directories, parent.clone());
			}
			path = parent;
		};
		let add_file = |checked: &mut HashSet<String>,
		                directories: &mut BTreeSet<String>,
		                files: &mut BTreeSet<String>,
		                path: String| {
			let parent = gopath::dir(&path);
			watch_aliases(checked, directories, path.clone());
			add_directory(directories, parent.clone());
			if is_dir(&parent) {
				files.insert(path);
			}
		};
		for root in type_roots {
			let root = gopath::clean(&gopath::from_slash(root));
			watch_aliases(&mut checked_ancestors, &mut directories, root.clone());
			add_directory(&mut directories, root.clone());
			add_directory(&mut directories, gopath::dir(&root));
		}
		for resolver in &self.resolvers {
			let state = resolver.state();
			// parseConfig records explicit references before checking existence. Missing named
			// configs stay subscribed so their recreation is observed.
			for path in state.walked_configs {
				add_file(&mut checked_ancestors, &mut directories, &mut files, path);
			}
			for directory in state.walked_directories {
				watch_aliases(&mut checked_ancestors, &mut directories, directory.clone());
				add_directory(&mut directories, directory.clone());
				add_directory(&mut directories, gopath::dir(&directory));
				add_file(
					&mut checked_ancestors,
					&mut directories,
					&mut files,
					gopath::join(&[&directory, "default.project.json"]),
				);
			}
			for partition in state.partitions {
				add_file(
					&mut checked_ancestors,
					&mut directories,
					&mut files,
					gopath::join(&[&partition.fs_path, "default.project.json"]),
				);
			}
		}
		for path in self.files.paths().into_iter().chain(self.manifests.iter().cloned()) {
			add_file(&mut checked_ancestors, &mut directories, &mut files, path);
		}
		PackageWatchInputs {
			files: files.into_iter().collect(),
			directories: directories.into_iter().collect(),
			fingerprint: self.fingerprint(dir),
		}
	}
}

impl SolutionCoordinator {
	/// `RefreshPackageWatchInputs`: rechecks discovery after a package input event. An unchanged
	/// mapping can still need new subscriptions; only a semantic change needs a compiler build.
	pub fn refresh_package_watch_inputs(&mut self, project_path: &str) -> bool {
		let previous = self
			.watch_sets
			.get(project_path)
			.and_then(|set| set.package_inputs.clone());
		let Some(project) = self
			.graph
			.projects
			.iter()
			.find(|project| project.config_path == project_path)
			.cloned()
		else {
			return false;
		};
		let set = self.project_watch_set(&project, &mut HashMap::new());
		let current = set.package_inputs.clone();
		self.watch_sets.insert(project_path.to_owned(), set);
		match (previous, current) {
			(Some(previous), Some(current)) => previous.fingerprint != current.fingerprint,
			(None, None) => false,
			_ => true,
		}
	}
}
