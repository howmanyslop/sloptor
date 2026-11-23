//! Not a Go file: each output's stat fields beside its content hash.
//!
//! Validating the previous build's output hashes (`validatedOutputHashes`) reads only the files
//! whose stat changed; Go reads and hashes every output on every build. A file whose size,
//! modification and status-change/creation times, and inode/attributes all match the recorded ones
//! holds the recorded content. The cache lives in `.rotor/cache`, is a pure accelerator, and is
//! trusted on Unix and Windows; elsewhere every output is read.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::sync::Mutex;

use rotor_support::gopath;
use rustc_hash::FxHashMap;

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
	/// The stamp of a regular file's metadata; `None` off Unix and Windows.
	#[cfg(unix)]
	#[expect(clippy::unnecessary_wraps, reason = "the stamp is `None` off Unix and Windows")]
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

	/// The stamp of a regular file's metadata; `None` off Unix and Windows.
	#[cfg(windows)]
	#[expect(clippy::unnecessary_wraps, reason = "the stamp is `None` off Unix and Windows")]
	pub(crate) fn of(metadata: &fs::Metadata) -> Option<Self> {
		use std::os::windows::fs::MetadataExt;
		Some(Self {
			len: metadata.file_size(),
			mtime: i128::from(metadata.last_write_time()),
			ctime: i128::from(metadata.creation_time()),
			ino: u64::from(metadata.file_attributes()),
			dev: 0,
		})
	}

	/// The stamp of a regular file's metadata; `None` off Unix and Windows.
	#[cfg(not(any(unix, windows)))]
	pub(crate) const fn of(_metadata: &fs::Metadata) -> Option<Self> {
		None
	}
}

/// 20 ms in 100-ns NTFS FILETIME units, covering Windows's ~15.625 ms timer tick granularity.
#[cfg(any(windows, test))]
const WINDOWS_RACY_MARGIN_100NS: i128 = 200_000;

const fn is_racy_with_margin(mtime: i128, ctime: i128, written: i128, margin: i128) -> bool {
	mtime.saturating_add(margin) >= written || ctime.saturating_add(margin) >= written
}

impl FileStamp {
	/// Git's "racily clean" test: whether the file may have changed again since this stamp
	/// without changing it. A file written in the same timestamp tick as the stamps file, or
	/// later, can be rewritten with the same size and times; `written` is the stamps file's mtime,
	/// from the same clock and at the same granularity as the file's times.
	const fn racy(&self, written: i128) -> bool {
		#[cfg(windows)]
		{
			is_racy_with_margin(self.mtime, self.ctime, written, WINDOWS_RACY_MARGIN_100NS)
		}
		#[cfg(not(windows))]
		{
			is_racy_with_margin(self.mtime, self.ctime, written, 0)
		}
	}
}

/// The output stamps a build read and the ones it observed.
#[derive(Default)]
pub(crate) struct OutputStamps {
	loaded: FxHashMap<String, (FileStamp, String)>,
	observed: Mutex<FxHashMap<String, (FileStamp, String)>>,
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
		let mut loaded = FxHashMap::default();
		let path = stamps_path(project_dir);
		let Some(written) = fs::metadata(&path).ok().and_then(|metadata| FileStamp::of(&metadata)) else {
			return Self::default();
		};
		if let Ok(text) = fs::read_to_string(&path) {
			for line in text.lines() {
				let mut parts = line.splitn(7, '\t');
				let (Some(hash), Some(len), Some(mtime), Some(ctime), Some(ino), Some(dev), Some(key), None) = (
					parts.next(),
					parts.next(),
					parts.next(),
					parts.next(),
					parts.next(),
					parts.next(),
					parts.next(),
					parts.next(),
				) else {
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
	#[cfg(all(test, unix))]
	pub(crate) fn observe(&self, key: &str, stamp: Option<FileStamp>, hash: &str) {
		if let Some(stamp) = stamp {
			lock(&self.observed).insert(key.to_owned(), (stamp, hash.to_owned()));
		}
	}

	/// Records a batch of `(key, stamp, hash)` observations under a single lock acquisition.
	pub(crate) fn observe_batch<I>(&self, entries: I)
	where
		I: IntoIterator<Item = (String, FileStamp, String)>,
	{
		let mut observed = lock(&self.observed);
		for (key, stamp, hash) in entries {
			observed.insert(key, (stamp, hash));
		}
	}

	/// Writes the stamps observed for `outputs` (manifest key to hash), unless they are the ones
	/// loaded. A failure leaves the old file, which the next build checks file by file anyway.
	pub(crate) fn persist(&self, project_dir: &str, outputs: &BTreeMap<String, String>) {
		let observed = lock(&self.observed);
		let mut text = String::with_capacity(outputs.len() * 140);
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
			unchanged &= self
				.loaded
				.get(key)
				.is_some_and(|(loaded_stamp, loaded_hash)| loaded_stamp == stamp && loaded_hash == hash);
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

// Uses Unix nanosecond timestamps; Windows FILETIME margin behavior is covered in `racy_margin_tests`.
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

#[cfg(test)]
mod racy_margin_tests {
	use super::{WINDOWS_RACY_MARGIN_100NS, is_racy_with_margin};

	#[test]
	fn racy_with_margin_accounts_for_tick_granularity() {
		let written = 1_000_000;
		assert!(!is_racy_with_margin(written - 1, written - 1, written, 0));
		assert!(is_racy_with_margin(written, written - 1, written, 0));
		assert!(is_racy_with_margin(written - 1, written, written, 0));

		// Within 20 ms of `written` is treated as racy on Windows; strictly older is trusted.
		assert!(is_racy_with_margin(
			written - WINDOWS_RACY_MARGIN_100NS,
			written - WINDOWS_RACY_MARGIN_100NS - 1,
			written,
			WINDOWS_RACY_MARGIN_100NS,
		));
		assert!(!is_racy_with_margin(
			written - WINDOWS_RACY_MARGIN_100NS - 1,
			written - WINDOWS_RACY_MARGIN_100NS - 1,
			written,
			WINDOWS_RACY_MARGIN_100NS,
		));
	}
}
