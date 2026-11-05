//! Parallel directory reads for a resolver walk.
//!
//! A package build walks every installed package under its type roots, and
//! nearly all of that time is `read_dir` and realpath syscalls that do not
//! depend on one another. The walk itself stays sequential, since its
//! partition and path order is the output, so the reads are fetched ahead of
//! it in parallel.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rotor_support::gopath;

use crate::resolver::{ROJO_DEFAULT_NAME, realpath_or};

/// What a directory entry is, decided without following links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
	/// A directory (not a link to one).
	Dir,
	/// A regular file.
	File,
	/// A symlink, junction, or anything else that needs a following `stat`.
	Other,
}

/// One entry of a directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
	/// The entry's file name.
	pub name: String,
	/// What the directory scan said the entry is.
	pub kind: EntryKind,
}

/// `os.ReadDir`: the entries of `path` sorted by file name.
pub fn read_dir(path: &str) -> io::Result<Vec<Entry>> {
	let mut entries = Vec::new();
	for entry in fs::read_dir(path)? {
		let entry = entry?;
		let file_type = entry.file_type()?;
		let kind = if file_type.is_dir() {
			EntryKind::Dir
		} else if file_type.is_file() {
			EntryKind::File
		} else {
			EntryKind::Other
		};
		entries.push(Entry {
			name: entry.file_name().to_string_lossy().into_owned(),
			kind,
		});
	}
	entries.sort_by(|a, b| a.name.cmp(&b.name));
	Ok(entries)
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The directory reads one resolver walk will make, fetched ahead of it in
/// parallel.
#[derive(Default)]
pub struct DirectoryListings {
	/// Physical directory to its listing; `None` records a failed read.
	entries: Mutex<HashMap<String, Option<Arc<[Entry]>>>>,
	/// Logical link path to its physical path.
	realpaths: Mutex<HashMap<String, String>>,
}

/// Reads the tree under `root` the way the walk's directory search will:
/// every directory down to the first one holding a `default.project.json`,
/// following links to directories.
#[must_use]
pub fn prefetch_directory_listings(root: &str) -> DirectoryListings {
	let listings = DirectoryListings::default();
	let visited = Mutex::new(HashSet::new());
	rayon::scope(|scope| visit(scope, &listings, &visited, realpath_or(root)));
	listings
}

fn visit<'scope>(
	scope: &rayon::Scope<'scope>,
	listings: &'scope DirectoryListings,
	visited: &'scope Mutex<HashSet<String>>,
	physical: String,
) {
	if !locked(visited).insert(physical.clone()) {
		return;
	}
	scope.spawn(move |scope| {
		let entries = read_dir(&physical).ok().map(Arc::<[Entry]>::from);
		locked(&listings.entries).insert(physical.clone(), entries.clone());
		let Some(entries) = entries else {
			return;
		};
		if entries.iter().any(|entry| entry.name == ROJO_DEFAULT_NAME) {
			return;
		}
		let mut links = Vec::new();
		for entry in entries.iter() {
			match entry.kind {
				EntryKind::Dir => visit(scope, listings, visited, gopath::join(&[&physical, &entry.name])),
				EntryKind::File => {}
				EntryKind::Other => links.push(entry),
			}
		}
		for entry in links {
			let child = gopath::join(&[&physical, &entry.name]);
			if !fs::metadata(&child).is_ok_and(|info| info.is_dir()) {
				continue;
			}
			let real = realpath_or(&child);
			locked(&listings.realpaths).insert(child, real.clone());
			visit(scope, listings, visited, real);
		}
	});
}

impl DirectoryListings {
	/// The listing of `physical`, from the prefetch when it covered it.
	pub fn read_dir(&self, physical: &str) -> Option<Arc<[Entry]>> {
		if let Some(cached) = locked(&self.entries).get(physical) {
			return cached.clone();
		}
		read_dir(physical).ok().map(Arc::<[Entry]>::from)
	}

	/// Answers for a link the prefetch followed. The prefetch only saw links
	/// under physical directories, so the logical path has to match one.
	pub fn realpath(&self, path: &str) -> String {
		if let Some(real) = locked(&self.realpaths).get(path) {
			return real.clone();
		}
		realpath_or(path)
	}
}
