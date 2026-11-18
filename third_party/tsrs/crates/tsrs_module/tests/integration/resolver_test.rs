use tsrs_module::resolver::*;
use tsrs_module::types::*;
use tsrs_module::util::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, mpsc};
use std::time::{Duration, SystemTime};

use tsrs_core::collections::new_ordered_map_with_size_hint;
use tsrs_core::{CompilerOptions, ModuleKind, ModuleResolutionKind, P, ScriptTarget, Tristate};
use tsrs_vfs::{Entries, FS, FileInfo, vfstest};

use tsrs_module::packagejson::InfoCacheEntryExt;

struct ResolutionHostStub {
    fs: Box<dyn FS>,
    cwd: String,
}

impl ResolutionHost for ResolutionHostStub {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &str {
        &self.cwd
    }
}

fn host(fs: impl FS + 'static, cwd: &str) -> &'static ResolutionHostStub {
    Box::leak(Box::new(ResolutionHostStub { fs: Box::new(fs), cwd: cwd.to_string() }))
}

fn bundler_options() -> P<CompilerOptions> {
    P::new(CompilerOptions {
        module_resolution: ModuleResolutionKind::BUNDLER,
        module: ModuleKind::ES_NEXT,
        target: ScriptTarget::ES_NEXT,
        ..Default::default()
    })
}

fn dynamic_options(module: ModuleKind, root_dirs: Option<Vec<String>>) -> P<CompilerOptions> {
    P::new(CompilerOptions {
        module_resolution: ModuleResolutionKind::BUNDLER,
        module,
        target: ScriptTarget::ES_NEXT,
        root_dirs,
        ..Default::default()
    })
}

fn resolve_dynamic(
    files: impl IntoIterator<Item = (&'static str, &'static str)>,
    options: P<CompilerOptions>,
    source_file: &str,
    specifier: &str,
    mode: ModuleKind,
) -> String {
    let fs = vfstest::from_map(files, true);
    let resolver = new_resolver(ResolverOptions::new(
        host(fs, "^/~ts-uri~/custom/ts-nul-authority/"),
        options,
    ));
    let (resolved, _) = resolver.resolve_module_name(specifier, source_file, mode, None).unwrap();
    resolved.resolved_file_name.to_string()
}

#[test]
fn test_resolve_dynamic_module_name_using_root_dirs() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/main.ts";
    for (target_root, target_file) in [
        (
            "^/~ts-uri~/custom/ts-nul-authority/generated",
            "^/~ts-uri~/custom/ts-nul-authority/generated/~ts-uri-escape~7e74732d7572692d6573636170657e66696c65~.ts",
        ),
        ("c:/generated", "c:/generated/~ts-uri-escape~file.ts"),
    ] {
        let resolved = resolve_dynamic(
            [(SOURCE, ""), (target_file, "export const value = 1;")],
            dynamic_options(
                ModuleKind::ES_NEXT,
                Some(vec![
                    "^/~ts-uri~/custom/ts-nul-authority/src".to_string(),
                    target_root.to_string(),
                ]),
            ),
            SOURCE,
            "./~ts-uri-escape~file",
            ModuleKind::ES_NEXT,
        );
        assert_eq!(resolved, target_file);
    }
}

#[test]
fn test_root_dirs_preserves_exceptional_dynamic_segments() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/~ts-uri-escape~2e2e~/main.ts";
    const TARGET: &str = "^/~ts-uri~/custom/ts-nul-authority/generated/~ts-uri-escape~2e2e~/dep.ts";
    let resolved = resolve_dynamic(
        [
            (SOURCE, ""),
            (TARGET, "export const value = 1;"),
            ("^/~ts-uri~/custom/ts-nul-authority/dep.ts", "export const wrong = 1;"),
        ],
        dynamic_options(
            ModuleKind::ES_NEXT,
            Some(vec![
                "^/~ts-uri~/custom/ts-nul-authority/src".to_string(),
                "^/~ts-uri~/custom/ts-nul-authority/generated".to_string(),
            ]),
        ),
        SOURCE,
        "./dep",
        ModuleKind::ES_NEXT,
    );
    assert_eq!(resolved, TARGET);
}

