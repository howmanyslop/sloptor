//! Port of `internal/rojo/resolver_cache_test.go`.

use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::{RESOLVER_CACHE_FORMAT_VERSION, RojoResolverCache, cache_key, parse_cache_file};
use crate::resolver::{PartitionInfo, RbxPathLookup, ResolverFilePathState, ResolverState, RojoResolver};
use crate::test_support::{must_rbx_path, rbx, set_mtime, write_project};

fn unix_time(seconds: u64) -> SystemTime {
	SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn resolver_state_round_trips_deterministically() {
	// Given
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"game","tree":{"$className":"DataModel","z":{"$path":"misc/z.json"},"a":{"$path":"misc/a.json"},"src":{"$path":"out"}}}"#,
		),
		("misc/a.json", "{}"),
		("misc/z.json", "{}"),
		("out/", ""),
	]);
	let resolver = RojoResolver::from_path(&project.path("default.project.json"));

	// When
	let state = resolver.state();
	let first = format!("{state:?}");
	let second = format!("{:?}", resolver.state());
	let restored = RojoResolver::from_state(&state).expect("from_state");

	// Then
	assert_eq!(first, second, "state serialization is not deterministic");
	assert_eq!(state, restored.state(), "restored state");
	assert_eq!(
		must_rbx_path(&restored, &project.path("out/main.luau")),
		rbx(&["src", "main"]),
		"restored output path"
	);
	assert_eq!(
		must_rbx_path(&restored, &project.path("misc/a.json")),
		rbx(&["a"]),
		"restored exact path"
	);
	assert!(
		state.file_paths.len() == 2 && state.file_paths[0].path <= state.file_paths[1].path,
		"file path state is not sorted: {:?}",
		state.file_paths
	);
	assert!(state.is_game, "restored state must preserve the game flag");
}

#[test]
fn resolver_state_rejects_malformed_mappings() {
	let tests = [
		(
			"empty partition path",
			ResolverState {
				partitions: vec![PartitionInfo {
					fs_path: String::new(),
					rbx_path: rbx(&["ReplicatedStorage"]),
				}],
				..ResolverState::default()
			},
		),
		(
			"empty mapped file path",
			ResolverState {
				file_paths: vec![ResolverFilePathState {
					path: String::new(),
					rbx_path: Some(rbx(&["ReplicatedStorage"])),
				}],
				..ResolverState::default()
			},
		),
		(
			"duplicate mapped file path",
			ResolverState {
				file_paths: vec![
					ResolverFilePathState {
						path: "/project/src/module.luau".to_owned(),
						rbx_path: Some(rbx(&["ReplicatedStorage", "Module"])),
					},
					ResolverFilePathState {
						path: "/project/src/module.luau".to_owned(),
						rbx_path: Some(rbx(&["ServerStorage", "Module"])),
					},
				],
				..ResolverState::default()
			},
		),
		(
			"Lua file mapping aliases Luau",
			ResolverState {
				file_paths: vec![ResolverFilePathState {
					path: "/project/src/module.lua".to_owned(),
					rbx_path: Some(rbx(&["ReplicatedStorage", "Module"])),
				}],
				..ResolverState::default()
			},
		),
	];

	for (name, state) in tests {
		// When
		let resolver = RojoResolver::from_state(&state);

		// Then
		assert!(
			resolver.is_err(),
			"{name}: from_state({state:?}) succeeded with resolver {resolver:?}"
		);
	}
}

