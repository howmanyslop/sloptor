// program_test.go (the parts about program reuse and pluggable checker pools; tracing is not ported).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tsrs_core::P;
use tsrs_core::tspath::Path;
use tsrs_tsoptions::{self as tsoptions, ParseConfigHost};
use tsrs_vfs::{FS, vfstest};

use tsrs_compiler::checkerpool::CheckerPoolImpl;
use tsrs_compiler::{
    CheckerHandle, CheckerPool, CompilerHost, Context, CreateCheckerPool, PooledChecker, Program,
    ProgramOptions, new_compiler_host, new_program,
};

struct ParseConfigHostImpl {
    fs: Arc<dyn FS>,
}

impl ParseConfigHost for ParseConfigHostImpl {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &'static str {
        "/"
    }
}

fn host_for(files: &[(&str, &str)]) -> Arc<dyn CompilerHost> {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().map(|&(k, v)| (k, v)), true));
    new_compiler_host("/", fs, "", None, None, None)
}

#[test]
fn later_relative_import_restarts_package_imports_at_lower_depth() {
    let files = [
        (
            "/src/tsconfig.json",
            vfstest::MapFile::from(
                r#"{"compilerOptions":{"noLib":true,"module":"commonjs"},"files":["b.ts","a.ts"]}"#,
            ),
        ),
        (
            "/src/a.ts",
            vfstest::MapFile::from("import { util } from './lib/index'; export const a = util;"),
        ),
        ("/src/b.ts", vfstest::MapFile::from("import { util } from 'lib'; export const b = util;")),
        ("/src/lib/package.json", vfstest::MapFile::from(r#"{"name":"lib","types":"index.ts"}"#)),
        ("/src/lib/index.ts", vfstest::MapFile::from("export { util } from './util';")),
        ("/src/lib/util.ts", vfstest::MapFile::from("export const util = 1;")),
        ("/src/node_modules/lib/package.json", vfstest::symlink("/src/lib/package.json")),
        ("/src/node_modules/lib/index.ts", vfstest::symlink("/src/lib/index.ts")),
        ("/src/node_modules/lib/util.ts", vfstest::symlink("/src/lib/util.ts")),
    ];
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files, true));
    let config_host: &'static ParseConfigHostImpl =
        Box::leak(Box::new(ParseConfigHostImpl { fs: Arc::clone(&fs) }));
    let (config, diagnostics) = tsoptions::get_parsed_command_line_of_config_file(
        "/src/tsconfig.json",
        None,
        None,
        config_host,
        None,
    );
    assert_eq!(diagnostics, [] as [tsrs_core::P<tsrs_ast::Diagnostic>; 0]);
    let program = new_program(ProgramOptions::new(
        P::new(config.unwrap()),
        new_compiler_host("/", fs, "", None, None, None),
    ));
    let util = program.get_source_file("/src/lib/util.ts").expect("transitive util import");
    assert!(!program.is_source_file_from_external_library(util));
    assert!(program.source_file_may_be_emitted(util, false));
}

