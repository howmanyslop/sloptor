//! Cases from Go's `path/filepath` tests. Both flavors run on every host, so
//! the Windows rules are covered without a Windows machine.

use super::Flavor::{Unix, Windows};

#[test]
fn clean_unix() {
	let cases = [
		("abc", "abc"),
		("abc/def", "abc/def"),
		("a/b/c", "a/b/c"),
		(".", "."),
		("..", ".."),
		("../..", "../.."),
		("../../abc", "../../abc"),
		("/abc", "/abc"),
		("/", "/"),
		("", "."),
		("abc/", "abc"),
		("abc/def/", "abc/def"),
		("a/b/c/", "a/b/c"),
		("./", "."),
		("../", ".."),
		("../../", "../.."),
		("/abc/", "/abc"),
		("abc//def//ghi", "abc/def/ghi"),
		("abc//", "abc"),
		("abc/./def", "abc/def"),
		("/./abc/def", "/abc/def"),
		("abc/.", "abc"),
		("abc/def/ghi/../jkl", "abc/def/jkl"),
		("abc/def/../ghi/../jkl", "abc/jkl"),
		("abc/def/..", "abc"),
		("abc/def/../..", "."),
		("/abc/def/../..", "/"),
		("abc/def/../../..", ".."),
		("/abc/def/../../..", "/"),
		("abc/def/../../../ghi/jkl/../../../mno", "../../mno"),
		("/../abc", "/abc"),
		("a/../b:/../../c", "../c"),
		("abc/./../def", "def"),
		("abc//./../def", "def"),
		("abc/../../././../def", "../../def"),
	];
	for (path, want) in cases {
		assert_eq!(Unix.clean(path), want, "clean({path:?})");
		assert_eq!(Unix.clean(want), want, "clean({want:?}) is idempotent");
	}
}