#[test]
fn rojo_cache_loads_resolver_state_from_disk() {
	// Given
	let project = write_project(&[
		("default.project.json", r#"{"name":"p","tree":{"src":{"$path":"out"}}}"#),
		("out/", ""),
	]);
	let config_path = project.path("default.project.json");
	let cache_dir = project.path(".cache");
	let first_cache = RojoResolverCache::new(&cache_dir, "compiler-v1");

	// When
	let first = first_cache.load(&config_path);
	let second_cache = RojoResolverCache::new(&cache_dir, "compiler-v1");
	let second = second_cache.load(&config_path);

	// Then
	assert_eq!(
		must_rbx_path(&first, &project.path("out/main.luau")),
		rbx(&["src", "main"]),
		"first cache path"
	);
	assert_eq!(
		must_rbx_path(&second, &project.path("out/main.luau")),
		rbx(&["src", "main"]),
		"disk cache path"
	);
	let data = fs::read(first_cache.cache_path(&config_path)).expect("cache file");
	let file = parse_cache_file(&data).expect("cache file must be valid JSON");
	assert!(
		file.version == RESOLVER_CACHE_FORMAT_VERSION
			&& file.compiler_version == "compiler-v1"
			&& file.key == cache_key(&config_path),
		"cache header = {} {} {}",
		file.version,
		file.compiler_version,
		file.key
	);
	assert!(
		!file.mtime_manifest.is_empty(),
		"cache file must persist the walked-path mtime manifest"
	);
}

#[test]
fn rojo_cache_keeps_nil_paths_apart_from_empty_ones() {
	// Given a tree-root module mapping, which Go stores as a nil path and marshals as `null`
	let project = write_project(&[
		("default.project.json", r#"{"name":"p","tree":{"$path":"init.lua"}}"#),
		("init.lua", ""),
	]);
	let config_path = project.path("default.project.json");
	let cache_dir = project.path(".cache");
	let first_cache = RojoResolverCache::new(&cache_dir, "compiler-v1");
	let _ = first_cache.load(&config_path);

	// When the state is read back from disk
	let data = fs::read(first_cache.cache_path(&config_path)).expect("cache file");
	let file = parse_cache_file(&data).expect("cache file must be valid JSON");
	let reloaded = RojoResolverCache::new(&cache_dir, "compiler-v1").load(&config_path);

	// Then both keep the nil path
	let text = String::from_utf8(data).expect("UTF-8 cache");
	assert!(text.contains(r#""rbxPath": null"#), "nil path written as null: {text}");
	assert_eq!(file.state.file_paths[0].rbx_path, None, "parsed nil path");
	assert_eq!(
		reloaded.lookup_rbx_path(&project.path("init.luau")),
		RbxPathLookup::Nil,
		"reloaded nil path"
	);
}

#[test]
fn rojo_cache_rebuilds_when_walked_directory_changes() {
	// Given
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"outer":{"$path":"src"}}}"#,
		),
		("src/pkg/lib/", ""),
	]);
	let config_path = project.path("default.project.json");
	let cache = RojoResolverCache::new(&project.path(".cache"), "compiler-v1");
	let before = must_rbx_path(&cache.load(&config_path), &project.path("src/pkg/lib/item.luau"));

	fs::write(
		project.path("src/pkg/default.project.json"),
		r#"{"name":"inner","tree":{"$path":"lib"}}"#,
	)
	.expect("write");
	set_mtime(&project.path("src/pkg"), unix_time(1_700_000_000));

	// When
	let after = must_rbx_path(&cache.load(&config_path), &project.path("src/pkg/lib/item.luau"));

	// Then
	assert_eq!(before, rbx(&["outer", "pkg", "lib", "item"]), "initial path");
	assert_eq!(after, rbx(&["outer", "inner", "item"]), "rebuilt path");
}

#[test]
fn rojo_cache_rebuilds_when_nested_project_changes() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"root","tree":{"Pkg":{"$path":"shared.project.json"}}}"#,
		),
		("shared.project.json", r#"{"name":"ignored","tree":{"$path":"first"}}"#),
		("first/", ""),
		("second/", ""),
	]);
	let config_path = project.path("default.project.json");
	let nested_config_path = project.path("shared.project.json");
	let cache = RojoResolverCache::new(&project.path(".cache"), "compiler-v1");

	let before = must_rbx_path(&cache.load(&config_path), &project.path("first/item.luau"));
	assert_eq!(before, rbx(&["Pkg", "item"]), "initial path");

	fs::write(&nested_config_path, r#"{"name":"ignored","tree":{"$path":"second"}}"#).expect("write");
	set_mtime(&nested_config_path, SystemTime::now() + Duration::from_secs(2));

	let after_resolver = cache.load(&config_path);
	assert!(
		after_resolver
			.get_rbx_path_from_file_path(&project.path("first/item.luau"))
			.is_none(),
		"stale nested-project partition survived cache invalidation"
	);
	let after = must_rbx_path(&after_resolver, &project.path("second/item.luau"));
	assert_eq!(after, rbx(&["Pkg", "item"]), "rebuilt path");
}