// program_test.go: TestImportSourceProgram
#[test]
fn source_phase_imports_do_not_resolve_or_keep_stale_programs() {
    for (source, evaluation) in [
        ("import source a from \"./a.js\";", "import { a as value } from \"./a.js\";"),
        ("import.source(\"./a.js\");", "import(\"./a.js\");"),
    ] {
        let config_file =
            r#"{"compilerOptions":{"module":"esnext","noLib":true},"files":["index.ts"]}"#;
        let content =
            format!("{source} import source b from \"missing\"; import.source(\"other\");");
        let files = [
            ("/src/tsconfig.json", config_file),
            ("/src/index.ts", content.as_str()),
            ("/src/a.ts", "export const a = 1;"),
        ];
        let config = config_for(&files);
        let program = new_program(ProgramOptions::new(config, host_for(&files)));
        assert!(program.get_source_file("/src/a.ts").is_none());
        let file = program.get_source_file("/src/index.ts").unwrap();
        assert!(
            program
                .get_resolved_modules()
                .get(file.path())
                .is_none_or(tsrs_module::ModeAwareCache::is_empty)
        );
        assert!(program.get_unresolved_imports().is_empty());
        assert!(program.unresolved_package_names().is_empty());

        let edited = format!("{evaluation}{}", content.strip_prefix(source).unwrap());
        let edited_files = [files[0], ("/src/index.ts", edited.as_str()), files[2]];
        let (updated, _, reused) = program.update_program(
            &Path::from("/src/index.ts"),
            host_for(&edited_files),
            None,
            None,
        );
        assert!(!reused);
        assert!(updated.get_source_file("/src/a.ts").is_some());
        let updated_file = updated.get_source_file("/src/index.ts").unwrap();
        for &specifier in updated_file.imports() {
            let resolved =
                updated.get_resolved_module_from_module_specifier(updated_file, specifier);
            assert_eq!(
                resolved.is_some_and(|module| module.is_resolved()),
                !specifier.parent().is_some_and(tsrs_ast::is_source_phase_import)
            );
        }

        let (restored, _, reused) =
            updated.update_program(&Path::from("/src/index.ts"), host_for(&files), None, None);
        assert!(!reused);
        assert!(restored.get_source_file("/src/a.ts").is_none());
    }
}

fn config_for(files: &[(&str, &str)]) -> P<tsoptions::ParsedCommandLine> {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().map(|&(k, v)| (k, v)), true));
    let host: &'static ParseConfigHostImpl = Box::leak(Box::new(ParseConfigHostImpl { fs }));
    let (config, diagnostics) = tsoptions::get_parsed_command_line_of_config_file(
        "/src/tsconfig.json",
        None,
        None,
        host,
        None,
    );
    assert_eq!(diagnostics, [] as [tsrs_core::P<tsrs_ast::Diagnostic>; 0]);
    P::new(config.unwrap())
}

// A pool outside the built-in one: hands out checkers through `CheckerHandle::from_raw` and takes them back on
// release, like the project system's pool.
struct TestPool {
    program: &'static Program,
    idle: Arc<Mutex<Vec<PooledChecker>>>,
    acquired: Arc<AtomicUsize>,
}

impl CheckerPool for TestPool {
    fn get_checker(&self, _ctx: &Context, _file: Option<P<tsrs_ast::SourceFile>>) -> CheckerHandle {
        self.acquired.fetch_add(1, Ordering::Relaxed);
        let mut checker = self
            .idle
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| PooledChecker::new(tsrs_checker::new_checker(self.program)));
        let ptr = checker.as_non_null();
        let idle = std::sync::Arc::clone(&self.idle);
        // SAFETY: the checker is out of the idle list (held) until the release function puts it back.
        unsafe { CheckerHandle::from_raw(ptr, move || idle.lock().unwrap().push(checker)) }
    }
}