#[test]
fn test_resolve_dynamic_package_subpath_file() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/main.ts";
    const TARGET: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/pkg/~ts-uri-escape~7e74732d7572692d6573636170657e3636366636667e~.ts";
    let resolved = resolve_dynamic(
        [
            (SOURCE, ""),
            (
                "^/~ts-uri~/custom/ts-nul-authority/node_modules/pkg/package.json",
                r#"{"name":"pkg"}"#,
            ),
            (TARGET, "export const value = 1;"),
        ],
        dynamic_options(ModuleKind::ES_NEXT, None),
        SOURCE,
        "pkg/~ts-uri-escape~666f6f~.ts",
        ModuleKind::ES_NEXT,
    );
    assert_eq!(resolved, TARGET);
}

#[test]
fn test_resolve_dynamic_dotted_directory() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/main.ts";
    const TARGET: &str = "^/~ts-uri~/custom/ts-nul-authority/src/~ts-uri-escape~7e74732d7572692d6573636170657e6469722e6a73~/index.ts";
    let resolved = resolve_dynamic(
        [(SOURCE, ""), (TARGET, "export const value = 1;")],
        dynamic_options(ModuleKind::COMMON_JS, None),
        SOURCE,
        "./~ts-uri-escape~dir.js",
        ModuleKind::COMMON_JS,
    );
    assert_eq!(resolved, TARGET);
}

// A type root under a dynamic (non-`file:`) URI, such as a document in a virtual workspace. The file encoding and
// the directory encoding of `~ts-uri-escape~thing.d.ts` differ: the file encoding keeps the `.d.ts` extension
// outside the escape. Go `getCandidateFromTypeRoot` (resolver.go:659) keeps both encodings, and the directory
// lookup uses the directory encoding (resolver.go:1823). The expected traces come from tsgo at the pinned commit.
const DYNAMIC_TYPE_ROOT: &str = "^/~ts-uri~/custom/ts-nul-authority/types";
const DYNAMIC_TYPE_ROOT_FILE: &str = "^/~ts-uri~/custom/ts-nul-authority/types/~ts-uri-escape~7e74732d7572692d6573636170657e7468696e67~.d.ts";
const DYNAMIC_TYPE_ROOT_INDEX: &str = "^/~ts-uri~/custom/ts-nul-authority/types/~ts-uri-escape~7e74732d7572692d6573636170657e7468696e672e642e7473~/index.d.ts";

// Resolves `~ts-uri-escape~thing.d.ts` from `$CWD/a.ts` with the type root `$ROOT`, where only `target` exists. The
// result is the resolved file name and the trace. In the trace, `$NAME`, `$CWD`, `$FILE`, `$DIRECTORY` and `$ROOT`
// replace the name and the paths.
fn resolve_in_dynamic_type_root(target: &str, module: bool) -> (String, Vec<String>) {
    const CWD: &str = "^/~ts-uri~/custom/ts-nul-authority/";
    const NAME: &str = "~ts-uri-escape~thing.d.ts";
    let fs = vfstest::from_map(
        [
            (format!("{DYNAMIC_TYPE_ROOT}/keep"), ""),
            (target.to_string(), "declare const value: number;"),
        ],
        true,
    );
    let resolver = new_resolver(ResolverOptions::new(
        host(fs, CWD),
        P::new(CompilerOptions {
            module_resolution: ModuleResolutionKind::BUNDLER,
            type_roots: Some(vec![DYNAMIC_TYPE_ROOT.to_string()]),
            trace_resolution: Tristate::True,
            ..Default::default()
        }),
    ));
    let source = format!("{CWD}a.ts");
    let (resolved, trace) = if module {
        let (resolved, trace) =
            resolver.resolve_module_name(NAME, &source, ModuleKind::COMMON_JS, None).unwrap();
        (resolved.resolved_file_name.to_string(), trace)
    } else {
        let (resolved, trace) =
            resolver.resolve_type_reference_directive(NAME, &source, ModuleKind::COMMON_JS, None);
        (resolved.resolved_file_name.to_string(), trace)
    };
    let directory = DYNAMIC_TYPE_ROOT_INDEX.strip_suffix("/index.d.ts").unwrap();
    let trace = trace
        .iter()
        .map(|t| {
            format!("{}|{}", t.message.key(), t.args.join("|"))
                .replace(DYNAMIC_TYPE_ROOT_FILE, "$FILE")
                .replace(directory, "$DIRECTORY")
                .replace(DYNAMIC_TYPE_ROOT, "$ROOT")
                .replace(CWD, "$CWD/")
                .replace(NAME, "$NAME")
        })
        .collect();
    (resolved, trace)
}

