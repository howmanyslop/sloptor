//! `flamework_program_update_test.go`

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rotor_support::{fsutil, gopath};
use rotor_tsrs::P;
use rotor_tsrs::ast::{ExternalModuleIndicatorOptions, SourceFile, SourceFileParseOptions};
use rotor_tsrs::compiler::{CompilerHost, new_compiler_host};
use rotor_tsrs::tscore::ScriptKind;
use rotor_tsrs::tsoptions::ParsedCommandLine;
use rotor_tsrs::tspath::Path;
use rotor_tsrs::vfs::{FS, vfstest, wrapvfs};

use super::{Temp, large_stack, write_project};
use crate::flamework_program_update::{NativeOverlayHost, new_native_overlay_host, update_program_with_text_overlays};
use crate::incremental_manifest::normalize_source_file_path;
use crate::project::{ProjectOptions, new_project_program_with_options};
use crate::solution_cache::{SolutionCompileCache, declaration_source_file_parse_options, wrap};

/// A `'static` reference to `value`, which the test leaks: Go's hosts and file systems are
/// garbage collected.
fn leak<T>(value: T) -> &'static T {
	Box::leak(Box::new(value))
}

/// `compiler.NewCompilerHost(directory, fs, "", nil, nil)`
fn compiler_host(directory: &str, fs: Arc<dyn FS>) -> Arc<dyn CompilerHost> {
	new_compiler_host(directory, fs, "", None, None, None)
}

/// `newNativeOverlayHost(base, overlays)`, over the base host's own file system.
fn overlay_host(base: Arc<dyn CompilerHost>, overlays: &[(&str, &str)]) -> Arc<NativeOverlayHost> {
	let base: &'static Arc<dyn CompilerHost> = leak(base);
	let overlays = overlays
		.iter()
		.map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
		.collect();
	Arc::new(new_native_overlay_host(Arc::clone(base), base.fs(), overlays))
}

fn parse_options(file_name: &str, path: &str) -> SourceFileParseOptions {
	SourceFileParseOptions {
		file_name: file_name.to_owned(),
		path: Path::from(path),
		..SourceFileParseOptions::default()
	}
}

fn same(left: Option<P<SourceFile>>, right: Option<P<SourceFile>>) -> bool {
	left.is_some() && left == right
}

