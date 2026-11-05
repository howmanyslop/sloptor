//! `sidecar_install_test.go`, `sidecar_env_test.go`, `sidecar_shards_test.go`,
//! `sidecar_slot_test.go`, `sidecar_roots_test.go` and `transformer_host_test.go`.

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rotor_tsrs::ast::{SourceFile, SourceFileParseOptions};
use rotor_tsrs::tscore::ScriptKind;
use rotor_tsrs::{P, tspath};

use super::sidecar::{
	PREFIX_STRING_PLUGIN, SessionsGuard, build, env_value, repo_sidecar_dir, round_trip, symlink_fixture_typescript,
};
use super::{Temp, write_project};
use crate::project::ProjectOptions;
use crate::sidecar::{
	close_sidecar_sessions, set_test_env, sidecar_env, sidecar_node_options, sidecar_slot_count, sidecar_slot_for,
	spawn_sidecar_session,
};
use crate::sidecar_install::{extract_sidecar_into, resolve_sidecar_dir};
use crate::sidecar_roots::narrowed_sidecar_roots;
use crate::sidecar_shards::{SIDECAR_SHARD_MIN_FILES, set_build_share, sidecar_shard_count, split_sidecar_shards};
use crate::transformer_host::{
	TransformerChangedFile, TransformerRequest, TransformerResponse, with_transformer_callback,
};

const GRACE: Duration = Duration::from_millis(50);

// sidecar_install_test.go

#[test]
fn resolve_sidecar_dir_extracts_embedded_worker() {
	// Go redirects the user cache directory; the port extracts into a temp cache root.
	let cache = Temp::new();
	let dir = extract_sidecar_into(&cache.dir()).unwrap_or_else(|error| panic!("resolveSidecarDir: {error}"));
	for name in [
		"main.js",
		"index.js",
		"lib/session.js",
		"lib/plugins.js",
		"lib/diagnostics.js",
		"package.json",
	] {
		assert!(
			std::path::Path::new(&dir).join(name).exists(),
			"extracted sidecar missing {name}"
		);
	}
	// Idempotent: a second call returns the same completed dir.
	let again = extract_sidecar_into(&cache.dir()).unwrap_or_else(|error| panic!("second resolveSidecarDir: {error}"));
	assert_eq!(again, dir, "resolveSidecarDir not stable");
}

#[test]
fn resolve_sidecar_dir_honors_override() {
	let override_dir = repo_sidecar_dir();
	set_test_env("ROTOR_SIDECAR_PATH", &override_dir);
	let dir = resolve_sidecar_dir().unwrap_or_else(|error| panic!("resolveSidecarDir: {error}"));
	assert_eq!(dir, override_dir, "dir, want override");
}

// sidecar_env_test.go