fn trace_lines(expected: &str) -> Vec<String> {
    expected.lines().map(str::trim).filter(|line| !line.is_empty()).map(String::from).collect()
}

#[test]
fn test_dynamic_type_root_directory_uses_directory_encoding() {
    let (resolved, trace) = resolve_in_dynamic_type_root(DYNAMIC_TYPE_ROOT_INDEX, false);
    assert_eq!(resolved, DYNAMIC_TYPE_ROOT_INDEX);
    assert_eq!(
        trace,
        trace_lines(
            "
            Resolving_type_reference_directive_0_containing_file_1_root_directory_2_6116|$NAME|$CWD/a.ts|$ROOT
            Resolving_with_primary_search_path_0_6121|$ROOT
            File_name_0_has_a_1_extension_stripping_it_6132|$FILE|.d.ts
            File_0_does_not_exist_6096|$FILE
            File_0_does_not_exist_6096|$FILE.d.ts
            File_0_does_not_exist_6096|$DIRECTORY/package.json
            File_0_exists_use_it_as_a_name_resolution_result_6097|$DIRECTORY/index.d.ts
            Resolving_real_path_for_0_result_1_6130|$DIRECTORY/index.d.ts|$DIRECTORY/index.d.ts
            Type_reference_directive_0_was_successfully_resolved_to_1_primary_Colon_2_6119|$NAME|$DIRECTORY/index.d.ts|true
            "
        )
    );
}

#[test]
fn test_dynamic_type_root_file_uses_file_encoding() {
    let (resolved, trace) = resolve_in_dynamic_type_root(DYNAMIC_TYPE_ROOT_FILE, false);
    assert_eq!(resolved, DYNAMIC_TYPE_ROOT_FILE);
    assert_eq!(
        trace,
        trace_lines(
            "
            Resolving_type_reference_directive_0_containing_file_1_root_directory_2_6116|$NAME|$CWD/a.ts|$ROOT
            Resolving_with_primary_search_path_0_6121|$ROOT
            File_name_0_has_a_1_extension_stripping_it_6132|$FILE|.d.ts
            File_0_exists_use_it_as_a_name_resolution_result_6097|$FILE
            Resolving_real_path_for_0_result_1_6130|$FILE|$FILE
            Type_reference_directive_0_was_successfully_resolved_to_1_primary_Colon_2_6119|$NAME|$FILE|true
            "
        )
    );
}

// Go `resolveFromTypeRoot` (resolver.go:678): the module resolution fallback builds the same candidate.
#[test]
fn test_dynamic_type_root_module_fallback_uses_directory_encoding() {
    let (resolved, trace) = resolve_in_dynamic_type_root(DYNAMIC_TYPE_ROOT_INDEX, true);
    assert_eq!(resolved, DYNAMIC_TYPE_ROOT_INDEX);
    assert_eq!(
        trace,
        trace_lines(
            "
            Resolving_module_0_from_1_6086|$NAME|$CWD/a.ts
            Explicitly_specified_module_resolution_kind_Colon_0_6087|Bundler
            Resolving_in_0_mode_with_conditions_1_6402|CJS|'require', 'types'
            File_0_does_not_exist_6096|$CWD/package.json
            Loading_module_0_from_node_modules_folder_target_file_types_Colon_1_6098|$NAME|TypeScript, JavaScript, Declaration, JSON
            Searching_all_ancestor_node_modules_directories_for_preferred_extensions_Colon_0_6417|TypeScript, Declaration
            Directory_0_does_not_exist_skipping_all_lookups_in_it_6148|$CWD/node_modules
            Searching_all_ancestor_node_modules_directories_for_fallback_extensions_Colon_0_6418|JavaScript, JSON
            Directory_0_does_not_exist_skipping_all_lookups_in_it_6148|$CWD/node_modules
            File_name_0_has_a_1_extension_stripping_it_6132|$FILE|.d.ts
            File_0_does_not_exist_6096|$FILE
            File_0_does_not_exist_6096|$FILE.d.ts
            File_0_does_not_exist_6096|$DIRECTORY/package.json
            File_0_exists_use_it_as_a_name_resolution_result_6097|$DIRECTORY/index.d.ts
            Module_name_0_was_successfully_resolved_to_1_6089|$NAME|$DIRECTORY/index.d.ts
            "
        )
    );
}

