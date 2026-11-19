//! `solution_cache_test.go`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use rotor_tsrs::ast::SourceFileParseOptions;
use rotor_tsrs::tscore::ScriptKind;

use super::solution::{join, write_solution_config};
use super::{NO_LIB_GLOBAL_STUBS, Temp, large_stack};
use crate::flamework_program_update::update_program_with_text_overlays;
use crate::project::{ProjectOptions, new_project_program_with_options};
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

/// One project of [`rebuilt_program_declaration_outlives_its_project`]: a config, the global
/// stubs and `src/main.ts`.
fn write_late_import_project(temp: &Temp, name: &str, main: &str) -> String {
	temp.write(&format!("{name}/tsconfig.json"), super::WRITE_PROJECT_TSCONFIG);
	temp.write(
		&format!("{name}/package.json"),
		format!(r#"{{"name":"@scope/{name}"}}"#),
	);
	temp.write(&format!("{name}/src/globals.d.ts"), NO_LIB_GLOBAL_STUBS);
	temp.write(&format!("{name}/src/main.ts"), main);
	join(&temp.dir(), &[name])
}

/// A long-lived session frees each project's programs when its build ends, while the drain's
/// parse cache goes on handing its declarations to the projects that follow. A declaration no
/// project had loaded, which the first project's program reaches only once an edit makes it
/// build again in full, must still be readable when the second project asks the cache for it.
/// Poison mode makes a read of freed memory fault.
#[test]
#[expect(
	unsafe_code,
	reason = "each project's programs are freed with its scope, as a build frees them"
)]
fn rebuilt_program_declaration_outlives_its_project() {
	const LATE_IMPORT: &str = "import { late } from \"../../shared/late\";\n";
	rotor_tsrs::tscore::arena::enable_poison_mode();
	rotor_tsrs::project::set_long_lived(true);
	// The loader parses on its worker threads, or on the build's own thread when single-threaded.
	for single_threaded in [false, true] {
		let temp = Temp::new();
		temp.write("shared/late.d.ts", "export declare const late: number;\n");
		let left = write_late_import_project(&temp, "left", "export const value = 1;\n");
		let right = write_late_import_project(
			&temp,
			"right",
			&format!("{LATE_IMPORT}export const value: string = late;\n"),
		);
		let cache = Arc::new(SolutionCompileCache::new());
		let load = |dir: &str| {
			let options = ProjectOptions {
				compile_cache: Some(Arc::clone(&cache)),
				single_threaded: Some(single_threaded),
				..ProjectOptions::default()
			};
			let (_, program) = new_project_program_with_options(dir, "", &options)
				.unwrap_or_else(|failure| panic!("{dir}: {} ({:?})", failure.error, failure.messages()));
			program
		};
		let codes = large_stack(|| {
			{
				// SAFETY: nothing of the left project's programs is used after this block.
				let _programs = unsafe { rotor_tsrs::project::ProgramScope::enter() };
				let program = load(&left);
				let overlays = BTreeMap::from([(
					join(&left, &["src", "main.ts"]),
					format!("{LATE_IMPORT}export const value = late;\n"),
				)]);
				let (_, reused) =
					update_program_with_text_overlays(program, &overlays).unwrap_or_else(|error| panic!("{error}"));
				assert!(
					!reused,
					"the import of a file outside the program did not build it again"
				);
			}
			// SAFETY: only the diagnostic codes, plain numbers, leave the scope.
			let _programs = unsafe { rotor_tsrs::project::ProgramScope::enter() };
			let program = load(&right);
			let diagnostics = program.get_semantic_diagnostics(&rotor_tsrs::Context::default(), None);
			diagnostics
				.iter()
				.map(|diagnostic| diagnostic.code())
				.collect::<Vec<i32>>()
		});
		// `late` is a number, so the right project's `string` annotation is the one error (TS2322).
		assert_eq!(
			codes,
			[2322],
			"the right project's semantic diagnostics (single-threaded: {single_threaded})"
		);
	}
}