#[test]
fn sidecar_node_options_drops_the_package_map() {
	for (name, options, want) in [
		("empty", "", ""),
		(
			"unrelated options",
			r#" --require "hook with spaces.js"  --trace-warnings "#,
			r#" --require "hook with spaces.js"  --trace-warnings "#,
		),
		(
			"equals",
			"--experimental-package-map=/project/node_modules/.package-map.json",
			"",
		),
		(
			"quoted path",
			r#"--trace-warnings --experimental-package-map="C:\\project with spaces\\.package-map.json" --max-old-space-size=2048"#,
			"--trace-warnings --max-old-space-size=2048",
		),
		(
			"separate value",
			r#"--experimental-package-map "/project with spaces/.package-map.json" --trace-warnings"#,
			"--trace-warnings",
		),
		(
			"quoted option",
			r#""--experimental-package-map=/project with spaces/map.json" --trace-warnings"#,
			"--trace-warnings",
		),
		(
			"repeated maps",
			"--experimental-package-map=a --trace-warnings --experimental-package-map b",
			"--trace-warnings",
		),
		(
			"empty quoted argument",
			r#"--experimental-package-map "" a --trace-warnings"#,
			"--trace-warnings",
		),
		(
			"underscore alias",
			"--experimental_package_map=a --trace-warnings",
			"--trace-warnings",
		),
		(
			"preserve quoted preload",
			r#"--require "C:\\hooks\\with spaces.js" --experimental-package-map=a --trace-warnings"#,
			r#"--require "C:\\hooks\\with spaces.js" --trace-warnings"#,
		),
		(
			"preserve escaped quote",
			r#"--require "hook\"name.js" --experimental-package-map=a"#,
			r#"--require "hook\"name.js""#,
		),
		(
			"preserve similar option",
			"--experimental-package-map-extra=a",
			"--experimental-package-map-extra=a",
		),
		(
			"preserve flag in path",
			r#"--require "hooks/--experimental-package-map=a.js""#,
			r#"--require "hooks/--experimental-package-map=a.js""#,
		),
		(
			"unterminated quote",
			r#"--experimental-package-map=a --require "hook"#,
			r#"--experimental-package-map=a --require "hook"#,
		),
		(
			"invalid escape",
			r#"--experimental-package-map=a --require "hook\"#,
			r#"--experimental-package-map=a --require "hook\"#,
		),
		(
			"missing value",
			"--experimental-package-map",
			"--experimental-package-map",
		),
		(
			"missing value before option",
			"--experimental-package-map --trace-warnings",
			"--experimental-package-map --trace-warnings",
		),
		(
			"missing value before short option",
			"--experimental-package-map -r hook.js",
			"--experimental-package-map -r hook.js",
		),
		(
			"value starting with hyphen",
			"--experimental-package-map -map.json",
			"--experimental-package-map -map.json",
		),
	] {
		assert_eq!(
			sidecar_node_options(options),
			want,
			"{name}: sidecarNodeOptions({options:?})"
		);
	}
}

#[test]
fn sidecar_env_removes_package_map_without_node_modules() {
	let options = r#"--trace-warnings --experimental-package-map="/project with spaces/map.json""#;
	set_test_env("NODE_OPTIONS", options);
	let temp = Temp::new();
	let env = sidecar_env(
		&temp.join("project").to_string_lossy(),
		&temp.join("sidecar").to_string_lossy(),
	);
	assert_eq!(
		env_value(&env, "NODE_OPTIONS"),
		"--trace-warnings",
		"child NODE_OPTIONS"
	);
	// The parent's NODE_OPTIONS is never touched: the override lives in the test environment.
	assert_eq!(
		crate::sidecar::getenv("NODE_OPTIONS").as_deref(),
		Some(options),
		"parent NODE_OPTIONS"
	);
}