#[test]
fn test_root_dirs_rejects_unrepresentable_disk_segments() {
    for source in [
        "^/~ts-uri~/custom/ts-nul-authority/src/~ts-uri-escape~2e2e~/main.ts",
        "^/~ts-uri~/custom/ts-nul-authority/src/c:/main.ts",
        "^/~ts-uri~/custom/ts-nul-authority/src/^/main.ts",
    ] {
        let resolved = resolve_dynamic(
            [(source, ""), ("c:/dep.ts", "export const wrong = 1;")],
            dynamic_options(
                ModuleKind::ES_NEXT,
                Some(vec![
                    "^/~ts-uri~/custom/ts-nul-authority/src".to_string(),
                    "c:/generated".to_string(),
                ]),
            ),
            source,
            "./dep",
            ModuleKind::ES_NEXT,
        );
        assert!(resolved.is_empty(), "{source} resolved to {resolved}");
    }
}

#[test]
fn test_resolve_dynamic_package_json_path() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/main.ts";
    const FALLBACK: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/pkg/~ts-uri-escape~7e74732d7572692d6573636170657e7479706573~.d.ts";
    const TARGET: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/pkg/ts3.1/~ts-uri-escape~7e74732d7572692d6573636170657e7479706573~.d.ts";
    let resolved = resolve_dynamic(
        [
            (SOURCE, ""),
            (
                "^/~ts-uri~/custom/ts-nul-authority/node_modules/pkg/package.json",
                r#"{"name":"pkg","types":"~ts-uri-escape~types.d.ts","typesVersions":{"*":{"*":["ts3.1/*"]}}}"#,
            ),
            (FALLBACK, "export const fallback: number;"),
            (TARGET, "export const value: number;"),
        ],
        dynamic_options(ModuleKind::ES_NEXT, None),
        SOURCE,
        "pkg",
        ModuleKind::ES_NEXT,
    );
    assert_eq!(resolved, TARGET);
}

#[test]
fn test_resolve_dynamic_esm_package_index_from_reserved_directory() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/main.ts";
    const PACKAGE_DIR: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/~ts-uri-escape~7e74732d7572692d6573636170657e706b672e6a73~";
    const TARGET: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/~ts-uri-escape~7e74732d7572692d6573636170657e706b672e6a73~/index.js";
    let resolved = resolve_dynamic(
        [
            (SOURCE, ""),
            (
                "^/~ts-uri~/custom/ts-nul-authority/node_modules/~ts-uri-escape~7e74732d7572692d6573636170657e706b672e6a73~/package.json",
                r#"{"name":"~ts-uri-escape~pkg.js"}"#,
            ),
            (TARGET, "exports.value = 1;"),
        ],
        dynamic_options(ModuleKind::ES_NEXT, None),
        SOURCE,
        "~ts-uri-escape~pkg.js",
        ModuleKind::ES_NEXT,
    );
    assert_eq!(resolved, TARGET, "{PACKAGE_DIR}");
}