#[test]
fn native_overlay_host_preserves_shared_parses() {
	for (file_name, text) in [
		("/project/shared.d.ts", "export declare const shared: number;"),
		("/project/data.json", r#"{"shared": 1}"#),
	] {
		let name = gopath::base(file_name);
		let reads = Arc::new(AtomicUsize::new(0));
		let base_fs: &'static dyn FS = leak(vfstest::from_map([(file_name, text)], true));
		let counted = Arc::clone(&reads);
		let fs = wrapvfs::wrap(
			base_fs,
			wrapvfs::Replacements {
				read_file: Some(Box::new(move |path: &str| {
					counted.fetch_add(1, Ordering::Relaxed);
					base_fs.read_file(path)
				})),
				..Default::default()
			},
		);
		let cache = Arc::new(SolutionCompileCache::new());
		let base = wrap(&cache, compiler_host("/project", Arc::new(fs)), []);
		let opts = parse_options(file_name, file_name);
		let original = base.get_source_file(opts.clone());
		assert!(original.is_some(), "{name}: base host did not parse the shared file");
		let host = overlay_host(base, &[("/project/other.ts", "export const other = 2;")]);
		for _ in 0..2 {
			assert!(
				same(host.get_source_file(opts.clone()), original),
				"{name}: unrelated overlay reparsed a shared declaration/JSON file"
			);
		}
		let reads = reads.load(Ordering::Relaxed);
		assert!(
			reads == 1 && cache.hits() == 2 && cache.misses() == 1,
			"{name}: reads={reads} hits={} misses={}, want 1 read, 2 hits, 1 miss",
			cache.hits(),
			cache.misses()
		);
		for variant in [
			parse_options(file_name, &format!("/another{file_name}")),
			SourceFileParseOptions {
				external_module_indicator_options: ExternalModuleIndicatorOptions {
					force: true,
					..ExternalModuleIndicatorOptions::default()
				},
				..opts.clone()
			},
		] {
			let parsed = host.get_source_file(variant.clone());
			assert!(
				parsed.is_some_and(|parsed| Some(parsed) != original && *parsed.parse_options() == variant),
				"{name}: overlay host did not preserve distinct source parse options"
			);
			assert!(
				same(host.get_source_file(variant), parsed),
				"{name}: parse-options variant was not shared through the underlying cache"
			);
		}
	}
}

#[test]
fn native_overlay_host_keeps_transformed_files_out_of_shared_cache() {
	for (file_name, original_text, transformed) in [
		(
			"/project/shared.d.ts",
			"export declare const original: number;",
			"export declare const transformed: string;",
		),
		("/project/data.json", r#"{"original": 1}"#, r#"{"transformed": 2}"#),
	] {
		let name = gopath::base(file_name);
		let cache = Arc::new(SolutionCompileCache::new());
		let fs = vfstest::from_map([(file_name, original_text)], true);
		let base = wrap(&cache, compiler_host("/project", Arc::new(fs)), []);
		let opts = parse_options(file_name, file_name);
		let original = base.get_source_file(opts.clone());
		assert!(original.is_some(), "{name}: base host did not parse the original file");
		let host = overlay_host(Arc::clone(&base), &[(file_name, transformed)]);
		let first = host.get_source_file(opts.clone());
		let second = host.get_source_file(opts.clone());
		let (Some(first), Some(second)) = (first, second) else {
			panic!("{name}: transformed declaration/JSON was not freshly parsed from its overlay");
		};
		assert!(
			Some(first) != original
				&& first != second
				&& first.text() == transformed
				&& second.text() == transformed
				&& *first.parse_options() == opts,
			"{name}: transformed declaration/JSON was not freshly parsed from its overlay"
		);
		assert_eq!(
			host.fs().read_file(file_name).as_deref(),
			Some(first.text()),
			"{name}: overlay filesystem and parsed source disagree"
		);
		let got = base.get_source_file(opts);
		assert!(
			same(got, original) && got.is_some_and(|got| got.text() == original_text),
			"{name}: transformed file contaminated the underlying shared parse cache"
		);
		assert!(
			cache.misses() == 1 && cache.hits() == 1,
			"{name}: overlay used shared cache: hits={} misses={}",
			cache.hits(),
			cache.misses()
		);
	}
}

/// `nativeOverlayTestHost`: a host whose sources come from `get_source_file`.
struct TestHost {
	inner: Arc<dyn CompilerHost>,
	get_source_file: Box<dyn Fn(SourceFileParseOptions) -> Option<P<SourceFile>> + Send + Sync>,
}

impl CompilerHost for TestHost {
	fn fs(&self) -> &dyn FS {
		self.inner.fs()
	}

	fn default_library_path(&self) -> &str {
		self.inner.default_library_path()
	}

	fn get_current_directory(&self) -> &str {
		self.inner.get_current_directory()
	}

	fn trace(&self, msg: &'static rotor_tsrs::diagnostics::Message, args: &[&dyn fmt::Display]) {
		self.inner.trace(msg, args);
	}

	fn get_source_file(&self, opts: SourceFileParseOptions) -> Option<P<SourceFile>> {
		(self.get_source_file)(opts)
	}

	fn get_content_mapped_source_files(
		&self,
		parse_options: SourceFileParseOptions,
		mapper: &rotor_tsrs::contentmapper::Mapper,
	) -> Result<rotor_tsrs::contentmapper::SourceFiles, rotor_tsrs::contentmapper::Error> {
		self.inner.get_content_mapped_source_files(parse_options, mapper)
	}

	fn content_mapper_project(&self) -> Option<Arc<dyn rotor_tsrs::contentmapper::Project>> {
		self.inner.content_mapper_project()
	}

	fn get_resolved_project_reference(&self, file_name: &str, path: Path) -> Option<P<ParsedCommandLine>> {
		self.inner.get_resolved_project_reference(file_name, path)
	}
}

#[test]
fn native_overlay_host_preserves_custom_host_and_parse_options() {
	let opts = SourceFileParseOptions {
		external_module_indicator_options: ExternalModuleIndicatorOptions { jsx: true, force: true },
		..parse_options("/project/custom.tsx", "/canonical/custom.tsx")
	};
	let original = rotor_tsrs::parser::parse_source_file(opts.clone(), "const custom = <Frame />;", ScriptKind::TSX);
	let base_calls = Arc::new(AtomicUsize::new(0));
	let calls = Arc::clone(&base_calls);
	let expected = opts.clone();
	let base = TestHost {
		inner: compiler_host(
			"/project",
			Arc::new(vfstest::from_map(std::iter::empty::<(&str, &str)>(), true)),
		),
		get_source_file: Box::new(move |got| {
			calls.fetch_add(1, Ordering::Relaxed);
			assert_eq!(got, expected, "underlying host received parse options");
			Some(original)
		}),
	};
	let transformed_path = "/project/transformed.tsx";
	let host = overlay_host(Arc::new(base), &[(transformed_path, "const transformed = <Frame />;")]);
	assert!(
		!host.fs().file_exists(&opts.file_name) && host.get_source_file(opts.clone()) == Some(original),
		"overlay did not preserve the custom host's source absent from its filesystem"
	);
	let transformed_opts = SourceFileParseOptions {
		file_name: transformed_path.to_owned(),
		path: Path::from("/canonical/transformed.tsx"),
		..opts
	};
	let parsed = host.get_source_file(transformed_opts.clone());
	assert!(
		parsed.is_some_and(|parsed| *parsed.parse_options() == transformed_opts
			&& parsed.script_kind() == ScriptKind::TSX
			&& parsed.external_module_indicator().is_some()),
		"transformed source lost its filename, canonical path, module indicators or script kind"
	);
	assert_eq!(
		base_calls.load(Ordering::Relaxed),
		1,
		"overlay hit unexpectedly invoked the custom base host"
	);
}

#[test]
fn native_overlay_host_preserves_project_cache_exclusions() {
	let temp = write_project("@scope/overlay-exclusions", "");
	let dir = temp.dir();
	let config_path = gopath::to_slash(&temp.join("tsconfig.json").to_string_lossy()).into_owned();
	let caller_path = temp.join("src/caller.d.ts").to_string_lossy().into_owned();
	std::fs::write(&caller_path, "export declare const disk: number;").expect("write");
	let caller_text = "export declare const caller: string;";
	let cache = Arc::new(SolutionCompileCache::new());
	large_stack(|| {
		let (_, program) = new_project_program_with_options(
			&dir,
			"",
			&ProjectOptions {
				compile_cache: Some(Arc::clone(&cache)),
				overlays: BTreeMap::from([(caller_path.clone(), caller_text.to_owned())]),
				..ProjectOptions::default()
			},
		)
		.unwrap_or_else(|failure| {
			panic!(
				"newProjectProgramWithOptions: {} ({:?})",
				failure.error,
				failure.messages()
			)
		});
		let host = Arc::new(new_native_overlay_host(
			Arc::clone(program.host()),
			program.host().fs(),
			HashMap::from([("/project/unrelated.ts".to_owned(), "export {};".to_owned())]),
		));
		let (hits, misses) = (cache.hits(), cache.misses());
		// Go compares the synthetic declarations with its text constants, whose headers the port
		// keeps private to rotor_tsrs; their bodies identify them.
		for (file_name, body) in [
			(
				crate::envdecl::env_decl_path(&config_path),
				rotor_tsrs::project::ENV_DECL_BODY,
			),
			(
				crate::macrodecl::macro_decl_path(&config_path),
				rotor_tsrs::project::MACRO_DECL_BODY,
			),
			(caller_path.clone(), caller_text),
		] {
			let source = program
				.get_source_file(&file_name)
				.unwrap_or_else(|| panic!("project declaration not loaded: {file_name}"));
			let text = source.text();
			assert!(text.contains(body), "{file_name}: unexpected project text {text:?}");
			let opts = source.parse_options().clone();
			let (first, second) = (host.get_source_file(opts.clone()), host.get_source_file(opts));
			assert!(
				matches!((first, second), (Some(first), Some(second)) if first != second && first.text() == text && second.text() == text),
				"synthetic/caller declaration no longer isolated: {file_name}"
			);
		}
		assert!(
			cache.hits() == hits && cache.misses() == misses,
			"synthetic/caller declaration was admitted to the shared parse cache"
		);
	});
}

#[test]
fn native_overlay_host_preserves_case_insensitive_caller_exclusion() {
	let file_name = "/project/Caller.d.ts";
	let transformed = "export declare const caller: string;";
	let opts = parse_options(file_name, "/project/caller.d.ts");
	let fs: &'static dyn FS = leak(vfstest::from_map(
		[(file_name, "export declare const disk: number;")],
		false,
	));
	let cache = Arc::new(SolutionCompileCache::new());
	let base = compiler_host("/project", Arc::new(fs));
	assert!(
		wrap(&cache, base, []).get_source_file(opts.clone()).is_some(),
		"initial disk declaration was not cached"
	);
	// Go's `newOverlayFS(fs, "", {"/PROJECT/cALLER.d.ts": transformed})`, which matches the
	// overlay with the file system's case sensitivity.
	let caller = "/PROJECT/cALLER.d.ts".to_lowercase();
	let exists = caller.clone();
	let caller_fs = wrapvfs::wrap(
		fs,
		wrapvfs::Replacements {
			file_exists: Some(Box::new(move |path: &str| {
				path.to_lowercase() == exists || fs.file_exists(path)
			})),
			read_file: Some(Box::new(move |path: &str| {
				if path.to_lowercase() == caller {
					return Some(transformed.to_owned());
				}
				fs.read_file(path)
			})),
			..Default::default()
		},
	);
	let caller_host = wrap(
		&cache,
		compiler_host("/project", Arc::new(caller_fs)),
		[normalize_source_file_path("/PROJECT/cALLER.d.ts")],
	);
	let host = overlay_host(caller_host, &[("/project/unrelated.ts", "export {};")]);
	let (first, second) = (host.get_source_file(opts.clone()), host.get_source_file(opts));
	assert!(
		matches!((first, second), (Some(first), Some(second)) if first != second && first.text() == transformed && second.text() == transformed),
		"case-insensitive caller declaration used the primed disk parse cache"
	);
	assert!(
		cache.hits() == 0 && cache.misses() == 1,
		"excluded caller declaration touched the shared parse cache"
	);
}

