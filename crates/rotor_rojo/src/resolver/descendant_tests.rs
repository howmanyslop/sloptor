//! Port of `internal/rojo/resolver_descendant_test.go` (commit `52310d81` on
//! `perf/data-oriented-baseline`).

use rotor_support::gopath;

use super::is_path_descendant_of;

/// The `filepath.Rel` definition of descendance that `RojoResolver.ts` uses:
/// any relative path starting with `..` is outside.
fn rel_descendant_oracle(file_path: &str, dir_path: &str) -> bool {
	if dir_path == file_path {
		return true;
	}
	gopath::rel(dir_path, file_path).is_ok_and(|rel| !rel.starts_with(".."))
}

/// Catches a file being assigned to the wrong Rojo partition, or to none,
/// because of how its path or the partition's path happens to be spelled.
#[test]
fn is_path_descendant_of_edge_cases() {
	let cases: [(&str, &str, &str, bool); 51] = [
		("equal absolute", "/a/b", "/a/b", true),
		("equal root", "/", "/", true),
		("equal relative", "a/b", "a/b", true),
		("equal empty", "", "", true),
		("equal unclean", "/a//b", "/a//b", true),
		("child", "/a/b/c", "/a/b", true),
		("deep child", "/a/b/c/d/e.luau", "/a/b", true),
		("parent", "/a", "/a/b", false),
		("root is not inside child", "/", "/a", false),
		("child of root", "/a", "/", true),
		("deep child of root", "/a/b", "/", true),
		("dotdot-named child of root", "/..foo", "/", false),
		("dir with trailing separator", "/a/b/c", "/a/b/", true),
		("file with trailing separator", "/a/b/c/", "/a/b", true),
		("same dir with trailing separator", "/a/b", "/a/b/", true),
		("same file with trailing separator", "/a/b/", "/a/b", true),
		("root with doubled separator", "/a", "//", true),
		("dotdot-named child", "/a/b/..foo/bar", "/a/b", false),
		("bare dotdot-named child", "/a/b/..foo", "/a/b", false),
		("triple-dot child", "/a/b/...", "/a/b", false),
		("triple-dot child with tail", "/a/b/.../c", "/a/b", false),
		("dotdot-suffixed child", "/a/b/foo../bar", "/a/b", true),
		("dotdot-named grandchild", "/a/b/c/..foo", "/a/b", true),
		("dot-named child", "/a/b/.foo", "/a/b", true),
		("sibling sharing a name prefix", "/a/bc", "/a/b", false),
		("child of sibling sharing a name prefix", "/a/bc/d", "/a/b", false),
		("sibling", "/a/c", "/a/b", false),
		("different root", "/x/y", "/a/b", false),
		("different case", "a/B/c", "a/b/c", cfg!(windows)),
		("doubled separator in file", "/a//b/c", "/a/b", true),
		("doubled separator in dir", "/a/b/c", "/a//b", true),
		("dot segment in file", "/a/./b/c", "/a/b", true),
		("dot segment in dir", "/a/b/c", "/a/./b", true),
		("dotdot segment in file", "/a/x/../b/c", "/a/b", true),
		("dotdot segment in dir", "/a/b/c", "/a/x/../b", true),
		("dotdot segment leaves dir", "/a/b/../c", "/a/b", false),
		("unclean file equal to dir once cleaned", "/a/x/../b", "/a/b", true),
		("dotdot above root", "/../a", "/a", true),
		("relative child", "a/b", "a", true),
		("relative sibling sharing a name prefix", "ab/c", "a", false),
		("relative dot dir", "a", ".", true),
		("relative parent escape", "../a", ".", false),
		("relative doubled separator", "a//b", "a/b", true),
		("relative dot segment", "a/./b", "a/b", true),
		("relative dotdot segment", "a/../b", "b", true),
		("relative file under absolute dir", "a/b", "/a", false),
		("absolute file under relative dir", "/a/b", "a", false),
		("empty file", "", "/a", false),
		("empty dir with absolute file", "/a", "", false),
		("empty dir with relative file", "a", "", true),
		("empty dir with dot", ".", "", true),
	];

	for (name, file_path, dir_path, want) in cases {
		assert_eq!(
			rel_descendant_oracle(file_path, dir_path),
			want,
			"{name}: oracle({file_path:?}, {dir_path:?}) disagrees with the table"
		);
		assert_eq!(
			is_path_descendant_of(file_path, dir_path),
			want,
			"{name}: is_path_descendant_of({file_path:?}, {dir_path:?})"
		);
	}
}

/// splitmix64: any seed will do, and the sequence only has to be reproducible.
struct Random(u64);

impl Random {
	fn next(&mut self) -> u64 {
		self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
		let mut z = self.0;
		z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
		z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
		z ^ (z >> 31)
	}

	fn below(&mut self, n: usize) -> usize {
		usize::try_from(self.next() % n as u64).expect("fits usize")
	}
}

/// Catches any spelling of a path, not just the ones listed above, deciding
/// partition membership differently from the `filepath.Rel` definition.
#[test]
fn is_path_descendant_of_matches_rel() {
	let segments = [
		"", ".", "..", "...", "..foo", "foo..", ".foo", "foo", "foobar", "FOO", "b", "a b", r"a\b", "é",
	];
	let roots = ["", "/", "//"];
	let trailers = ["", "/", "/."];

	let mut shallow = Vec::new();
	for root in roots {
		for trailer in trailers {
			shallow.push(format!("{root}{trailer}"));
			for first in segments {
				shallow.push(format!("{root}{first}{trailer}"));
				for second in segments {
					shallow.push(format!("{root}{first}/{second}{trailer}"));
				}
			}
		}
	}

	let check = |file_path: &str, dir_path: &str| {
		let want = rel_descendant_oracle(file_path, dir_path);
		assert_eq!(
			is_path_descendant_of(file_path, dir_path),
			want,
			"is_path_descendant_of({file_path:?}, {dir_path:?}), oracle says {want}"
		);
	};

	for dir_path in &shallow {
		for file_path in &shallow {
			check(file_path, dir_path);
		}
	}

	// Deep pairs that share a prefix, which independent random paths rarely do.
	let mut random = Random(0x0072_6f74_6f72);
	let random_path = |random: &mut Random, max_depth: usize| {
		let parts: Vec<&str> = (0..random.below(max_depth + 1))
			.map(|_| segments[random.below(segments.len())])
			.collect();
		parts.join("/")
	};
	for _ in 0..500_000 {
		let root = roots[random.below(roots.len())];
		let dir_path = format!("{root}{}", random_path(&mut random, 6));
		let mut file_path = format!("{dir_path}/{}", random_path(&mut random, 4));
		if random.below(4) == 0 {
			file_path = format!("{root}{}{}", random_path(&mut random, 6), &file_path[root.len()..]);
		}
		check(&file_path, &dir_path);
		check(&dir_path, &file_path);
	}
}
