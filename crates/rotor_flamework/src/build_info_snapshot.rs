//! `build_info_snapshot.go`: detached copies of a build info and its packages.

use crate::build_info::{BuildInfo, BuildInfoSnapshot};
use crate::build_info_clone::{clone_build_class, clone_metadata};

impl BuildInfo {
	/// `Snapshot`
	#[must_use]
	pub fn snapshot(&self) -> BuildInfoSnapshot {
		BuildInfoSnapshot {
			path: self.path.clone(),
			version: self.version,
			flamework_version: self.flamework_version.clone(),
			prefix: self.prefix.clone(),
			salt: self.salt.clone(),
			metadata: clone_metadata(self.metadata.as_ref()),
			string_hashes: self.string_hashes.as_ref().map(|hashes| hashes.values.clone()),
			identifiers: self.identifiers.values.clone(),
			classes: self
				.classes
				.as_ref()
				.map(|classes| classes.iter().map(clone_build_class).collect()),
		}
	}

	/// `PackageSnapshots`: every package, depth first.
	#[must_use]
	pub fn package_snapshots(&self) -> Vec<BuildInfoSnapshot> {
		let mut snapshots = Vec::new();
		for child in &self.packages {
			snapshots.push(child.snapshot());
			snapshots.extend(child.package_snapshots());
		}
		snapshots
	}
}