#[test]
fn native_overlay_host_stacks_overlays_and_aliases() {
	let cache = Arc::new(SolutionCompileCache::new());
	let base = wrap(
		&cache,
		compiler_host(
			"/project",
			Arc::new(vfstest::from_map(
				[("/project/shared.d.ts", "export declare const shared: number;")],
				false,
			)),
		),
		[],
	);
	let shared_opts = parse_options("/project/shared.d.ts", "/project/shared.d.ts");
	let shared = base.get_source_file(shared_opts.clone());
	let first = overlay_host(
		base,
		&[
			("/Project/First.ts", "export const first = 1;"),
			("/Project/Same.ts", "export const same = 1;"),
		],
	);
	let second = overlay_host(first, &[("/project/same.ts", "export const same = 2;")]);
	for (path, text) in [
		("/PROJECT/FIRST.ts", "export const first = 1;"),
		("/PROJECT/SAME.ts", "export const same = 2;"),
	] {
		let parsed = second.get_source_file(parse_options(path, path));
		assert!(
			parsed.is_some_and(|parsed| parsed.text() == text),
			"stacked overlay lost source {path}"
		);
		assert!(
			second.fs().read_file(path).as_deref() == Some(text) && second.fs().file_exists(path),
			"stacked filesystem disagrees with parsed source {path}"
		);
	}
	assert!(
		same(second.get_source_file(shared_opts), shared),
		"stacked overlays bypassed the underlying shared declaration"
	);
}