#[test]
fn test_generated_dynamic_entrypoint_specifier_resolves_encoded_file() {
    const SOURCE: &str = "^/~ts-uri~/custom/ts-nul-authority/src/main.ts";
    const PACKAGE: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/Pkg/package.json";
    const FILE: &str = "^/~ts-uri~/custom/ts-nul-authority/node_modules/Pkg/~ts-uri-escape~7e74732d7572692d6573636170657e76616c7565~.d.ts";
    const SPECIFIER: &str = "Pkg/~ts-uri-spec~7e74732d7572692d6573636170657e76616c7565~.d.ts";
    let fs = vfstest::from_map(
        [(SOURCE, ""), (PACKAGE, r#"{"name":"Pkg"}"#), (FILE, "export const value: number;")],
        true,
    );
    let resolver = new_resolver(ResolverOptions::new(
        host(fs, "^/~ts-uri~/custom/ts-nul-authority/"),
        dynamic_options(ModuleKind::ES_NEXT, None),
    ));
    resolver.resolve_module_name("Pkg", SOURCE, ModuleKind::ES_NEXT, None).unwrap();
    let mut package_json = None;
    resolver.get_resolution_data().package_json_cache_entries(|_, entry| {
        if entry.exists() {
            package_json = Some(*entry);
            false
        } else {
            true
        }
    });
    let package_json = package_json.expect("package JSON cache entry");
    let entrypoints =
        resolver.get_entrypoints_from_package_json_info(package_json, "Pkg", true).unwrap();
    assert_eq!(entrypoints.len(), 1);
    assert_eq!(entrypoints[0].module_specifier, SPECIFIER);
    let (resolved, _) =
        resolver.resolve_module_name(SPECIFIER, SOURCE, ModuleKind::ES_NEXT, None).unwrap();
    assert_eq!(resolved.resolved_file_name, FILE);
}

// Regression test for https://github.com/microsoft/TypeScript/tsc/issues/3526.
//
// Resolving a node_modules import with a trailing slash (e.g. `pkg/`) must
// produce the same result as without one.
#[test]
fn test_resolve_module_name_trailing_slash() {
    let fs = vfstest::from_map(
        [
            (
                "/repo/node_modules/pkg/package.json",
                r#"{"name":"pkg","main":"main.js","types":"main.d.ts"}"#,
            ),
            ("/repo/node_modules/pkg/main.d.ts", "export const x: number;"),
            ("/repo/node_modules/pkg/main.js", "exports.x = 1;"),
            ("/repo/src/file.ts", ""),
        ],
        true,
    );
    let resolver = new_resolver(ResolverOptions::new(host(fs, "/repo"), bundler_options()));

    for name in ["pkg", "pkg/"] {
        let (r, _) = resolver
            .resolve_module_name(name, "/repo/src/file.ts", ModuleKind::ES_NEXT, None)
            .unwrap();
        assert!(r.is_resolved(), "{name:?} failed to resolve");
    }
}

#[test]
fn test_resolution_data_caches() {
    let old_host = host(
        vfstest::from_map(
            [
                ("/src/node_modules/pkg/package.json", r#"{"name":"pkg","types":"index.d.ts"}"#),
                ("/src/node_modules/pkg/index.d.ts", "export const value: number;"),
            ],
            true,
        ),
        "/",
    );
    let new_host = host(
        vfstest::from_map(
            [
                ("/src/node_modules/pkg/package.json", r#"{"name":"pkg","types":"index.d.ts"}"#),
                ("/src/node_modules/pkg/index.d.ts", "export const value: number;"),
                ("/new/package.json", r#"{"name":"new"}"#),
                ("/missing-first/package.json", r#"{"name":"missing-first"}"#),
            ],
            true,
        ),
        "/",
    );
    let resolver = new_resolver(ResolverOptions::new(
        old_host,
        P::new(CompilerOptions { module: ModuleKind::NODE_NEXT, ..Default::default() }),
    ));
    let (resolved, _) =
        resolver.resolve_module_name("pkg", "/src/index.ts", ModuleKind::COMMON_JS, None).unwrap();
    assert!(resolved.is_resolved());
    let (cached, _) =
        resolver.resolve_module_name("pkg", "/src/index.ts", ModuleKind::COMMON_JS, None).unwrap();
    assert_eq!(cached, resolved);

    resolver.resolve_type_reference_directive(
        "missing",
        "/src/index.ts",
        ModuleKind::COMMON_JS,
        None,
    );
    let mut paths = new_ordered_map_with_size_hint(1);
    paths.insert("alias/*".to_string(), Some(vec!["./*".to_string()]));
    resolver.get_parsed_patterns_for_paths(&CompilerOptions {
        paths: Some(std::sync::Arc::new(paths)),
        ..Default::default()
    });
    assert_eq!(resolver.module_resolution_cache.size(), 1);
    assert_eq!(resolver.type_ref_directive_resolution_cache.size(), 1);
    assert_eq!(resolver.parsed_patterns_for_paths.size(), 1);

    let data = resolver.get_resolution_data();
    let rebound = data.get().new_resolver(old_host);
    assert_eq!(rebound.module_resolution_cache.size(), 0);
    assert_eq!(rebound.type_ref_directive_resolution_cache.size(), 0);
    assert_eq!(rebound.parsed_patterns_for_paths.size(), 0);

    let clone = data.clone_data().get().new_resolver(new_host);
    assert_eq!(clone.module_resolution_cache.size(), 0);
    assert_eq!(clone.type_ref_directive_resolution_cache.size(), 0);
    assert_eq!(clone.parsed_patterns_for_paths.size(), 0);
    assert_eq!(
        clone.get_package_scope_for_path("/src/node_modules/pkg"),
        resolver.get_package_scope_for_path("/src/node_modules/pkg")
    );
    assert!(clone.get_package_scope_for_path("/new").exists());
    assert!(!resolver.get_package_scope_for_path("/new").exists());
    assert!(!resolver.get_package_scope_for_path("/missing-first").exists());
    assert!(clone.get_package_scope_for_path("/missing-first").exists());
}

// A one-shot event, standing in for Go's `chan struct{}` that is closed once.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.cv.notify_all();
    }

    fn wait(&self, description: &str) {
        let guard = self.open.lock().unwrap();
        let (guard, timeout) =
            self.cv.wait_timeout_while(guard, Duration::from_secs(10), |open| !*open).unwrap();
        assert!(!timeout.timed_out() || *guard, "timed out waiting for {description}");
    }
}

// Delegates every method but `file_exists`/`read_file` to `fs`.
macro_rules! delegate_fs {
    () => {
        fn use_case_sensitive_file_names(&self) -> bool {
            self.fs.use_case_sensitive_file_names()
        }
        fn write_file(&self, path: &str, data: &str) -> Result<(), String> {
            self.fs.write_file(path, data)
        }
        fn append_file(&self, path: &str, data: &str) -> Result<(), String> {
            self.fs.append_file(path, data)
        }
        fn remove(&self, path: &str) -> Result<(), String> {
            self.fs.remove(path)
        }
        fn chtimes(
            &self,
            path: &str,
            a_time: SystemTime,
            m_time: SystemTime,
        ) -> Result<(), String> {
            self.fs.chtimes(path, a_time, m_time)
        }
        fn directory_exists(&self, path: &str) -> bool {
            self.fs.directory_exists(path)
        }
        fn get_accessible_entries(&self, path: &str) -> Entries {
            self.fs.get_accessible_entries(path)
        }
        fn stat(&self, path: &str) -> Option<FileInfo> {
            self.fs.stat(path)
        }
        fn realpath(&self, path: &str) -> String {
            self.fs.realpath(path)
        }
    };
}

// blockingFS wraps a vfs.FS and forces FileExists calls for `targetPath` to
// block on `gate` until released. Each caller sends on `arrived` when it
// reaches the gate.
struct BlockingFS<T: FS> {
    fs: T,
    target_path: String,
    gate: Gate,
    arrived: Mutex<mpsc::Sender<()>>,
}

impl<T: FS> FS for BlockingFS<T> {
    delegate_fs!();
    fn file_exists(&self, path: &str) -> bool {
        if path == self.target_path {
            self.arrived.lock().unwrap().send(()).unwrap();
            self.gate.wait("gate");
        }
        self.fs.file_exists(path)
    }
    fn read_file(&self, path: &str) -> Option<String> {
        self.fs.read_file(path)
    }
}

// flipFileExistsFS returns false for the first FileExists call to `targetPath`,
// then true for the second. ReadFile for the target path signals arrival then
// blocks, so the "file doesn't exist" Set completes before the "file exists" Set.
struct FlipFileExistsFS<T: FS> {
    fs: T,
    target_path: String,
    call_count: AtomicUsize,
    first_arrived: Gate,
    second_arrived: Gate,
    first_gate: Gate,
    second_gate: Gate,
    read_arrived: Gate,
    read_gate: Gate,
}

impl<T: FS> FlipFileExistsFS<T> {
    fn new(fs: T, target_path: &str) -> FlipFileExistsFS<T> {
        FlipFileExistsFS {
            fs,
            target_path: target_path.to_string(),
            call_count: AtomicUsize::new(0),
            first_arrived: Gate::default(),
            second_arrived: Gate::default(),
            first_gate: Gate::default(),
            second_gate: Gate::default(),
            read_arrived: Gate::default(),
            read_gate: Gate::default(),
        }
    }
}

impl<T: FS> FS for FlipFileExistsFS<T> {
    delegate_fs!();
    fn file_exists(&self, path: &str) -> bool {
        if path == self.target_path {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
            if n == 1 {
                self.first_arrived.open();
                self.first_gate.wait("first gate");
                return false; // first caller: simulate "file not yet visible"
            }
            if n == 2 {
                self.second_arrived.open();
                self.second_gate.wait("second gate");
                return self.fs.file_exists(path); // second caller: file is visible
            }
        }
        self.fs.file_exists(path)
    }
    fn read_file(&self, path: &str) -> Option<String> {
        if path == self.target_path {
            self.read_arrived.open();
            self.read_gate.wait("read gate");
        }
        self.fs.read_file(path)
    }
}

// Regression test for https://github.com/microsoft/TypeScript/tsc/issues/3526.
//
// Two threads resolve the same package via specifiers that differ only by
// a trailing slash (`pkg` and `pkg/`), both held at the `FileExists` check for
// `package.json` after a cache miss; the loser of the insert race must still
// see its own `PackageDirectory`.
#[test]
fn test_resolve_module_name_trailing_slash_race() {
    const PKG_JSON_PATH: &str = "/repo/node_modules/pkg/package.json";
    let (arrived_tx, arrived_rx) = mpsc::channel();
    let fs: &'static BlockingFS<_> = Box::leak(Box::new(BlockingFS {
        fs: vfstest::from_map(
            [
                (PKG_JSON_PATH, r#"{"name":"pkg","types":"./typings/index.d.ts"}"#),
                ("/repo/node_modules/pkg/typings/index.d.ts", "export const x: number;"),
                ("/repo/src/a/file.ts", ""),
                ("/repo/src/b/file.ts", ""),
            ],
            true,
        ),
        target_path: PKG_JSON_PATH.to_string(),
        gate: Gate::default(),
        arrived: Mutex::new(arrived_tx),
    }));
    let resolver = new_resolver(ResolverOptions::new(host(fs, "/repo"), bundler_options()));

    std::thread::scope(|s| {
        let handles: Vec<_> = ["pkg", "pkg/"]
            .into_iter()
            .map(|name| {
                let containing_file =
                    if name.ends_with('/') { "/repo/src/b/file.ts" } else { "/repo/src/a/file.ts" };
                let resolver = &resolver;
                s.spawn(move || {
                    (
                        name,
                        resolver
                            .resolve_module_name(name, containing_file, ModuleKind::ES_NEXT, None)
                            .unwrap()
                            .0
                            .is_resolved(),
                    )
                })
            })
            .collect();

        // Wait for both threads to reach the FileExists gate, guaranteeing
        // both have observed a package.json info-cache miss.
        arrived_rx.recv_timeout(Duration::from_secs(10)).expect("first FileExists gate arrival");
        arrived_rx.recv_timeout(Duration::from_secs(10)).expect("second FileExists gate arrival");
        fs.gate.open();

        for handle in handles {
            let (name, resolved) = handle.join().unwrap();
            assert!(resolved, "{name:?} failed to resolve");
        }
    });
}

// Regression test for https://github.com/microsoft/TypeScript/tsc/issues/1290.
//
// The second lookup of the root package.json receives the first lookup's
// nil-Contents cache entry; the typesVersions lookup must not dereference it.
#[test]
fn test_resolve_subpath_nil_contents_race() {
    const ROOT_PKG_JSON: &str = "/repo/node_modules/pkg/package.json";
    let fs: &'static FlipFileExistsFS<_> = Box::leak(Box::new(FlipFileExistsFS::new(
        vfstest::from_map(
            [
                (ROOT_PKG_JSON, r#"{"name":"pkg","version":"1.0.0"}"#),
                ("/repo/node_modules/pkg/sub/index.d.ts", "export declare const sub: number;"),
                ("/repo/node_modules/pkg/sub/index.js", "exports.sub = 1;"),
                ("/repo/src/a/file.ts", ""),
                ("/repo/src/b/file.ts", ""),
            ],
            true,
        ),
        ROOT_PKG_JSON,
    )));
    let resolver = new_resolver(ResolverOptions::new(host(fs, "/repo"), bundler_options()));

    std::thread::scope(|s| {
        let handles: Vec<_> = ["/repo/src/a/file.ts", "/repo/src/b/file.ts"]
            .into_iter()
            .map(|containing_file| {
                let resolver = &resolver;
                s.spawn(move || {
                    (
                        containing_file,
                        resolver
                            .resolve_module_name(
                                "pkg/sub",
                                containing_file,
                                ModuleKind::ES_NEXT,
                                None,
                            )
                            .unwrap()
                            .0
                            .is_resolved(),
                    )
                })
            })
            .collect();

        fs.first_arrived.wait("first root package.json FileExists arrival");
        fs.second_arrived.wait("second root package.json FileExists arrival");
        fs.first_gate.open();
        fs.second_gate.open();
        fs.read_arrived.wait("root package.json ReadFile arrival");
        fs.read_gate.open();

        for handle in handles {
            let (containing_file, resolved) =
                handle.join().expect("resolver panicked due to nil Contents dereference");
            assert!(resolved, "{containing_file:?} failed to resolve pkg/sub");
        }
    });
}

