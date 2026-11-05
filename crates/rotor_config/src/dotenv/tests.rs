//! Port of `internal/dotenv/dotenv_test.go`.

#![expect(
	clippy::significant_drop_tightening,
	reason = "`TestEnv` holds the environment lock for the whole test on purpose"
)]

use std::collections::HashMap;
use std::fs;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rotor_support::gopath;

use super::{Env, parse};

/// Serializes the tests that change or read the process environment.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// `t.Setenv`: sets variables for the guard's lifetime, then restores them.
struct TestEnv {
	_lock: MutexGuard<'static, ()>,
	saved: Vec<(String, Option<std::ffi::OsString>)>,
}

impl TestEnv {
	fn new() -> Self {
		Self {
			_lock: ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner),
			saved: Vec::new(),
		}
	}

	#[expect(unsafe_code, reason = "tests mutate the process environment, serialized by ENV_LOCK")]
	fn set(&mut self, name: &str, value: &str) {
		self.saved.push((name.to_owned(), std::env::var_os(name)));
		// SAFETY: ENV_LOCK serializes every test that touches the environment.
		unsafe { std::env::set_var(name, value) };
	}
}

impl Drop for TestEnv {
	#[expect(unsafe_code, reason = "tests mutate the process environment, serialized by ENV_LOCK")]
	fn drop(&mut self) {
		for (name, old) in self.saved.drain(..).rev() {
			// SAFETY: ENV_LOCK is still held through `_lock`.
			unsafe {
				match old {
					Some(value) => std::env::set_var(&name, value),
					None => std::env::remove_var(&name),
				}
			}
		}
	}
}

fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
	pairs.iter().map(|&(k, v)| (k.to_owned(), v.to_owned())).collect()
}

#[test]
fn parse_basics() {
	let parsed = parse("KEY=value\nOTHER=two words\nNUM=42\n");
	let want = vars(&[("KEY", "value"), ("OTHER", "two words"), ("NUM", "42")]);
	assert_eq!(parsed, want);
}

#[test]
fn parse_crlf() {
	// Windows-authored .env files end lines with \r\n; the \r must not leak
	// into values.
	let parsed = parse("A=one\r\nB=two\r\n");
	assert_eq!(parsed, vars(&[("A", "one"), ("B", "two")]));
}

#[test]
fn parse_comments_and_blanks() {
	let parsed = parse("# full line comment\n\n   \n  # indented comment\nKEY=value\n");
	assert_eq!(parsed, vars(&[("KEY", "value")]), "want only KEY=value");
}

#[test]
fn parse_quotes() {
	let parsed = parse(
		"D=\"double quoted\"\n\
		 S='single quoted'\n\
		 INNER=\"  keeps inner spaces  \"\n\
		 MIXED=\"unbalanced'\n\
		 EMPTYQ=\"\"\n\
		 HASH=\"# not a comment\"\n",
	);
	let cases = [
		("D", "double quoted"),
		("S", "single quoted"),
		("INNER", "  keeps inner spaces  "),
		("MIXED", "\"unbalanced'"), // mismatched quotes are kept verbatim
		("EMPTYQ", ""),
		("HASH", "# not a comment"),
	];
	for (key, want) in cases {
		assert_eq!(parsed.get(key).map(String::as_str), Some(want), "vars[{key:?}]");
	}
}

#[test]
fn parse_whitespace_and_junk() {
	let parsed = parse("  PADDED  =  spaced out  \nNOEQUALS\n=novalue\nEMPTY=\nLAST=wins\nLAST=really wins\n");
	assert_eq!(parsed.get("PADDED").map(String::as_str), Some("spaced out"), "PADDED");
	assert!(!parsed.contains_key("NOEQUALS"), "line without = should be ignored");
	assert!(!parsed.contains_key(""), "empty key should be ignored");
	assert_eq!(
		parsed.get("EMPTY").map(String::as_str),
		Some(""),
		"EMPTY should be an empty string, present"
	);
	assert_eq!(
		parsed.get("LAST").map(String::as_str),
		Some("really wins"),
		"later duplicate should win"
	);
}

fn write_file(path: &str, text: &str) {
	fs::write(path, text).expect("write");
}

#[test]
fn load_precedence() {
	let dir = tempfile::tempdir().expect("temp dir");
	let dir = dir.path().to_str().expect("UTF-8").to_owned();
	write_file(
		&gopath::join(&[&dir, ".env"]),
		"BASE=base\r\nOVERLAY=from-base\r\nPROC=from-file\r\nNODE_ENV=studio\r\n",
	);
	write_file(&gopath::join(&[&dir, ".env.studio"]), "OVERLAY=from-studio\n");

	// NODE_ENV comes from the .env file here (unset in the process env).
	let mut process = TestEnv::new();
	process.set("NODE_ENV", "");
	process.set("PROC", "from-process");

	let env = Env::load(&dir);

	let cases = [
		("BASE", Some("base")),           // .env only
		("OVERLAY", Some("from-studio")), // .env.<NODE_ENV> beats .env
		("PROC", Some("from-process")),   // process env beats files
		("ROTOR_DOTENV_UNSET_42", None),  // defined nowhere
		("NODE_ENV", Some("")),           // process env ("" set above) wins
	];
	for (name, want) in cases {
		assert_eq!(env.lookup(name).as_deref(), want, "lookup({name:?})");
	}
}

#[test]
fn load_node_env_from_process() {
	let dir = tempfile::tempdir().expect("temp dir");
	let dir = dir.path().to_str().expect("UTF-8").to_owned();
	write_file(&gopath::join(&[&dir, ".env"]), "NODE_ENV=studio\nWHO=base\n");
	write_file(&gopath::join(&[&dir, ".env.studio"]), "WHO=studio\n");
	write_file(&gopath::join(&[&dir, ".env.production"]), "WHO=production\n");

	// Process NODE_ENV outranks the .env-declared one.
	let mut process = TestEnv::new();
	process.set("NODE_ENV", "production");
	let env = Env::load(&dir);
	assert_eq!(env.lookup("WHO").as_deref(), Some("production"), "WHO");
}

#[test]
fn load_missing_files() {
	let _process = TestEnv::new();
	let dir = tempfile::tempdir().expect("temp dir");
	let env = Env::load(dir.path().to_str().expect("UTF-8"));
	assert_eq!(
		env.lookup("ROTOR_DOTENV_UNSET_42"),
		None,
		"unset name resolved with no .env files present"
	);
}

#[test]
fn nil_env_lookup() {
	let mut process = TestEnv::new();
	process.set("ROTOR_DOTENV_NIL_RECEIVER", "ok");
	assert_eq!(
		Env::lookup_in(None, "ROTOR_DOTENV_NIL_RECEIVER").as_deref(),
		Some("ok"),
		"process env hit"
	);
	assert_eq!(
		Env::lookup_in(None, "ROTOR_DOTENV_UNSET_42"),
		None,
		"nil receiver resolved an unset name"
	);
}