#[test]
fn rojo_cache_rebuilds_when_project_changes_keeping_its_mtime() {
	// Given a cached resolver whose nested project names a partition
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"root","tree":{"Pkg":{"$path":"shared.project.json"}}}"#,
		),
		("shared.project.json", r#"{"name":"ignored","tree":{"$path":"first"}}"#),
		("first/", ""),
		("second/", ""),
	]);
	let config_path = project.path("default.project.json");
	let nested_config_path = project.path("shared.project.json");
	let cache = RojoResolverCache::new(&project.path(".cache"), "compiler-v1");
	must_rbx_path(&cache.load(&config_path), &project.path("first/item.luau"));
	let modified = fs::metadata(&nested_config_path)
		.expect("stat")
		.modified()
		.expect("mtime");

	// When the nested project changes but keeps its mtime, as package
	// managers extracting a tarball with fixed timestamps do
	fs::write(&nested_config_path, r#"{"name":"ignored","tree":{"$path":"second"}}"#).expect("write");
	set_mtime(&nested_config_path, modified);

	// Then both the memory and disk caches rebuild the resolver
	let disk = RojoResolverCache::new(&project.path(".cache"), "compiler-v1");
	for (name, cache) in [("memory", &cache), ("disk", &disk)] {
		let resolver = cache.load(&config_path);
		assert!(
			resolver
				.get_rbx_path_from_file_path(&project.path("first/item.luau"))
				.is_none(),
			"{name} cache kept the stale partition"
		);
	}
}

#[test]
fn rojo_cache_rebuilds_when_compiler_version_or_disk_data_changes() {
	// Given
	let project = write_project(&[
		("default.project.json", r#"{"name":"p","tree":{"src":{"$path":"out"}}}"#),
		("out/", ""),
	]);
	let config_path = project.path("default.project.json");
	let cache_dir = project.path(".cache");
	let first_cache = RojoResolverCache::new(&cache_dir, "compiler-v1");
	let _ = first_cache.load(&config_path);

	// When
	let version_cache = RojoResolverCache::new(&cache_dir, "compiler-v2");
	let version_resolver = version_cache.load(&config_path);
	fs::write(version_cache.cache_path(&config_path), "{not json").expect("write");
	let corrupt_resolver = RojoResolverCache::new(&cache_dir, "compiler-v2").load(&config_path);

	// Then
	assert_eq!(
		must_rbx_path(&version_resolver, &project.path("out/main.luau")),
		rbx(&["src", "main"]),
		"version-invalidated path"
	);
	assert_eq!(
		must_rbx_path(&corrupt_resolver, &project.path("out/main.luau")),
		rbx(&["src", "main"]),
		"corruption fallback path"
	);
	let data = fs::read(version_cache.cache_path(&config_path)).expect("cache file");
	let file = parse_cache_file(&data).expect("corruption fallback did not atomically replace cache file");
	assert_eq!(file.compiler_version, "compiler-v2");
}

#[test]
fn rojo_cache_rebuilds_when_missing_partition_appears() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"outer":{"$path":"out"},"other":{"$path":"present"}}}"#,
		),
		("present/", ""),
	]);
	let config_path = project.path("default.project.json");
	let item = project.path("out/lib/code/item.luau");
	let cache = RojoResolverCache::memory();
	let before = must_rbx_path(&cache.load(&config_path), &item);

	// A build creates the output with a nested project, which renames lib.
	fs::create_dir_all(project.path("out/lib/code")).expect("mkdir");
	fs::write(
		project.path("out/lib/default.project.json"),
		r#"{"name":"inner","tree":{"$path":"code"}}"#,
	)
	.expect("write");

	let after = must_rbx_path(&cache.load(&config_path), &item);
	assert_eq!(before, rbx(&["outer", "lib", "code", "item"]), "initial path");
	assert_eq!(after, rbx(&["outer", "inner", "item"]), "rebuilt path");
}