#[test]
fn test_parse_node_module_from_path() {
    let tests = [
        ("file in package", "/a/node_modules/b/lib/index.d.ts", false, "/a/node_modules/b"),
        (
            "file in scoped package",
            "/a/node_modules/@scope/b/lib/index.d.ts",
            false,
            "/a/node_modules/@scope/b",
        ),
        ("folder subpath", "/a/node_modules/b/lib/File", true, "/a/node_modules/b"),
        (
            "folder subpath scoped",
            "/a/node_modules/@scope/b/lib/File",
            true,
            "/a/node_modules/@scope/b",
        ),
        ("package root folder", "/a/node_modules/b", true, "/a/node_modules/b"),
        (
            "scoped package root folder",
            "/a/node_modules/@scope/b",
            true,
            "/a/node_modules/@scope/b",
        ),
        // A bare scope directory has no package name; must not panic (https://github.com/microsoft/TypeScript/tsc/issues/4373).
        ("scope-only folder", "/a/node_modules/@scope", true, "/a/node_modules/@scope"),
        ("types scope-only folder", "/a/node_modules/@types", true, "/a/node_modules/@types"),
        ("not in node_modules", "/a/src/index.ts", false, ""),
    ];
    for (name, path, is_folder, want) in tests {
        assert_eq!(parse_node_module_from_path(path, is_folder), want, "{name}");
    }
}