// program_test.go:95 TestProgramHostsAndFactories
#[test]
fn program_hosts_and_factories() {
    let index = "/// <reference types=\"dep\" />\nimport { value } from \"./dep.js\"; export const result = value;";
    let files = [
        (
            "/src/tsconfig.json",
            r#"{"compilerOptions":{"noLib":true,"module":"nodenext"},"files":["index.ts"]}"#,
        ),
        ("/src/index.ts", index),
        ("/src/dep.ts", "export const value = 1;"),
        ("/src/node_modules/@types/dep/index.d.ts", "export {};"),
    ];
    let config = config_for(&files);
    let pools = Arc::new(AtomicUsize::new(0));
    let resolvers = Arc::new(AtomicUsize::new(0));
    let mut opts = ProgramOptions::new(config, host_for(&files));
    let pools_c = std::sync::Arc::clone(&pools);
    opts.create_checker_pool = Some(Arc::new(move |p: &'static Program| -> Box<dyn CheckerPool> {
        pools_c.fetch_add(1, Ordering::Relaxed);
        Box::new(CheckerPoolImpl::new(p, None))
    }));
    let resolvers_c = std::sync::Arc::clone(&resolvers);
    opts.create_module_resolver = Some(Arc::new(
        move |options: tsrs_module::ResolverOptions<'static>| -> Box<dyn tsrs_module::Resolver> {
            resolvers_c.fetch_add(1, Ordering::Relaxed);
            Box::new(tsrs_module::new_resolver(options))
        },
    ));
    let p = new_program(opts);
    assert_eq!(pools.load(Ordering::Relaxed), 1);
    assert_eq!(resolvers.load(Ordering::Relaxed), 1);
    let old_file = p.get_source_file("/src/index.ts").unwrap();
    let resolved =
        p.get_resolved_module_from_module_specifier(old_file, old_file.imports()[0]).unwrap();
    assert!(resolved.is_resolved());
    let resolved_type_ref = p
        .get_resolved_type_reference_directive_from_type_reference_directive(
            old_file.type_reference_directives.get()[0],
            old_file,
        )
        .unwrap();
    assert!(resolved_type_ref.is_resolved());

    let new_index = format!("\n{index}");
    let new_files = [
        files[0],
        ("/src/index.ts", new_index.as_str()),
        files[2],
        files[3],
        ("/probe/package.json", r#"{"name":"new-host"}"#),
    ];
    let new_host = host_for(&new_files);
    let pools_c = std::sync::Arc::clone(&pools);
    let new_host_c = std::sync::Arc::clone(&new_host);
    let create: CreateCheckerPool = Arc::new(move |p: &'static Program| -> Box<dyn CheckerPool> {
        pools_c.fetch_add(1, Ordering::Relaxed);
        assert!(Arc::ptr_eq(p.host(), &new_host_c));
        assert_eq!(p.get_source_file("/src/index.ts").unwrap().text(), format!("\n{index}"));
        Box::new(CheckerPoolImpl::new(p, None))
    });
    let (cloned, changed, reused) = p.reuse_program(
        &Path::from("/src/index.ts"),
        std::sync::Arc::clone(&new_host),
        Some(create),
        Some(Arc::new(|_| {
            panic!("cloning must reuse resolution data without invoking construction callbacks")
        })),
    );
    assert!(reused);
    let cloned = cloned.unwrap();
    let changed = changed.unwrap();
    assert_eq!(Some(changed), cloned.get_source_file("/src/index.ts"));
    assert_eq!(Some(old_file), p.get_source_file("/src/index.ts"));
    assert_eq!(
        cloned.get_resolved_module_from_module_specifier(changed, changed.imports()[0]),
        Some(resolved)
    );
    assert_eq!(
        cloned.get_resolved_type_reference_directive_from_type_reference_directive(
            changed.type_reference_directives.get()[0],
            changed
        ),
        Some(resolved_type_ref)
    );
    assert_ne!(cloned.resolution_data, p.resolution_data);
    assert!(cloned.get_package_json_info("/probe/package.json").is_some());
    assert!(
        p.get_package_json_info("/probe/package.json").is_none(),
        "new lazy lookups must not populate the old generation's cache"
    );
    assert_eq!(pools.load(Ordering::Relaxed), 2);
    assert_eq!(resolvers.load(Ordering::Relaxed), 1);

    let (defaults, _, reused) = cloned.reuse_program(
        &Path::from("/src/index.ts"),
        std::sync::Arc::clone(&new_host),
        None,
        None,
    );
    assert!(reused);
    assert!(defaults.unwrap().compiler_checker_pool().is_some());
    assert_eq!(pools.load(Ordering::Relaxed), 2);

    let rebuild_files = [
        files[0],
        ("/src/index.ts", "import \"./other.js\";"),
        files[2],
        files[3],
        ("/src/other.ts", "export {};"),
    ];
    let rebuild_host = host_for(&rebuild_files);
    let (rebuilt, _, reused) = p.update_program(
        &Path::from("/src/index.ts"),
        std::sync::Arc::clone(&rebuild_host),
        None,
        None,
    );
    assert!(!reused);
    assert!(rebuilt.compiler_checker_pool().is_some());
    assert!(Arc::ptr_eq(rebuilt.host(), &rebuild_host));
    assert!(rebuilt.get_source_file("/src/other.ts").is_some());
    assert_eq!(pools.load(Ordering::Relaxed), 2);
    assert_eq!(resolvers.load(Ordering::Relaxed), 1);
}