#[test]
fn clean_windows() {
	let cases = [
		(r"c:", r"c:."),
		(r"c:\", r"c:\"),
		(r"c:\abc", r"c:\abc"),
		(r"c:abc\..\..\.\.\..\def", r"c:..\..\def"),
		(r"c:\abc\def\..\..", r"c:\"),
		(r"c:\..\abc", r"c:\abc"),
		(r"c:..\abc", r"c:..\abc"),
		(r"c:\b:\..\..\..\d", r"c:\d"),
		(r"\", r"\"),
		(r"/", r"\"),
		(r"\\i\..\c$", r"\\i\..\c$"),
		(r"\\i\..\i\c$", r"\\i\..\i\c$"),
		(r"\\i\..\I\c$", r"\\i\..\I\c$"),
		(r"\\host\share\foo\..\bar", r"\\host\share\bar"),
		(r"//host/share/foo/../baz", r"\\host\share\baz"),
		(r"\\host\share\foo\..\..\..\..\bar", r"\\host\share\bar"),
		(r"\\.\C:\a\..\..\c", r"\\.\C:\c"),
		(r"\\.\C:\\\\a", r"\\.\C:\a"),
		(r"\\a\b\..\c", r"\\a\b\c"),
		(r"\\a\b", r"\\a\b"),
		(r"a/../c:", r".\c:"),
		(r"a/../c:/a", r".\c:\a"),
		(r"a/../../c:", r"..\c:"),
		(r"foo:bar", r"foo:bar"),
		(r"\a\..\??\c:\x", r"\.\??\c:\x"),
		("abc/def/", r"abc\def"),
	];
	for (path, want) in cases {
		assert_eq!(Windows.clean(path), want, "clean({path:?})");
	}
}

#[test]
fn clean_reuses_clean_input() {
	for path in ["/", "/a/b", "a", ".", "../a", "/a/"] {
		let cleaned = Unix.clean_cow(path);
		assert!(
			matches!(cleaned, std::borrow::Cow::Borrowed(_)),
			"clean_cow({path:?}) allocated"
		);
	}
}

#[test]
fn join_cases() {
	let unix: [(&[&str], &str); 17] = [
		(&[], ""),
		(&[""], ""),
		(&["/"], "/"),
		(&["a"], "a"),
		(&["a", "b"], "a/b"),
		(&["a", ""], "a"),
		(&["", "b"], "b"),
		(&["/", "a"], "/a"),
		(&["/", "a/b"], "/a/b"),
		(&["/", ""], "/"),
		(&["/a", "b"], "/a/b"),
		(&["a", "/b"], "a/b"),
		(&["/a", "/b"], "/a/b"),
		(&["a/", "b"], "a/b"),
		(&["a/", ""], "a"),
		(&["", ""], ""),
		(&["/", "a", "b"], "/a/b"),
	];
	for (elems, want) in unix {
		assert_eq!(Unix.join(elems), want, "join({elems:?})");
	}
	let windows_cases: [(&[&str], &str); 30] = [
		(&["directory", "file"], r"directory\file"),
		(&[r"C:\Windows\", "System32"], r"C:\Windows\System32"),
		(&[r"C:\Windows\", ""], r"C:\Windows"),
		(&[r"C:\", "Windows"], r"C:\Windows"),
		(&["C:", "a"], "C:a"),
		(&["C:", r"a\b"], r"C:a\b"),
		(&["C:", "a", "b"], r"C:a\b"),
		(&["C:", "", "b"], "C:b"),
		(&["C:", "", "", "b"], "C:b"),
		(&["C:", ""], "C:."),
		(&["C:", "", ""], "C:."),
		(&["C:", r"\a"], r"C:\a"),
		(&["C:", "", r"\a"], r"C:\a"),
		(&["C:.", "a"], "C:a"),
		(&["C:a", "b"], r"C:a\b"),
		(&["C:a", "b", "d"], r"C:a\b\d"),
		(&[r"\\host\share", "foo"], r"\\host\share\foo"),
		(&[r"\\host\share\foo"], r"\\host\share\foo"),
		(&["//host/share", "foo/bar"], r"\\host\share\foo\bar"),
		(&[r"\"], r"\"),
		(&[r"\", ""], r"\"),
		(&[r"\", "a"], r"\a"),
		(&[r"\\", "a"], r"\\a"),
		(&[r"\", "a", "b"], r"\a\b"),
		(&[r"\\", "a", "b"], r"\\a\b"),
		(&[r"\", r"\\a\b", "c"], r"\a\b\c"),
		(&[r"\\a", "b", "c"], r"\\a\b\c"),
		(&[r"\\a\", "b", "c"], r"\\a\b\c"),
		(&["//", "a"], r"\\a"),
		(&[r"\", r"??\a"], r"\.\??\a"),
	];
	for (elems, want) in windows_cases {
		assert_eq!(Windows.join(elems), want, "join({elems:?})");
	}
	assert_eq!(Windows.join(&[r"a:\b\c", r"x\..\y:\..\..\z"]), r"a:\b\z");
}

#[test]
fn base_and_dir() {
	let base_unix = [
		("", "."),
		(".", "."),
		("/.", "."),
		("/", "/"),
		("////", "/"),
		("x/", "x"),
		("abc", "abc"),
		("abc/def", "def"),
		("a/b/.x", ".x"),
		("a/b/c.", "c."),
		("a/b/c.x", "c.x"),
	];
	for (path, want) in base_unix {
		assert_eq!(Unix.base(path), want, "base({path:?})");
	}
	let base_windows = [
		(r"c:\", r"\"),
		(r"c:.", "."),
		(r"c:\a\b", "b"),
		(r"c:a\b", "b"),
		(r"c:a\b\c", "c"),
		(r"\\host\share\", r"\"),
		(r"\\host\share\a", "a"),
		(r"\\host\share\a\b", "b"),
	];
	for (path, want) in base_windows {
		assert_eq!(Windows.base(path), want, "base({path:?})");
	}
	let dir_unix = [
		("", "."),
		(".", "."),
		("/.", "/"),
		("/", "/"),
		("/foo", "/"),
		("x/", "x"),
		("abc", "."),
		("abc/def", "abc"),
		("a/b/.x", "a/b"),
		("a/b/c.", "a/b"),
		("a/b/c.x", "a/b"),
	];
	for (path, want) in dir_unix {
		assert_eq!(Unix.dir(path), want, "dir({path:?})");
	}
	let dir_windows = [
		(r"c:\", r"c:\"),
		(r"c:.", r"c:."),
		(r"c:\a\b", r"c:\a"),
		(r"c:a\b", "c:a"),
		(r"c:a\b\c", r"c:a\b"),
		(r"\\host\share", r"\\host\share"),
		(r"\\host\share\", r"\\host\share\"),
		(r"\\host\share\a", r"\\host\share\"),
		(r"\\host\share\a\b", r"\\host\share\a"),
		(r"\\\\", r"\\\\"),
	];
	for (path, want) in dir_windows {
		assert_eq!(Windows.dir(path), want, "dir({path:?})");
	}
}

#[test]
fn is_abs_cases() {
	for (path, want) in [
		("", false),
		("/", true),
		("/usr/bin/gcc", true),
		("..", false),
		("/a/../bb", true),
		(".", false),
		("./", false),
		("lala", false),
	] {
		assert_eq!(Unix.is_abs(path), want, "is_abs({path:?})");
	}
	for (path, want) in [
		(r"C:\", true),
		(r"c\", false),
		(r"c::", false),
		(r"c:", false),
		(r"/", false),
		(r"\", false),
		(r"\Windows", false),
		(r"c:a\b", false),
		(r"c:\a\b", true),
		(r"c:/a/b", true),
		(r"\\host\share", true),
		(r"\\host\share\", true),
		(r"\\host\share\foo", true),
		(r"//host/share/foo/bar", true),
		(r"\\?\a\b\c", true),
		(r"\??\a\b\c", true),
	] {
		assert_eq!(Windows.is_abs(path), want, "is_abs({path:?})");
	}
}

#[test]
fn rel_unix() {
	let cases = [
		("a/b", "a/b", "."),
		("a/b/.", "a/b", "."),
		("a/b", "a/b/.", "."),
		("./a/b", "a/b", "."),
		("a/b", "./a/b", "."),
		("ab/cd", "ab/cde", "../cde"),
		("ab/cd", "ab/c", "../c"),
		("a/b", "a/b/c/d", "c/d"),
		("a/b", "a/b/../c", "../c"),
		("a/b/../c", "a/b", "../b"),
		("a/b/c", "a/c/d", "../../c/d"),
		("a/b", "c/d", "../../c/d"),
		("a/b/c/d", "a/b", "../.."),
		("a/b/c/d", "a/b/", "../.."),
		("a/b/c/d/", "a/b", "../.."),
		("a/b/c/d/", "a/b/", "../.."),
		("../../a/b", "../../a/b/c/d", "c/d"),
		("/a/b", "/a/b", "."),
		("/a/b/.", "/a/b", "."),
		("/a/b", "/a/b/.", "."),
		("/ab/cd", "/ab/cde", "../cde"),
		("/ab/cd", "/ab/c", "../c"),
		("/a/b", "/a/b/c/d", "c/d"),
		("/a/b", "/a/b/../c", "../c"),
		("/a/b/../c", "/a/b", "../b"),
		("/a/b/c", "/a/c/d", "../../c/d"),
		("/a/b", "/c/d", "../../c/d"),
		("/a/b/c/d", "/a/b", "../.."),
		("/a/b/c/d", "/a/b/", "../.."),
		("/a/b/c/d/", "/a/b", "../.."),
		("/a/b/c/d/", "/a/b/", "../.."),
		("/../../a/b", "/../../a/b/c/d", "c/d"),
		(".", "a/b", "a/b"),
		(".", "..", ".."),
		("", "../../.", "../.."),
	];
	for (base, target, want) in cases {
		assert_eq!(Unix.rel(base, target).as_deref(), Ok(want), "rel({base:?}, {target:?})");
	}
	for (base, target) in [("..", "."), ("..", "a"), ("../..", ".."), ("a", "/a"), ("/a", "a")] {
		let err = Unix.rel(base, target).expect_err("no lexical relative path");
		assert_eq!(err.to_string(), format!("Rel: can't make {target} relative to {base}"));
	}
}

#[test]
fn rel_windows() {
	let cases = [
		(r"C:a\b\c", "C:a/b/d", r"..\d"),
		(r"C:\Projects", r"c:\projects\src", "src"),
		(r"C:\Projects", r"c:\projects", "."),
		(r"C:\Projects\a\..", r"c:\projects", "."),
		(r"\\host\share", r"\\host\share\file.txt", "file.txt"),
		(r"\\host\share", r"\\host\share\", "."),
		("//host/share", "//host/share/", "."),
	];
	for (base, target, want) in cases {
		assert_eq!(
			Windows.rel(base, target).as_deref(),
			Ok(want),
			"rel({base:?}, {target:?})"
		);
	}
	for (base, target) in [(r"C:\", r"D:\"), ("C:", "D:")] {
		assert!(Windows.rel(base, target).is_err(), "rel({base:?}, {target:?})");
	}
}

#[test]
fn is_local() {
	for (path, want) in [
		("", false),
		(".", true),
		("..", false),
		("../a", false),
		("/", false),
		("/a", false),
		("/a/../..", false),
		("a", true),
		("a/../a", true),
		("a/", true),
		("a/.", true),
		("a/./b/./c", true),
		("a/../b:/../../c", false),
		("a/../..", false),
		("..a", true),
		("weird..name", true),
	] {
		assert_eq!(Unix.is_local(path), want, "Unix is_local({path:?})");
		assert_eq!(Windows.is_local(path), want, "Windows is_local({path:?})");
	}
	assert!(Unix.is_local(r"..\a"), "a backslash is a file name character on Unix");
	assert!(Unix.is_local("C:"), "a colon is a file name character on Unix");
	assert!(Unix.is_local("nul"), "device names are Windows's");
	for (path, want) in [
		("NUL", false),
		("nul", false),
		("nul ", false),
		("nul.", false),
		("a/nul:", false),
		("a/nul : a", false),
		("com0", true),
		("com1", false),
		("com9", false),
		("com\u{b9}", false),
		("com\u{b2}", false),
		("com\u{b3}", false),
		("com\u{b9} : a", false),
		("cOm1", false),
		("lpt1", false),
		("LPT1", false),
		("lpt\u{b3}", false),
		("./nul", false),
		(r"\", false),
		(r"\a", false),
		("C:", false),
		(r"C:\a", false),
		(r"..\a", false),
		("a/../c:", false),
		("CONIN$", false),
		("conin$", false),
		("CONOUT$", false),
		("conout$", false),
		("dollar$", true),
		("con.luau", true),
		("console", true),
	] {
		assert_eq!(Windows.is_local(path), want, "Windows is_local({path:?})");
	}
}

#[test]
fn volume_names() {
	for (path, want) in [
		("c:/foo/bar", "c:"),
		("c:", "c:"),
		("2:", "2:"),
		("", ""),
		(r"\\host", r"\\host"),
		("//host/share", r"\\host\share"),
		(r"\\host\share\foo", r"\\host\share"),
		("//host/share/foo", r"\\host\share"),
		(r"\\.\C:\foo", r"\\.\C:"),
		(r"\\?\C:\foo", r"\\?\C:"),
		("foo/bar", ""),
	] {
		assert_eq!(Windows.volume_name(path), want, "volume_name({path:?})");
	}
	assert_eq!(Unix.volume_name("c:/foo"), "");
}

#[test]
fn slash_conversion() {
	assert_eq!(Windows.from_slash("a/b\\c"), "a\\b\\c");
	assert_eq!(Windows.to_slash("a/b\\c"), "a/b/c");
	assert_eq!(Unix.from_slash("a/b\\c"), "a/b\\c");
	assert_eq!(Unix.to_slash("a/b\\c"), "a/b\\c");
}

#[cfg(windows)]
#[test]
fn abs_drops_the_verbatim_prefix() {
	assert_eq!(
		super::abs(r"\\?\C:\project\tsconfig.json").unwrap(),
		r"C:\project\tsconfig.json"
	);
}