#[test]
fn sidecar_session_with_inherited_package_map() {
	for project_typescript in [true, false] {
		let temp = Temp::new();
		let dir = temp.join("project with spaces").to_string_lossy().into_owned();
		std::fs::create_dir_all(format!("{dir}/node_modules/test-transformer")).expect("mkdir");
		if project_typescript {
			symlink_fixture_typescript(&dir);
		}
		for (file, contents) in [
			("package.json", r#"{"name":"mapped-project"}"#),
			(
				"package-map.json",
				r#"{"packages":{"project":{"url":"./","dependencies":{}}}}"#,
			),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"noLib":true,"types":[],"plugins":[{"transform":"test-transformer","prefix":"mapped"}]},"files":["main.ts"]}"#,
			),
			("main.ts", r#"export const phase = "start";"#),
			("node_modules/test-transformer/index.js", PREFIX_STRING_PLUGIN),
		] {
			std::fs::write(format!("{dir}/{file}"), contents).expect("write");
		}
		let options = format!(r#"--experimental-package-map="{dir}/package-map.json" --max-old-space-size=2048"#);
		set_test_env("NODE_OPTIONS", &options);
		let mut session = spawn_sidecar_session(&dir, &repo_sidecar_dir()).unwrap_or_else(|error| panic!("{error}"));
		let request = TransformerRequest {
			protocol: 1,
			operation: "transform".to_owned(),
			project_dir: dir.clone(),
			ts_config_path: format!("{dir}/tsconfig.json"),
			compile_file_names: Some(vec![format!("{dir}/main.ts")]),
			changed_files: Some(Vec::new()),
			..TransformerRequest::default()
		};
		let response = round_trip(&mut session, &request, Duration::from_secs(30));
		session.close();
		let response = response.unwrap_or_else(|error| panic!("project typescript {project_typescript}: {error}"));
		assert!(
			response.diagnostics.is_empty(),
			"diagnostics: {:?}",
			response.diagnostics
		);
		assert!(
			response.transformed.len() == 1 && response.transformed[0].text.contains(r#""mapped:start""#),
			"transformed: {:?}, want mapped:start",
			response.transformed
		);
	}
}

// sidecar_shards_test.go

/// `parseSourceFile`: one file parsed the way a project program would.
pub fn parse_source_file(name: &str, text: &str) -> P<SourceFile> {
	rotor_tsrs::parser::parse_source_file(
		SourceFileParseOptions {
			file_name: name.to_owned(),
			path: tspath::Path::from(name),
			..SourceFileParseOptions::default()
		},
		text,
		ScriptKind::TS,
	)
}

/// Catches a sharded transform that loses a file or sends one to two workers.
#[test]
fn split_sidecar_shards_partitions_every_file_once() {
	let files: Vec<P<SourceFile>> = (0..1000)
		.map(|index| {
			let body = "export const value = 1;\n".repeat(1 + index % 37);
			parse_source_file(&format!("/proj/src/{:04}.ts", (index * 7919) % 1000), &body)
		})
		.collect();
	for count in [2, 3, 4, 7] {
		let shards = split_sidecar_shards(&files, count);
		assert_eq!(shards.len(), count, "count {count}: shards");
		let mut seen = Vec::new();
		for shard in &shards {
			assert!(!shard.is_empty(), "count {count}: empty shard");
			seen.extend(shard.iter().map(|file| file.file_name().to_owned()));
		}
		let mut want: Vec<String> = files.iter().map(|file| file.file_name().to_owned()).collect();
		seen.sort();
		want.sort();
		assert_eq!(seen, want, "count {count}: shards do not cover the input exactly once");
	}
}

/// Catches a small project or an incremental rebuild fanning out across several Node workers.
#[test]
fn sidecar_shard_count_keeps_small_requests_on_one_worker() {
	set_test_env("ROTOR_SIDECAR_SHARDS", "4");
	for files in [0, 1, 3, SIDECAR_SHARD_MIN_FILES * 2 - 1] {
		let got = sidecar_shard_count(files);
		assert!(
			got <= 1 || files >= SIDECAR_SHARD_MIN_FILES * 2,
			"{files} files: got {got} shards, want 1"
		);
	}
	assert_eq!(
		sidecar_shard_count(SIDECAR_SHARD_MIN_FILES * 4),
		4,
		"{} files",
		SIDECAR_SHARD_MIN_FILES * 4
	);
	set_test_env("ROTOR_SIDECAR_SHARDS", "1");
	assert_eq!(sidecar_shard_count(SIDECAR_SHARD_MIN_FILES * 4), 1, "sharding off");
}

/// Catches every one of several concurrent builds starting its full pool of Node workers.
#[test]
fn sidecar_shard_count_shrinks_with_concurrent_builds() {
	set_test_env("ROTOR_SIDECAR_SHARDS", "");
	set_build_share(4);
	assert_eq!(
		sidecar_shard_count(SIDECAR_SHARD_MIN_FILES * 16),
		1,
		"4 concurrent builds"
	);
	set_test_env("ROTOR_SIDECAR_SHARDS", "3");
	assert_eq!(
		sidecar_shard_count(SIDECAR_SHARD_MIN_FILES * 16),
		3,
		"ROTOR_SIDECAR_SHARDS=3"
	);
	set_build_share(1);
}

// sidecar_slot_test.go

#[test]
fn sidecar_slots_allow_concurrent_keys() {
	let _sessions = SessionsGuard::new();
	let first = sidecar_slot_for("dir-a|cfg-a");
	let second = sidecar_slot_for("dir-b|cfg-b");
	let (started, started_rx) = channel();
	let (release, release_rx) = std::sync::mpsc::sync_channel::<()>(0);
	let release_rx = Arc::new(Mutex::new(release_rx));
	let handles: Vec<_> = [("a", first), ("b", second)]
		.into_iter()
		.map(|(name, slot)| {
			let started = started.clone();
			let release_rx = Arc::clone(&release_rx);
			std::thread::spawn(move || {
				let _guard = slot.acquire();
				let _ = started.send(name);
				let _ = crate::project::lock(&release_rx).recv();
			})
		})
		.collect();
	for _ in 0..2 {
		started_rx
			.recv_timeout(Duration::from_secs(10))
			.expect("concurrent keys started");
	}
	drop(release);
	for handle in handles {
		handle.join().expect("the holder");
	}
}

#[test]
fn sidecar_slots_serialize_the_same_key() {
	let _sessions = SessionsGuard::new();
	let slot = sidecar_slot_for("dir-a|cfg-a");
	let (started, started_rx) = channel();
	let (release, release_rx) = channel::<()>();
	let release_rx = Arc::new(Mutex::new(release_rx));
	let spawn = || {
		let slot = Arc::clone(&slot);
		let started = started.clone();
		let release_rx = Arc::clone(&release_rx);
		std::thread::spawn(move || {
			let _guard = slot.acquire();
			let _ = started.send(());
			let _ = crate::project::lock(&release_rx).recv();
		})
	};
	let first = spawn();
	started_rx
		.recv_timeout(Duration::from_secs(10))
		.expect("the first holder");
	let second = spawn();
	assert!(started_rx.recv_timeout(GRACE).is_err(), "same key overlapped");
	drop(release);
	first.join().expect("the first holder");
	second.join().expect("the second holder");
}

#[test]
fn close_sidecar_sessions_waits_for_active_slot() {
	let _sessions = SessionsGuard::new();
	let slot = sidecar_slot_for("dir-a|cfg-a");
	let (held, held_rx) = channel();
	let (release, release_rx) = channel::<()>();
	let holder = std::thread::spawn(move || {
		let _guard = slot.acquire();
		let _ = held.send(());
		let _ = release_rx.recv();
	});
	held_rx.recv_timeout(Duration::from_secs(10)).expect("held");
	let (done, closed_rx) = channel();
	let closer = std::thread::spawn(move || {
		close_sidecar_sessions();
		let _ = done.send(());
	});
	assert!(
		closed_rx.recv_timeout(GRACE).is_err(),
		"closeSidecarSessions returned while the slot was held"
	);
	drop(release);
	closed_rx.recv_timeout(Duration::from_secs(10)).expect("closed");
	holder.join().expect("the holder");
	closer.join().expect("the closer");
	assert_eq!(sidecar_slot_count(), 0, "slots after close");
}

// sidecar_roots_test.go

/// `fakeSourceFiles`: ordinary external modules.
fn fake_source_files(names: &[&str]) -> Vec<P<SourceFile>> {
	names
		.iter()
		.map(|name| parse_source_file(name, "export {};\n"))
		.collect()
}

fn native(names: &[&str]) -> Vec<String> {
	names
		.iter()
		.map(|name| rotor_support::gopath::from_slash(name).into_owned())
		.collect()
}

#[test]
fn narrowed_sidecar_roots_keeps_every_declaration_file() {
	let project = fake_source_files(&[
		"/proj/a.ts",
		"/proj/b.ts",
		"/proj/globals.d.ts",
		"/proj/node.d.mts",
		"/proj/legacy.d.cts",
	]);
	assert_eq!(
		narrowed_sidecar_roots(&project[..1], &project),
		native(&[
			"/proj/a.ts",
			"/proj/globals.d.ts",
			"/proj/node.d.mts",
			"/proj/legacy.d.cts"
		]),
		"narrowedSidecarRoots"
	);
}

/// A `.ts` that is not an external module puts every top-level name in the global scope.
#[test]
fn narrowed_sidecar_roots_keeps_global_scripts() {
	let project = [
		parse_source_file("/proj/a.ts", "export {};\n"),
		parse_source_file("/proj/b.ts", "export {};\n"),
		parse_source_file("/proj/script.ts", "declare const AMBIENT: number;\n"),
	];
	assert_eq!(
		narrowed_sidecar_roots(&project[..1], &project),
		native(&["/proj/a.ts", "/proj/script.ts"]),
		"narrowedSidecarRoots"
	);
}

/// `declare global` inside a module reaches every file too.
#[test]
fn narrowed_sidecar_roots_keeps_global_augmentations() {
	let project = [
		parse_source_file("/proj/a.ts", "export {};\n"),
		parse_source_file("/proj/b.ts", "export {};\n"),
		parse_source_file(
			"/proj/augment.ts",
			"export {};\ndeclare global {\n\tinterface Ambient { name: string }\n}\n",
		),
	];
	assert_eq!(
		narrowed_sidecar_roots(&project[..1], &project),
		native(&["/proj/a.ts", "/proj/augment.ts"]),
		"narrowedSidecarRoots"
	);
}

/// An augmentation of a NAMED module rides in on the import closure.
#[test]
fn narrowed_sidecar_roots_drops_named_module_augmentations() {
	let project = [
		parse_source_file("/proj/a.ts", "export {};\n"),
		parse_source_file("/proj/b.ts", "export {};\n"),
		parse_source_file(
			"/proj/augment.ts",
			"export {};\ndeclare module \"other\" {\n\tinterface Thing { name: string }\n}\n",
		),
	];
	assert_eq!(
		narrowed_sidecar_roots(&project[..1], &project),
		native(&["/proj/a.ts"]),
		"narrowedSidecarRoots"
	);
}

/// A full build already compiles every file, so narrowing would name the whole project.
#[test]
fn narrowed_sidecar_roots_skips_full_builds() {
	let project = fake_source_files(&["/proj/a.ts", "/proj/b.ts"]);
	assert!(
		narrowed_sidecar_roots(&project, &project).is_empty(),
		"narrowedSidecarRoots(full build)"
	);
	assert!(
		narrowed_sidecar_roots(&[], &project).is_empty(),
		"narrowedSidecarRoots(no selection)"
	);
}

/// A compiled file that the tsconfig never named still has to become a root.
#[test]
fn narrowed_sidecar_roots_names_every_compiled_file() {
	let project = fake_source_files(&["/proj/a.ts", "/proj/b.ts", "/proj/c.ts"]);
	let compile = fake_source_files(&["/proj/a.ts", "/proj/generated.ts"]);
	assert_eq!(
		narrowed_sidecar_roots(&compile, &project),
		native(&["/proj/a.ts", "/proj/generated.ts"]),
		"narrowedSidecarRoots"
	);
}

// transformer_host_test.go

#[test]
fn callback_transformer_host_retains_and_reverts_overlays() {
	let temp = write_project("@scope/callback-overlay-fixture", "");
	let dir = temp.dir();
	temp.write("src/main.ts", "export const phase = \"disk\";\n");
	temp.write("plugins/fixture.js", "module.exports = () => () => file => file;\n");
	temp.write(
		"tsconfig.json",
		r#"{"compilerOptions":{"allowSyntheticDefaultImports":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out","plugins":[{"transform":"./plugins/fixture.js"}]},"include":["src"]}"#,
	);
	let main_path = temp.join("src/main.ts").to_string_lossy().into_owned();
	let requests: Arc<Mutex<Vec<TransformerRequest>>> = Arc::default();
	let recorded = Arc::clone(&requests);
	let mut opts = ProjectOptions::default();
	with_transformer_callback(
		&mut opts,
		Arc::new(move |_, request| {
			crate::project::lock(&recorded).push(request);
			Ok(TransformerResponse::default())
		}),
	);
	let overlay_opts = ProjectOptions {
		overlays: [(main_path, "export const phase = \"memory\";\n".to_owned())].into(),
		..opts.clone()
	};
	let (_, diags, error) = build(&dir, &overlay_opts);
	assert!(error.is_none(), "overlay build: {error:?} ({diags:?})");
	let (_, diags, error) = build(&dir, &opts);
	assert!(error.is_none(), "disk build: {error:?} ({diags:?})");
	let requests = crate::project::lock(&requests);
	assert_eq!(requests.len(), 2, "callback requests");
	let changed =
		|index: usize| -> Vec<TransformerChangedFile> { requests[index].changed_files.clone().unwrap_or_default() };
	assert!(
		changed(0).len() == 1 && changed(0)[0].text.contains("memory"),
		"first changedFiles = {:?}, want overlay",
		changed(0)
	);
	assert!(
		changed(1).len() == 1 && changed(1)[0].text.contains("disk"),
		"second changedFiles = {:?}, want overlay revert",
		changed(1)
	);
}

/// Go hands the callback a `context.WithTimeout` context; a callback that waits it out reports
/// Go's deadline error.
#[test]
fn callback_transformer_host_passes_the_response_deadline() {
	use crate::transformer_host::TransformerHost;
	set_test_env("ROTOR_SIDECAR_TIMEOUT", "50ms");
	let temp = Temp::new();
	let host = crate::transformer_host_sidecar::CallbackTransformerHost::new(Arc::new(|cancel, _| {
		let cancel = cancel.expect("the callback gets the deadline");
		let started = std::time::Instant::now();
		while !cancel.is_cancelled() {
			assert!(started.elapsed() < Duration::from_secs(10), "the deadline never passed");
			std::thread::sleep(Duration::from_millis(5));
		}
		Err(crate::compile::Error::message(cancel.err().unwrap_or_default()))
	}));
	let config_path = temp.join("tsconfig.json").to_string_lossy().into_owned();
	let error = host
		.validate(None, &temp.dir(), &config_path)
		.err()
		.map(|error| error.to_string());
	assert_eq!(
		error.as_deref(),
		Some("transformer callback failed: context deadline exceeded"),
		"validate error"
	);
}

/// `syscall.Signal.String` on this OS.
#[cfg(unix)]
#[test]
fn signal_names_match_go() {
	use crate::sidecar::signal_name;
	let (abort, bus) = if cfg!(target_os = "macos") {
		("abort trap", 10)
	} else {
		("aborted", 7)
	};
	assert_eq!(signal_name(6), abort);
	assert_eq!(signal_name(bus), "bus error");
	assert_eq!(signal_name(11), "segmentation fault");
	assert_eq!(signal_name(9), "killed");
	assert_eq!(signal_name(64), "signal 64");
}

/// Go embeds `main.js index.js lib/*.js package.json`; the port lists the files by hand, so a
/// worker file added to `tools/sidecar/lib` must fail here until it is listed.
#[test]
fn embedded_sidecar_lists_every_worker_file() {
	let root = super::repo_root().join("tools/sidecar");
	let mut want = vec!["index.js".to_owned(), "main.js".to_owned(), "package.json".to_owned()];
	for entry in std::fs::read_dir(root.join("lib")).expect("tools/sidecar/lib") {
		let name = entry.expect("an entry").file_name().to_string_lossy().into_owned();
		if std::path::Path::new(&name)
			.extension()
			.is_some_and(|extension| extension == "js")
		{
			want.push(format!("lib/{name}"));
		}
	}
	want.sort();
	let got: Vec<String> = crate::sidecar_install::EMBEDDED_SIDECAR
		.iter()
		.map(|(name, _)| (*name).to_owned())
		.collect();
	assert_eq!(got, want, "embedded worker files");
	for (name, data) in crate::sidecar_install::EMBEDDED_SIDECAR {
		assert_eq!(
			*data,
			std::fs::read(root.join(name)).expect("the worker file").as_slice(),
			"{name} is stale"
		);
	}
}