// tsrs-only: the diagnostics entry points give the same results through an external pool (Go's per-file
// `GetChecker` path) as through the built-in grouped pool, and every acquired checker is released.
#[test]
fn external_checker_pool_diagnostics() {
    let files = [
        (
            "/src/tsconfig.json",
            r#"{"compilerOptions":{"noLib":true,"strict":true},"files":["a.ts","b.ts"]}"#,
        ),
        ("/src/a.ts", "export const a: number = \"x\";\nexport function f(x) { return x; }"),
        (
            "/src/b.ts",
            "import { a } from \"./a\";\nconst b: string = a;\n// @ts-expect-error\nconst ok = 1;",
        ),
    ];
    let config = config_for(&files);
    let builtin = new_program(ProgramOptions::new(config, host_for(&files)));
    let idle = Arc::new(Mutex::new(Vec::new()));
    let acquired = Arc::new(AtomicUsize::new(0));
    let mut opts = ProgramOptions::new(config, host_for(&files));
    let (idle_c, acquired_c) = (std::sync::Arc::clone(&idle), std::sync::Arc::clone(&acquired));
    opts.create_checker_pool =
        Some(Arc::new(move |program: &'static Program| -> Box<dyn CheckerPool> {
            Box::new(TestPool {
                program,
                idle: std::sync::Arc::clone(&idle_c),
                acquired: std::sync::Arc::clone(&acquired_c),
            })
        }));
    let external = new_program(opts);
    assert!(external.compiler_checker_pool().is_none());

    let ctx = Context::default();
    let messages = |program: &'static Program| -> Vec<String> {
        program
            .get_semantic_diagnostics(&ctx, None)
            .iter()
            .chain(program.get_suggestion_diagnostics(&ctx, None).iter())
            .map(|d| {
                let pos = d.pos().map_or_else(|| "-".to_string(), |pos| pos.to_string());
                format!("{}:{pos}:{}", d.file().unwrap().file_name(), d.code())
            })
            .collect()
    };
    let expected = messages(builtin);
    assert!(expected.len() >= 4, "{expected:?}");
    assert_eq!(messages(external), expected);
    let file = external.get_source_file("/src/b.ts").unwrap();
    let single: Vec<i32> =
        external.get_semantic_diagnostics(&ctx, Some(file)).iter().map(|d| d.code()).collect();
    assert_eq!(single, vec![2322, 2578]);
    assert_eq!(
        external.get_global_diagnostics(&ctx),
        [] as [tsrs_core::P<tsrs_ast::Diagnostic>; 0]
    );
    {
        let c = external.get_type_checker_for_file(&ctx, file);
        assert!(!c.was_canceled());
    }
    assert_eq!(acquired.load(Ordering::Relaxed), 6);
    assert!(!idle.lock().unwrap().is_empty());
}

// tsrs-only (fileregions.rs): after a type-check pass that freed a leaf, asking for that one file's diagnostics would
// read its freed tree and bind diagnostics; it panics instead (`fileregions::assert_not_freed`). The CLI never asks.
#[test]
#[should_panic(expected = "was freed after the type-check pass")]
fn single_file_diagnostics_of_a_freed_leaf_panic() {
    // The file is predicted (`.test.`) and loaded as a root file, so it gets a region; nothing imports it.
    let files = [
        (
            "/src/tsconfig.json",
            r#"{"compilerOptions":{"noLib":true,"noEmit":true},"files":["a.test.ts"]}"#,
        ),
        ("/src/a.test.ts", "export const a: number = 1;"),
    ];
    tsrs_compiler::fileregions::enable(
        tsrs_compiler::LeafSettings {
            mode: tsrs_compiler::LeafMode::Free,
            stats: false,
            every_file: false,
        },
        "/",
    );
    let mut opts = ProgramOptions::new(config_for(&files), host_for(&files));
    opts.leaf_files = tsrs_compiler::LeafMode::Free;
    let program = new_program(opts);
    let ctx = Context::default();
    let _ = program.get_semantic_diagnostics(&ctx, None);
    let file = program.get_source_file("/src/a.test.ts").unwrap();
    assert!(file.is_check_leaf());
    let _ = program.get_semantic_diagnostics(&ctx, Some(file));
}