// Regression test for https://github.com/microsoft/TypeScript/tsc/issues/4478.
//
// The peer package.json lookup must not dereference a nil-Contents cache entry.
#[test]
fn test_resolve_peer_dependency_nil_contents_race() {
    const PEER_PKG_JSON: &str = "/repo/node_modules/peer/package.json";
    let fs: &'static FlipFileExistsFS<_> = Box::leak(Box::new(FlipFileExistsFS::new(
        vfstest::from_map(
            [
                (
                    "/repo/node_modules/pkg/package.json",
                    r#"{"name":"pkg","version":"1.0.0","types":"index.d.ts","peerDependencies":{"peer":"*"}}"#,
                ),
                ("/repo/node_modules/pkg/index.d.ts", "export declare const x: number;"),
                (PEER_PKG_JSON, r#"{"name":"peer","version":"2.0.0"}"#),
                ("/repo/src/a/file.ts", ""),
                ("/repo/src/b/file.ts", ""),
            ],
            true,
        ),
        PEER_PKG_JSON,
    )));
    let resolver = new_resolver(ResolverOptions::new(host(fs, "/repo"), bundler_options()));

    std::thread::scope(|s| {
        let (results_tx, results_rx) = mpsc::channel();
        for containing_file in ["/repo/src/a/file.ts", "/repo/src/b/file.ts"] {
            let resolver = &resolver;
            let results_tx = results_tx.clone();
            s.spawn(move || {
                let resolved = resolver
                    .resolve_module_name("pkg", containing_file, ModuleKind::ES_NEXT, None)
                    .unwrap()
                    .0
                    .is_resolved();
                results_tx.send((containing_file, resolved)).unwrap();
            });
        }

        fs.first_arrived.wait("first peer package.json FileExists arrival");
        fs.second_arrived.wait("second peer package.json FileExists arrival");
        fs.first_gate.open();
        let first_result = results_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("timed out waiting for first peer package.json lookup to finish");
        fs.second_gate.open();
        fs.read_arrived.wait("peer package.json ReadFile arrival");
        fs.read_gate.open();

        assert!(first_result.1, "{:?} failed to resolve pkg", first_result.0);
        let second_result = results_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("resolver panicked due to nil Contents dereference");
        assert!(second_result.1, "{:?} failed to resolve pkg", second_result.0);
    });
}
