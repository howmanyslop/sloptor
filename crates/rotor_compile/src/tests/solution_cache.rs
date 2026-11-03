//! `solution_cache_test.go`.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use rotor_tsrs::ast::SourceFileParseOptions;
use rotor_tsrs::tscore::ScriptKind;

use super::solution::{join, write_solution_config};
use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::project::ProjectOptions;
use crate::solution_cache::{SolutionCompileCache, cacheable_parse_name};
use crate::solution_drain::build_solution_with_options;
use crate::timings::BuildTimings;

#[test]
fn cacheable_parse_name_shares_declarations_and_json() {
	assert!(
		!cacheable_parse_name("src/main.ts") && !cacheable_parse_name("src/main.tsx"),
		"ordinary TypeScript sources must not be cached across projects"
	);
	assert!(
		cacheable_parse_name("node_modules/@rbxts/types/index.d.ts") && cacheable_parse_name("tsconfig.json"),
		"declaration and JSON files should be cacheable"
	);
}

#[test]
fn solution_compile_cache_singleflight() {
	let cache = Arc::new(SolutionCompileCache::new());
	let parses = Arc::new(AtomicI32::new(0));
	// Go's options name `shared.d.ts`; tsrs parses only absolute names.
	let opts = SourceFileParseOptions {
		file_name: "/shared.d.ts".to_owned(),
		path: rotor_tsrs::tspath::to_path("/shared.d.ts", "", true),
		..SourceFileParseOptions::default()
	};
	let sentinel = rotor_tsrs::parser::parse_source_file(opts.clone(), "export {};", ScriptKind::TS);
	let (started, started_rx) = channel::<()>();
	let (release, release_rx) = channel::<()>();
	let release_rx = Arc::new(Mutex::new(release_rx));
	let handles: Vec<_> = (0..2)
		.map(|_| {
			let cache = Arc::clone(&cache);
			let parses = Arc::clone(&parses);
			let opts = opts.clone();
			let started = started.clone();
			let release_rx = Arc::clone(&release_rx);
			std::thread::spawn(move || {
				cache.get_source_file(&opts, |_| {
					let _ = started.send(());
					let _ = crate::project::lock(&release_rx).recv();
					parses.fetch_add(1, Ordering::Relaxed);
					Some(sentinel)
				})
			})
		})
		.collect();
	started_rx.recv().expect("one parse starts");
	// Go's synctest.Wait: give the second caller time to reach the in-flight parse.
	std::thread::sleep(std::time::Duration::from_millis(50));
	assert!(started_rx.try_recv().is_err(), "in-flight parses = 2, want 1");
	drop(release);
	for handle in handles {
		assert!(handle.join().expect("the caller").is_some(), "a caller got no file");
	}
	assert_eq!(parses.load(Ordering::Relaxed), 1, "parses");
	assert!(
		cache.misses() == 1 && cache.hits() >= 1,
		"hits={} misses={}, want a miss then a hit",
		cache.hits(),
		cache.misses()
	);
}

/// `writeSharedDeclSolutionProject`
fn write_shared_decl_solution_project(dir: &str, shared_decl: &str) {
	std::fs::create_dir_all(join(dir, &["src"])).expect("the directory");
	let config = format!(
		r#"{{"compilerOptions":{{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"}},"files":["src/main.ts","src/globals.d.ts","{shared_decl}"]}}"#
	);
	for (path, content) in [
		("tsconfig.json", config.as_str()),
		("package.json", r#"{"name":"@scope/solution-child"}"#),
		("src/globals.d.ts", NO_LIB_GLOBAL_STUBS),
		("src/main.ts", "export const value = 1;\n"),
	] {
		std::fs::write(join(dir, &[path]), content).expect("the file writes");
	}
}

#[test]
fn solution_parse_cache_hits_shared_declarations() {
	let temp = Temp::new();
	let root = temp.dir();
	temp.write("shared/index.d.ts", "export declare const shared: number;\n");
	write_solution_config(&root, "tsconfig.json", &["./left", "./right"], true);
	write_shared_decl_solution_project(&join(&root, &["left"]), "../shared/index.d.ts");
	write_shared_decl_solution_project(&join(&root, &["right"]), "../shared/index.d.ts");
	let timings = Arc::new(BuildTimings::new());
	let options = ProjectOptions {
		builders: Some(1),
		timings: Some(Arc::clone(&timings)),
		..ProjectOptions::default()
	};
	large_stack(|| {
		if let Err(failure) = build_solution_with_options(&join(&root, &["tsconfig.json"]), &options) {
			panic!("BuildSolutionWithOptions: {failure} ({:?})", failure.failure.messages());
		}
	});
	let counts = timings.report().counts;
	assert!(
		counts.parse_cache_misses != 0,
		"parse cache misses = 0, want the shared declaration parsed at least once"
	);
	assert!(
		counts.parse_cache_hits != 0,
		"parse cache hits = 0, want the second project to reuse the shared declaration"
	);
}