// tsrs-only: a program made by `reuse_program` takes over the file assignment of the pool of the program it reuses
// (checkerpool.rs `inherited_associations`) instead of computing its own, once that pool has assigned its files.
#[test]
fn a_reused_program_keeps_the_checker_assignment() {
    // Assignment is process-wide; exercise both defaults without racing the other compiler tests.
    let Ok(assignment) = std::env::var("TSRS_CHECKER_ASSIGNMENT") else {
        for assignment in ["go", "locality"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "program_test::a_reused_program_keeps_the_checker_assignment"])
                .env("TSRS_CHECKER_ASSIGNMENT", assignment)
                .output()
                .unwrap();
            assert!(output.status.success(), "{assignment}: {output:?}");
        }
        return;
    };
    let files = [
        ("/src/tsconfig.json", r#"{"compilerOptions":{"noLib":true,"types":[],"checkers":2}}"#),
        ("/src/a.ts", "export const a = 1;"),
        ("/src/b.ts", "import { a } from './a'; export const b = a;"),
        ("/src/c.ts", "export const c = 1;"),
        ("/src/d.ts", "import { c } from './c'; export const d = c;"),
    ];
    let edited =
        [files[0], ("/src/a.ts", "// edited\nexport const a = 1;"), files[2], files[3], files[4]];
    let a = Path::from("/src/a.ts");

    let p = new_program(ProgramOptions::new(config_for(&files), host_for(&files)));
    let (unassigned, _, reused) = p.reuse_program(&a, host_for(&edited), None, None);
    assert!(reused);
    assert!(
        unassigned.unwrap().pool().inherited_associations.is_none(),
        "the old pool had not assigned its files yet"
    );

    let associations = p.pool().state().associations.clone();
    assert_eq!(associations.len(), 4);
    let (cloned, _, reused) = p.reuse_program(&a, host_for(&edited), None, None);
    assert!(reused);
    let cloned = cloned.unwrap();
    assert_eq!(
        cloned.pool().inherited_associations.as_deref(),
        (assignment == "locality").then_some(associations.as_slice())
    );
    assert_eq!(cloned.pool().state().associations, associations);
}

// tsrs-only: a program made by `reuse_program` takes over the import graph of tsgo's default-pool slots
// (`Program::tsgo_import_adjacency`) and computes the same slots as a fresh program of the edited files, also when
// the edit changes the reparsed file's weight.
#[test]
fn a_reused_program_computes_the_tsgo_slots_of_a_fresh_one() {
    let files = [
        ("/src/tsconfig.json", r#"{"compilerOptions":{"noLib":true,"types":[]}}"#),
        ("/src/a.ts", "export const a = 1;"),
        ("/src/b.ts", "import { a } from './a'; export const b = a;"),
        ("/src/c.ts", "import { b } from './b'; export const c = b;"),
        ("/src/d.ts", "export const d = 1;"),
        ("/src/e.ts", "import { d } from './d'; import { a } from './a'; export const e = d + a;"),
    ];
    let comment = format!("// {}\n", "x".repeat(20_000));
    let edited_a = format!("{comment}export const a = 1;");
    let edited =
        [files[0], ("/src/a.ts", edited_a.as_str()), files[2], files[3], files[4], files[5]];
    let by_path = |program: &'static Program| -> Vec<(String, (u32, u32))> {
        let mut slots: Vec<_> = tsrs_compiler::checkerpool::tsgo_default_check_slots(program)
            .into_iter()
            .map(|(file, slot)| (file.file_name().to_string(), slot))
            .collect();
        slots.sort();
        slots
    };

    let p = new_program(ProgramOptions::new(config_for(&files), host_for(&files)));
    let before = by_path(p);
    let (reused_program, _, reused) =
        p.reuse_program(&Path::from("/src/a.ts"), host_for(&edited), None, None);
    assert!(reused);
    let reused_program = reused_program.unwrap();
    assert!(Arc::ptr_eq(
        reused_program.tsgo_import_adjacency.get().unwrap(),
        p.tsgo_import_adjacency.get().unwrap()
    ));
    let fresh = by_path(new_program(ProgramOptions::new(config_for(&edited), host_for(&edited))));
    assert_ne!(before, fresh, "the edit must move a slot");
    assert_eq!(by_path(reused_program), fresh);
}