#[test]
fn native_overlay_host_uses_physical_aliases() {
	let temp = Temp::new();
	let dir = temp.dir();
	let physical_path = temp.join("source.ts");
	std::fs::write(&physical_path, "export const disk = 1;").expect("write");
	// TempDir can contain Windows short names. Physical aliases use the resolved name; lookups do
	// not resolve every possible spelling of the same file.
	let physical_path = fsutil::eval_symlinks(&physical_path.to_string_lossy()).expect("EvalSymlinks");
	let alias_temp = Temp::new();
	let alias_dir = alias_temp.join("project");
	super::solution_flamework_integration::symlink(std::path::Path::new(&dir), &alias_dir);
	let text = "export const transformed = 2;";
	let base = compiler_host(&gopath::to_slash(&dir), Arc::new(rotor_tsrs::vfs::osvfs::fs()));
	let alias_path = alias_dir.join("source.ts").to_string_lossy().into_owned();
	let host = overlay_host(base, &[(&alias_path, text)]);
	for path in [alias_path.as_str(), physical_path.as_str()] {
		let parsed = host.get_source_file(declaration_source_file_parse_options(path));
		assert!(
			parsed.is_some_and(|parsed| parsed.text() == text),
			"source path {path:?} did not receive its symlinked overlay"
		);
		assert_eq!(
			host.fs().read_file(path).as_deref(),
			Some(text),
			"source read {path:?} disagrees with its parsed overlay"
		);
	}
}

