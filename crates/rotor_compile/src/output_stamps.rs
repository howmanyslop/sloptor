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

impl FileStamp {
	/// Git's "racily clean" test: whether the file may have changed again since this stamp
	/// without changing it. A file written in the same timestamp tick as the stamps file, or
	/// later, can be rewritten with the same size and times; `written` is the stamps file's mtime,
	/// from the same clock and at the same granularity as the file's times.
	const fn racy(&self, written: i128) -> bool {
		self.mtime >= written || self.ctime >= written
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
	///
	/// A racily clean stamp ([`FileStamp::racy`]) is left out, so the file is hashed again and the
	/// next stamps file, written later, records it.
	pub(crate) fn load(project_dir: &str) -> Self {
		let mut loaded = HashMap::new();
		let path = stamps_path(project_dir);
		let Some(written) = fs::metadata(&path).ok().and_then(|metadata| FileStamp::of(&metadata)) else {
			return Self::default();
		};
		if let Ok(text) = fs::read_to_string(&path) {
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
				let stamp = FileStamp {
					len,
					mtime,
					ctime,
					ino,
					dev,
				};
				if !stamp.racy(written.mtime) {
					loaded.insert(key.to_owned(), (stamp, hash.to_owned()));
				}
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

// `FileStamp::of` is `None` off Unix, so nothing is loaded or trusted there.
#[cfg(all(test, unix))]
mod tests {
	use std::collections::BTreeMap;
	use std::time::{Duration, SystemTime};

	use super::{FileStamp, OutputStamps, stamps_path};

	const SECOND: i128 = 1_000_000_000;

	const fn stamp(mtime: i128, ctime: i128) -> FileStamp {
		FileStamp {
			len: 6,
			mtime,
			ctime,
			ino: 7,
			dev: 8,
		}
	}

	/// A file whose mtime or ctime is not older than the stamps file may have changed again in
	/// the same timestamp tick without changing its stamp, so the next build must hash it again;
	/// an older stamp is trusted.
	#[test]
	fn load_leaves_out_racily_clean_stamps() {
		let temp = crate::tests::Temp::new();
		let dir = temp.dir();
		let written = SystemTime::UNIX_EPOCH + Duration::from_hours(500_000);
		let at = i128::from(1_800_000_000_u32) * SECOND;
		let cases = [
			("old.luau", stamp(at - SECOND, at - SECOND)),
			("same-tick-mtime.luau", stamp(at, at - SECOND)),
			("same-tick-ctime.luau", stamp(at - SECOND, at)),
			("later.luau", stamp(at + SECOND, at + SECOND)),
		];
		let stamps = OutputStamps::default();
		let mut outputs = BTreeMap::new();
		for (key, file_stamp) in cases {
			stamps.observe(key, Some(file_stamp), "hash");
			outputs.insert(key.to_owned(), "hash".to_owned());
		}
		stamps.persist(&dir, &outputs);
		let file = std::fs::File::options()
			.write(true)
			.open(stamps_path(&dir))
			.expect("the stamps file opens");
		file.set_modified(written).expect("the stamps file's mtime sets");

		let loaded = OutputStamps::load(&dir);
		let trusted: Vec<&str> = cases
			.iter()
			.filter(|(key, file_stamp)| loaded.holds(key, Some(*file_stamp), "hash"))
			.map(|(key, _)| *key)
			.collect();
		assert_eq!(trusted, ["old.luau"], "trusted stamps");
	}
}