// A spread lowered for ES2015 under a type assertion: the return's expression is a partially emitted expression
// around the synthesized `Object.assign` call. The printer's leading-newline check reads the comments at the
// synthesized node's position, of which Go's scanner yields none (position -1); tsrs panicked there.
#[test]
fn emit_of_a_synthesized_return_expression() {
    let files = [
        (
            "/src/tsconfig.json",
            r#"{ "compilerOptions": { "target": "es2015", "strict": true, "types": [] } }"#,
        ),
        ("/src/a.ts", "export function f(p: object) {\n    return { a: 1, ...p } as object;\n}\n"),
    ];
    let program = new_program(ProgramOptions::new(config_for(&files), host_for(&files)));
    let written = Mutex::new(Vec::new());
    let write_file =
        |name: &str, text: &str, _: &mut tsrs_compiler::WriteFileData| -> Result<(), String> {
            written.lock().unwrap().push((name.to_string(), text.to_string()));
            Ok(())
        };
    program.emit(
        &Context::default(),
        &tsrs_compiler::EmitOptions { write_file: Some(&write_file), ..Default::default() },
    );
    // tsgo's output.
    assert_eq!(
        *written.lock().unwrap(),
        [(
            "/src/a.js".to_string(),
            "export function f(p) {\n    return Object.assign({ a: 1 }, p);\n}\n".to_string()
        )]
    );
}

// A file's JS transform, declaration transform, and printers share one emit context and resolver.
// Re-emitting through the same checker must not keep the prior file's scratch node builder or emit state.
#[test]
fn repeated_js_and_declaration_emit_with_shared_context() {
    let files = [
        (
            "/src/tsconfig.json",
            r#"{"compilerOptions":{"noLib":true,"types":[],"target":"es2015","module":"commonjs","declaration":true,"sourceMap":true,"declarationMap":true},"files":["a.ts"]}"#,
        ),
        (
            "/src/a.ts",
            "export const value = { answer: 42 };\nexport function identity<T>(arg: T) { return arg; }\n",
        ),
    ];
    let program = new_program(ProgramOptions::new(config_for(&files), host_for(&files)));
    let written = Mutex::new(Vec::<(String, String)>::new());
    let write_file =
        |name: &str, text: &str, _: &mut tsrs_compiler::WriteFileData| -> Result<(), String> {
            written.lock().unwrap().push((name.to_string(), text.to_string()));
            Ok(())
        };
    let options =
        tsrs_compiler::EmitOptions { write_file: Some(&write_file), ..Default::default() };
    program.emit(&Context::default(), &options);
    let first = written.lock().unwrap().clone();
    assert!(
        first.iter().any(|(name, body)| name.ends_with("a.js") && body.contains("exports.value"))
    );
    assert!(
        first.iter().any(|(name, body)| name.ends_with("a.d.ts") && body.contains("identity<T>"))
    );
    assert!(first.iter().any(|(name, _)| name.ends_with("a.js.map")));
    assert!(first.iter().any(|(name, _)| name.ends_with("a.d.ts.map")));

    written.lock().unwrap().clear();
    program.emit(&Context::default(), &options);
    assert_eq!(*written.lock().unwrap(), first);
}