#[test]
fn rojo_cache_keeps_resolver_when_directory_gains_plain_file() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"outer":{"$path":"out"}}}"#,
		),
		("out/", ""),
	]);
	let config_path = project.path("default.project.json");
	let cache = RojoResolverCache::memory();
	let before = cache.load(&config_path);

	fs::write(project.path("out/init.luau"), "return nil\n").expect("write");
	set_mtime(&project.path("out"), unix_time(1_700_000_000));

	let after = cache.load(&config_path);
	assert!(
		Arc::ptr_eq(&after, &before),
		"a build output file reparsed the Rojo tree"
	);
}

/// A partition that was there when the cache was stored and that the resolver never walked (a
/// symlinked `$path`, whose target directory is what it walks) must still be checked: Go's fresh
/// manifest records it as missing once the link goes, so the disk entry no longer matches.
#[cfg(unix)]
#[test]
fn rojo_cache_rebuilds_when_unwalked_partition_vanishes() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"outer":{"$path":"out"},"linked":{"$path":"link"}}}"#,
		),
		("out/", ""),
		("target/", ""),
	]);
	std::os::unix::fs::symlink(project.path("target"), project.path("link")).expect("symlink");
	let config_path = project.path("default.project.json");
	let cache_dir = project.path(".cache");
	let first = RojoResolverCache::new(&cache_dir, "compiler-v1");
	let _ = first.load(&config_path);
	let missing = |cache: &RojoResolverCache| {
		let data = fs::read(cache.cache_path(&config_path)).expect("cache file");
		let file = parse_cache_file(&data).expect("cache file must be valid JSON");
		file.mtime_manifest
			.iter()
			.any(|entry| entry.missing && entry.path == project.path("link"))
	};
	assert!(!missing(&first), "the linked partition was stored as missing");

	fs::remove_file(project.path("link")).expect("remove link");
	let second = RojoResolverCache::new(&cache_dir, "compiler-v1");
	let _ = second.load(&config_path);

	assert!(
		missing(&second),
		"the disk entry outlived its linked partition; it must be rebuilt with the partition missing"
	);
}

/// A cache trusted for one build keeps its in-memory resolver until the build revalidates it, as
/// it does once cleanup and the include copy are done; then a changed tree parses again, as Go's
/// next load would.
#[test]
fn rojo_cache_for_one_build_reparses_a_changed_tree_once_revalidated() {
	let project = write_project(&[
		(
			"default.project.json",
			r#"{"name":"p","tree":{"outer":{"$path":"out"},"other":{"$path":"present"}}}"#,
		),
		("present/", ""),
	]);
	let config_path = project.path("default.project.json");
	let item = project.path("out/lib/code/item.luau");
	let cache = RojoResolverCache::memory().for_one_build();
	let before = cache.load(&config_path);

	fs::create_dir_all(project.path("out/lib/code")).expect("mkdir");
	fs::write(
		project.path("out/lib/default.project.json"),
		r#"{"name":"inner","tree":{"$path":"code"}}"#,
	)
	.expect("write");
	let trusted = cache.load(&config_path);
	cache.revalidate();
	let after = cache.load(&config_path);

	assert!(
		Arc::ptr_eq(&trusted, &before),
		"the build's cache checked its entry before revalidation"
	);
	assert_eq!(
		must_rbx_path(&before, &item),
		rbx(&["outer", "lib", "code", "item"]),
		"initial path"
	);
	assert_eq!(
		must_rbx_path(&after, &item),
		rbx(&["outer", "inner", "item"]),
		"revalidated path"
	);
}
