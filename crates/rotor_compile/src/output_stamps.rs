//! Not a Go file: each output's stat fields beside its content hash.
//!
//! Validating the previous build's output hashes (`validatedOutputHashes`) reads only the files
//! whose stat changed; Go reads and hashes every output on every build. A file whose size,
//! modification and status-change times, inode and device all match the recorded ones holds the
//! recorded content: any write updates the status-change time, which no tool can set. The cache
//! lives in `.rotor/cache`, is a pure accelerator, and is only trusted on Unix; elsewhere every
//! output is read.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::sync::Mutex;

use rotor_support::gopath;

use crate::project::lock;

/// The stat fields of one file that any content change updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
	len: u64,
	mtime: i128,
	ctime: i128,
	ino: u64,
	dev: u64,
}

impl FileStamp {
	/// The stamp of a regular file's metadata; `None` off Unix.
	#[cfg(unix)]
	#[expect(clippy::unnecessary_wraps, reason = "the stamp is `None` off Unix")]
	pub(crate) fn of(metadata: &fs::Metadata) -> Option<Self> {
		use std::os::unix::fs::MetadataExt;
		Some(Self {
			len: metadata.len(),
			mtime: i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec()),
			ctime: i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec()),
			ino: metadata.ino(),
			dev: metadata.dev(),
		})
	}

	/// The stamp of a regular file's metadata; `None` off Unix.
	#[cfg(not(unix))]
	pub(crate) const fn of(_metadata: &fs::Metadata) -> Option<Self> {
		None
	}
}

/// The output stamps a build read and the ones it observed.
#[derive(Default)]
pub(crate) struct OutputStamps {
	loaded: HashMap<String, (FileStamp, String)>,
	observed: Mutex<HashMap<String, (FileStamp, String)>>,
}

/// The cache file under the project directory.
fn stamps_path(project_dir: &str) -> String {
	gopath::join(&[project_dir, ".rotor", "cache", "output-stamps"])
}

impl OutputStamps {
	/// The stamps the last build recorded in `project_dir`; none when the file is missing or
	/// malformed.
	pub(crate) fn load(project_dir: &str) -> Self {
		let mut loaded = HashMap::new();
		if let Ok(text) = fs::read_to_string(stamps_path(project_dir)) {
			for line in text.lines() {
				let fields: Vec<&str> = line.splitn(7, '\t').collect();
				let [hash, len, mtime, ctime, ino, dev, key] = fields[..] else {
					return Self::default();
				};
				let (Ok(len), Ok(mtime), Ok(ctime), Ok(ino), Ok(dev)) =
					(len.parse(), mtime.parse(), ctime.parse(), ino.parse(), dev.parse())
				else {
					return Self::default();
				};
				loaded.insert(
					key.to_owned(),
					(
						FileStamp {
							len,
							mtime,
							ctime,
							ino,
							dev,
						},
						hash.to_owned(),
					),
				);
			}
		}
		Self {
			loaded,
			observed: Mutex::default(),
		}
	}

	/// Whether `key`'s file, whose metadata is `stamp`, still holds content hashing to `hash`.
	pub(crate) fn holds(&self, key: &str, stamp: Option<FileStamp>, hash: &str) -> bool {
		stamp.is_some_and(|stamp| {
			self.loaded
				.get(key)
				.is_some_and(|(loaded, loaded_hash)| *loaded == stamp && loaded_hash == hash)
		})
	}

	/// Records that `key`'s file, whose metadata is `stamp`, holds content hashing to `hash`.
	pub(crate) fn observe(&self, key: &str, stamp: Option<FileStamp>, hash: &str) {
		if let Some(stamp) = stamp {
			lock(&self.observed).insert(key.to_owned(), (stamp, hash.to_owned()));
		}
	}

	/// Writes the stamps observed for `outputs` (manifest key to hash), unless they are the ones
	/// loaded. A failure leaves the old file, which the next build checks file by file anyway.
	pub(crate) fn persist(&self, project_dir: &str, outputs: &BTreeMap<String, String>) {
		let observed = lock(&self.observed);
		let mut text = String::new();
		let mut unchanged = true;
		let mut count = 0;
		for (key, hash) in outputs {
			let Some((stamp, observed_hash)) = observed.get(key) else {
				continue;
			};
			if observed_hash != hash {
				continue;
			}
			count += 1;
			unchanged &= self.loaded.get(key) == Some(&(*stamp, hash.clone()));
			let _ = writeln!(
				text,
				"{hash}\t{}\t{}\t{}\t{}\t{}\t{key}",
				stamp.len, stamp.mtime, stamp.ctime, stamp.ino, stamp.dev
			);
		}
		drop(observed);
		if unchanged && count == self.loaded.len() {
			return;
		}
		let path = stamps_path(project_dir);
		let _ = fs::create_dir_all(gopath::dir(&path));
		let _ = crate::envdecl::atomic_write_file(&path, text.as_bytes());
	}
}