#[test]
fn text_overlay_full_fallback_preserves_shared_declarations() {
	let temp = write_project("@scope/sparse-native-update", "");
	std::fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
	for (name, text) in [
		("a.ts", "export const a = 1;\n"),
		("b.ts", "export const b = 1;\n"),
		("shared.d.ts", "export declare const shared: number;\n"),
	] {
		temp.write(&format!("src/{name}"), text);
	}
	let dir = temp.dir();
	let cache = Arc::new(SolutionCompileCache::new());
	large_stack(|| {
		// Go wraps the loaded program's host in the cache and builds a program over it; the port
		// loads the program through the cache.
		let (_, program) = new_project_program_with_options(
			&dir,
			"",
			&ProjectOptions {
				compile_cache: Some(Arc::clone(&cache)),
				..ProjectOptions::default()
			},
		)
		.unwrap_or_else(|failure| panic!("newProjectProgram: {} ({:?})", failure.error, failure.messages()));
		let shared = program
			.get_source_file(&format!("{dir}/src/shared.d.ts"))
			.expect("shared declaration missing from original Program");
		let extra_path = format!("{dir}/src/extra.ts");
		std::fs::write(&extra_path, "export const extra = 1;\n").expect("write");
		assert!(
			program.get_source_file(&extra_path).is_none(),
			"new module was already loaded before the overlay"
		);
		let a_path = format!("{dir}/src/a.ts");
		let b_path = format!("{dir}/src/b.ts");
		let a_text = "import { extra } from \"./extra\"; export const a = extra;\n";
		let b_text = "export const b = 2;\n";
		let overlays = BTreeMap::from([(a_path.clone(), a_text.to_owned()), (b_path.clone(), b_text.to_owned())]);
		let (updated, reused) =
			update_program_with_text_overlays(program, &overlays).unwrap_or_else(|error| panic!("{error}"));
		assert!(!reused, "new import did not force the full Program rebuild");
		assert!(
			updated.get_source_file(&extra_path).is_some(),
			"full fallback did not load the new imported module"
		);
		for (path, text) in [(&a_path, a_text), (&b_path, b_text)] {
			let got = updated.get_source_file(path);
			assert!(
				got.is_some_and(|got| got.text() == text && Some(got) != program.get_source_file(path)),
				"full fallback lost transformed text or reused the changed source: {path}"
			);
		}
		assert!(
			updated.get_source_file(shared.file_name()) == Some(shared),
			"full fallback reparsed the unchanged shared declaration"
		);
	});
}
