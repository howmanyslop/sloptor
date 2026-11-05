// contentmapper_test.go: program construction with content-mapped files, through a fake content mapper project.

use std::sync::Arc;
use std::sync::Mutex;

use tsrs_ast::{Diagnostic, MappedDiagnosticDirective, MappedDiagnosticDirectivePolicy};
use tsrs_contentmapper::{
    self as contentmapper, Definition, Manifest, MappedResult, Mapper, OptionDiagnostic,
    ProjectError, ProjectErrorKind, Request, TransformErrorKind, TransformResultFiles,
};
use tsrs_core::{
    CompilerOptions, ModuleKind, ModuleResolutionKind, P, ResolutionMode, TextRange, Tristate,
};
use tsrs_diagnostics as diagnostics;
use tsrs_spanmap::{Feature, Kind, Segment};
use tsrs_tsoptions::{ParsedCommandLine, new_parsed_command_line};
use tsrs_vfs::{FS, bundled, vfstest};

use tsrs_compiler::{Context, Program, ProgramOptions, new_compiler_host, new_program};

type TransformFn =
    dyn Fn(&str, &str) -> Result<TransformResultFiles, contentmapper::Error> + Send + Sync;

// contentmapper_test.go:22
struct FakeContentMapperHost {
    transform: Arc<TransformFn>,
}

impl contentmapper::Project for FakeContentMapperHost {
    fn refresh(&self) -> Result<(), contentmapper::Error> {
        Ok(())
    }
    fn identities(&self) -> Result<Vec<String>, contentmapper::Error> {
        Ok(Vec::new())
    }
    fn identity(&self, _mapper: &Mapper) -> Result<String, contentmapper::Error> {
        Ok("test".to_string())
    }
    fn watched_files(&self) -> Result<Vec<String>, contentmapper::Error> {
        Ok(Vec::new())
    }
    fn diagnostics(&self) -> Vec<OptionDiagnostic> {
        Vec::new()
    }
    fn close(&self) -> Result<(), contentmapper::Error> {
        Ok(())
    }
    fn transform(
        &self,
        _mapper: &Mapper,
        request: Request<'_>,
    ) -> Result<TransformResultFiles, contentmapper::Error> {
        (self.transform)(request.file_name, request.content)
    }
}

fn fake_host(
    transform: impl Fn(&str, &str) -> Result<TransformResultFiles, contentmapper::Error>
    + Send
    + Sync
    + 'static,
) -> Arc<dyn contentmapper::Project> {
    Arc::new(FakeContentMapperHost { transform: Arc::new(transform) })
}

fn options(module: ModuleKind, module_resolution: ModuleResolutionKind) -> CompilerOptions {
    CompilerOptions {
        skip_lib_check: Tristate::True,
        module,
        module_resolution,
        ..Default::default()
    }
}

// contentmapper_test.go:37
fn new_content_mapper_program(
    content_mapper_project: Arc<dyn contentmapper::Project>,
    files: &[(&str, &str)],
    root_files: &[&str],
) -> Option<&'static Program> {
    new_content_mapper_program_with_options(
        content_mapper_project,
        files,
        root_files,
        options(ModuleKind::ES_NEXT, ModuleResolutionKind::BUNDLER),
    )
}

// contentmapper_test.go:45 (None is Go's t.Skip when the bundled files are not embedded.)
fn new_content_mapper_program_with_options(
    content_mapper_project: Arc<dyn contentmapper::Project>,
    files: &[(&str, &str)],
    root_files: &[&str],
    options: CompilerOptions,
) -> Option<&'static Program> {
    new_content_mapper_program_with_output_extensions(
        content_mapper_project,
        files,
        root_files,
        options,
        &[],
    )
}

fn new_content_mapper_program_with_output_extensions(
    content_mapper_project: Arc<dyn contentmapper::Project>,
    files: &[(&str, &str)],
    root_files: &[&str],
    options: CompilerOptions,
    output_extensions: &[(&str, &str)],
) -> Option<&'static Program> {
    if !bundled::EMBEDDED {
        return None;
    }
    let fs: Arc<dyn FS> = Arc::new(bundled::wrap_fs(vfstest::from_map(
        files.iter().map(|&(k, v)| (k, v)),
        false, /*useCaseSensitiveFileNames*/
    )));

    let mut config: ParsedCommandLine = new_parsed_command_line(
        P::new(options),
        root_files.iter().map(std::string::ToString::to_string).collect(),
        Vec::new(),
        tsrs_core::tspath::ComparePathsOptions::default(),
    );
    config.parsed_config.content_mappers = vec![Mapper {
        definition: Definition {
            package: "vue".to_string(),
            extensions: vec![".vue".to_string()],
            output_extensions: output_extensions
                .iter()
                .map(|&(source, target)| (source.to_string(), target.to_string()))
                .collect(),
            output_extensions_present: !output_extensions.is_empty(),
            options: String::new(),
        },
        manifest: Manifest {
            name: "vue-mapper".to_string(),
            version: "1.0.0".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }]
    .into();
    let mut opts = ProgramOptions::new(
        P::new(config),
        new_compiler_host(
            "/src",
            fs,
            &bundled::lib_path(),
            None,
            None,
            Some(content_mapper_project),
        ),
    );
    // Load files on the calling goroutine for deterministic diagnostics ordering.
    opts.single_threaded = Tristate::True;
    Some(new_program(opts))
}

fn result(
    text: &str,
    virtual_extension: &str,
    mappings: P<tsrs_spanmap::SpanMap>,
) -> TransformResultFiles {
    TransformResultFiles {
        text: text.to_string(),
        virtual_extension: virtual_extension.to_string(),
        mappings: Some(mappings),
        ..Default::default()
    }
}

// contentmapper_test.go:73. The mapper's virtual extension decides the module format: a `.mts` output is ESM even
// though `.vue` itself implies none.
#[test]
fn test_content_mapper_virtual_extension_sets_implied_node_format() {
    let Some(program) = new_content_mapper_program_with_options(
        fake_host(|_, _| Ok(result("export {};", ".mts", tsrs_spanmap::new(&[])))),
        &[("/src/Component.vue", "<template />")],
        &["/src/Component.vue"],
        options(ModuleKind::NODE_NEXT, ModuleResolutionKind::NODE_NEXT),
    ) else {
        return;
    };

    let file = program.get_source_file("/src/Component.vue").expect("mapped file");
    assert_eq!(
        program.get_source_file_meta_data(file.path()).implied_node_format,
        ResolutionMode::ESM
    );
}

// contentmapper_test.go:95. A mapped diagnostic directive suppresses the file's diagnostics, but a missing global
// type that checking the file discovers is still reported once with the file's incremental diagnostics; an
// `expect` directive that suppressed nothing in the file reports the mapper's unused-directive diagnostic.
#[test]
fn test_content_mapper_directives_preserve_incremental_globals() {
    for policy in [MappedDiagnosticDirectivePolicy::Ignore, MappedDiagnosticDirectivePolicy::Expect]
    {
        const TEXT: &str = "export function values() { function* generator() { yield 1; } }";
        let Some(program) = new_content_mapper_program_with_options(
            fake_host(move |_, content| {
                // The virtual source is unchanged; the directive covers the entire file, including offset zero.
                let len =
                    u32::try_from(content.len()).expect("test content fits in a source position");
                Ok(TransformResultFiles {
                    text: content.to_string(),
                    virtual_extension: ".ts".to_string(),
                    mappings: Some(tsrs_spanmap::new(&[Segment {
                        original_end: len,
                        virtual_end: len,
                        kind: Kind::VERBATIM,
                        features: Feature::All,
                        ..Default::default()
                    }])),
                    diagnostic_directives: vec![MappedDiagnosticDirective {
                        virtual_range: TextRange::new(0, len),
                        original_range: TextRange::new(0, len),
                        policy,
                        source: "vue",
                        unused_code: 2578,
                        unused_message_text: "Unused mapped expect directive.",
                    }],
                    ..Default::default()
                })
            }),
            &[("/src/Component.vue", TEXT)],
            &["/src/Component.vue"],
            CompilerOptions {
                lib: Some(vec!["lib.es5.d.ts".to_string()]),
                ..options(ModuleKind::ES_NEXT, ModuleResolutionKind::BUNDLER)
            },
        ) else {
            return;
        };
        let ctx = Context::default();
        assert_eq!(program.get_global_diagnostics(&ctx).len(), 0);
        let file = program.get_source_file("/src/Component.vue").unwrap();
        let diags =
            program.get_semantic_diagnostics_for_incremental(&ctx, &[file]).remove(&file).unwrap();
        let (mut globals, mut unused) = (0, 0);
        for diag in diags {
            if diag.file().is_none() {
                assert_eq!(diag.code(), diagnostics::CANNOT_FIND_GLOBAL_TYPE_0.code());
                assert_eq!(diag.message_args()[0], "IterableIterator");
                globals += 1;
            } else {
                assert_eq!(diag.file(), Some(file));
                assert_eq!(diag.source(), "vue");
                assert_eq!(diag.code(), 2578);
                unused += 1;
            }
        }
        assert_eq!(globals, 1, "{policy:?}");
        assert_eq!(
            unused,
            i32::from(policy == MappedDiagnosticDirectivePolicy::Expect),
            "{policy:?}"
        );
    }
}

fn supplemental_host() -> Arc<dyn contentmapper::Project> {
    fake_host(|_, _| {
        Ok(TransformResultFiles {
            supplemental: vec![MappedResult {
                text: "export {};".to_string(),
                virtual_extension: ".mts".to_string(),
                mappings: Some(tsrs_spanmap::new(&[])),
                diagnostic_directives: Vec::new(),
            }],
            ..result("export {};", ".ts", tsrs_spanmap::new(&[]))
        })
    })
}

fn composite_options() -> CompilerOptions {
    CompilerOptions {
        composite: Tristate::True,
        ..options(ModuleKind::ES_NEXT, ModuleResolutionKind::BUNDLER)
    }
}

fn is_unlisted_file_diagnostic(diagnostic: P<Diagnostic>) -> bool {
    diagnostic.code()
        == diagnostics::FILE_0_IS_NOT_LISTED_WITHIN_THE_FILE_LIST_OF_PROJECT_1_PROJECTS_MUST_LIST_ALL_FILES_OR_USE_AN_INCLUDE_PATTERN.code()
}

// contentmapper_test.go:162 "listed canonical root". A composite project lists the canonical `.vue` file; its
// supplemental output is part of that root and must not be reported as an unlisted file.
#[test]
fn test_composite_project_content_mapper_supplemental_roots_listed_canonical_root() {
    let Some(program) = new_content_mapper_program_with_options(
        supplemental_host(),
        &[("/src/Component.vue", "<template />")],
        &["/src/Component.vue"],
        composite_options(),
    ) else {
        return;
    };

    let program_diagnostics = collect_content_mapper_diagnostics(program);
    let has_unlisted_file_diagnostic =
        program_diagnostics.iter().copied().any(is_unlisted_file_diagnostic);
    assert!(
        !has_unlisted_file_diagnostic,
        "supplemental output should be covered by its listed canonical root: {program_diagnostics:?}"
    );
}

// contentmapper_test.go:180 "imported canonical file". A canonical file reached only through an import is not
// listed, and neither is its supplemental output: both are reported.
#[test]
fn test_composite_project_content_mapper_supplemental_roots_imported_canonical_file() {
    let Some(program) = new_content_mapper_program_with_options(
        supplemental_host(),
        &[
            ("/src/index.ts", r#"import "./Component.vue";"#),
            ("/src/Component.vue", "<template />"),
        ],
        &["/src/index.ts"],
        composite_options(),
    ) else {
        return;
    };

    let unlisted_file_diagnostic_count = collect_content_mapper_diagnostics(program)
        .iter()
        .filter(|d| is_unlisted_file_diagnostic(**d))
        .count();
    assert_eq!(unlisted_file_diagnostic_count, 2);
}

// contentmapper_test.go:200
fn collect_content_mapper_diagnostics(program: &'static Program) -> Vec<P<Diagnostic>> {
    let ctx = Context::default();
    let mut all = program.get_syntactic_diagnostics(&ctx, None);
    all.extend(program.get_semantic_diagnostics(&ctx, None));
    all.extend(program.get_program_diagnostics());
    all
}

// contentmapper_test.go:209. Overlapping mapping segments are a mapper bug: the import of the mapped file gets
// the dedicated mapping diagnostic instead of a parse of the bad output.
#[test]
fn test_content_mapper_invalid_mappings() {
    const TRANSFORMED: &str = "export const x = 1;\n";
    const ORIGINAL: &str = "<template>x</template>\n";
    let Some(program) = new_content_mapper_program(
        fake_host(|_, _| {
            let mappings = tsrs_spanmap::new(&[
                Segment {
                    virtual_start: 0,
                    virtual_end: 10,
                    original_start: 0,
                    original_end: 0,
                    kind: Kind::ATOM,
                    ..Default::default()
                },
                Segment {
                    virtual_start: 5,
                    virtual_end: u32::try_from(TRANSFORMED.len())
                        .expect("test content fits in a source position"),
                    original_start: 0,
                    original_end: 0,
                    kind: Kind::ATOM,
                    ..Default::default()
                },
            ]);
            Ok(result(TRANSFORMED, ".ts", mappings))
        }),
        &[("/src/app.ts", r#"import "./Component.vue";"#), ("/src/Component.vue", ORIGINAL)],
        &["/src/app.ts"],
    ) else {
        return;
    };
    let program_diagnostics = collect_content_mapper_diagnostics(program);
    let found = program_diagnostics.iter().any(|diagnostic| {
        diagnostic.code() == diagnostics::THE_CONTENT_MAPPER_0_PRODUCED_OVERLAPPING_OR_OUT_OF_ORDER_POSITION_MAPPINGS_NEAR_VIRTUAL_OFFSET_1.code()
    });
    assert!(found, "expected an invalid mapping diagnostic, got: {program_diagnostics:?}");
}

// contentmapper_test.go:241 "successful synthesized empty file". An empty original whose transform succeeds is a
// mapped file, not a failure stub.
#[test]
fn test_content_mapper_source_file_state_successful_synthesized_empty_file() {
    let Some(program) = new_content_mapper_program(
        fake_host(|_, _| Ok(result("export {};", ".ts", tsrs_spanmap::new(&[])))),
        &[("/src/empty.vue", "")],
        &["/src/empty.vue"],
    ) else {
        return;
    };
    let file = program.get_source_file("/src/empty.vue").expect("mapped file");
    assert_eq!(file.original_text(), "");
    assert_eq!(file.content_mapper(), "vue-mapper@1.0.0");
    assert!(!file.is_content_mapper_failure_stub());
}

// contentmapper_test.go:255 "failed transform". The file stays in the program as an empty stub that keeps the
// original text and the mapper's identity.
#[test]
fn test_content_mapper_source_file_state_failed_transform() {
    let Some(program) = new_content_mapper_program(
        fake_host(|_, _| Err(contentmapper::Error::Other("failed".to_string()))),
        &[("/src/fail.vue", "original")],
        &["/src/fail.vue"],
    ) else {
        return;
    };
    let file = program.get_source_file("/src/fail.vue").expect("stub file");
    assert_eq!(file.original_text(), "original");
    assert_eq!(file.content_mapper(), "vue-mapper@1.0.0");
    assert!(file.is_content_mapper_failure_stub());
}

// contentmapper_test.go:269 "project error is localized". A project error behind a transform error is reported
// with its own message in the failure's chain, not as a generic transform failure.
#[test]
fn test_content_mapper_source_file_state_project_error_is_localized() {
    let Some(program) = new_content_mapper_program(
        fake_host(|_, _| {
            Err(contentmapper::new_transform_error(
                TransformErrorKind::Project,
                Some(ProjectError { kind: ProjectErrorKind::MalformedResponse }.into()),
            )
            .into())
        }),
        &[("/src/fail.vue", "original")],
        &["/src/fail.vue"],
    ) else {
        return;
    };
    let program_diagnostics = collect_content_mapper_diagnostics(program);
    let found = program_diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message_chain()
            .iter()
            .any(|message| message.code() == diagnostics::THE_CONTENT_MAPPER_RETURNED_A_PROJECT_RESPONSE_THAT_COULD_NOT_BE_DECODED.code())
    });
    assert!(
        found,
        "expected a localized project response diagnostic, got: {program_diagnostics:?}"
    );
}

// contentmapper_test.go:288. Each invalid project response has its own message (code and text).
#[test]
fn test_content_mapper_project_error_diagnostics() {
    for (kind, code, message_text) in [
        (
            ProjectErrorKind::MissingConfigIdentity,
            diagnostics::THE_CONTENT_MAPPER_DID_NOT_RETURN_CONFIG_IDENTITY_WHICH_IS_REQUIRED_WHEN_THE_CONTENT_MAPPER_HAS_DYNAMIC_CONFIG_COLON_TRUE_IN_ITS_PACKAGE_JSON.code(),
            r#"The content mapper did not return 'configIdentity', which is required when the content mapper has '"dynamicConfig": true' in its package.json."#,
        ),
        (
            ProjectErrorKind::UnexpectedConfigIdentity,
            diagnostics::THE_CONTENT_MAPPER_RETURNED_CONFIG_IDENTITY_WHICH_IS_ONLY_ALLOWED_WHEN_IT_DECLARES_DYNAMIC_CONFIG_COLON_TRUE_IN_ITS_PACKAGE_JSON.code(),
            r#"The content mapper returned 'configIdentity', which is only allowed when it declares '"dynamicConfig": true' in its package.json."#,
        ),
        (
            ProjectErrorKind::UnexpectedWatchedFiles,
            diagnostics::THE_CONTENT_MAPPER_RETURNED_WATCHED_FILES_WHICH_IS_ONLY_ALLOWED_WHEN_IT_DECLARES_DYNAMIC_CONFIG_COLON_TRUE_IN_ITS_PACKAGE_JSON.code(),
            r#"The content mapper returned 'watchedFiles', which is only allowed when it declares '"dynamicConfig": true' in its package.json."#,
        ),
    ] {
        let message = tsrs_compiler::content_mapper_project_error_diagnostic(&ProjectError { kind }.into());
        assert_eq!(message.code(), code);
        assert_eq!(message.localize(&[]), message_text);
    }
}

// The exact-source Go compiler's contentMapperOutputExtensions baseline rewrites static imports in both JavaScript
// and declarations, and passes custom extensions to the dynamic-import helper.
#[test]
fn content_mapper_output_extensions_rewrite_emit_and_declarations() {
    let Some(program) = new_content_mapper_program_with_output_extensions(
        fake_host(|_, _| {
            Ok(result(
                "export interface CardProps { title: string }\nexport const Card = (props: CardProps) => props.title;\n",
                ".ts",
                tsrs_spanmap::new(&[]),
            ))
        }),
        &[
            ("/src/Component.vue", "<template />"),
            (
                "/src/main.ts",
                "export { Card } from './Component.vue';\nexport type { CardProps } from './Component.vue';\nconst path = './Component.vue';\nvoid import(path);\nexport async function load() { return import('./Component.vue'); }\n",
            ),
        ],
        &["/src/main.ts", "/src/Component.vue"],
        CompilerOptions {
            target: tsrs_core::ScriptTarget::ES_NEXT,
            rewrite_relative_import_extensions: Tristate::True,
            declaration: Tristate::True,
            ..options(ModuleKind::PRESERVE, ModuleResolutionKind::BUNDLER)
        },
        &[(".vue", ".js")],
    ) else {
        return;
    };

    let written = Mutex::new(Vec::<(String, String)>::new());
    let write_file = |name: &str, body: &str, _: &mut tsrs_compiler::WriteFileData| {
        written.lock().unwrap().push((name.to_string(), body.to_string()));
        Ok(())
    };
    let emitted = program.emit(
        &Context::default(),
        &tsrs_compiler::EmitOptions { write_file: Some(&write_file), ..Default::default() },
    );
    assert!(!emitted.emit_skipped, "{:#?}", emitted.diagnostics);
    let written = written.lock().unwrap();
    let main_js = written.iter().find(|(name, _)| name.ends_with("main.js")).unwrap().1.as_str();
    let main_dts = written.iter().find(|(name, _)| name.ends_with("main.d.ts")).unwrap().1.as_str();
    assert!(main_js.contains("from './Component.js'"), "{main_js}");
    assert!(main_js.contains("import('./Component.js')"), "{main_js}");
    assert!(
        main_js
            .contains("__rewriteRelativeImportExtension(path, false, { \".vue\": \".js\" }, true)"),
        "{main_js}"
    );
    assert!(main_dts.contains("from './Component.js'"), "{main_dts}");
    assert!(main_dts.contains("import(\"./Component.js\")"), "{main_dts}");
}

// Go's output-path collision check includes external output files, even when the mapped source has no JS emit.
#[test]
fn content_mapper_output_extension_collision_is_reported() {
    let Some(program) = new_content_mapper_program_with_output_extensions(
        fake_host(|_, _| Ok(result("export const Card = 1;", ".ts", tsrs_spanmap::new(&[])))),
        &[("/src/Card.vue", "<template />"), ("/src/Card.ts", "export const Card = 2;")],
        &["/src/Card.vue", "/src/Card.ts"],
        options(ModuleKind::ES_NEXT, ModuleResolutionKind::BUNDLER),
        &[(".vue", ".js")],
    ) else {
        return;
    };

    let diagnostics = program.get_program_diagnostics();
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code()
                == diagnostics::CANNOT_WRITE_FILE_0_BECAUSE_IT_WOULD_BE_OVERWRITTEN_BY_MULTIPLE_INPUT_FILES.code()
        }),
        "expected the external output collision diagnostic: {diagnostics:#?}"
    );
}

// A path ending in a mapped extension can resolve to a directory; rewriting that specifier would change the target.
#[test]
fn content_mapper_output_extension_unsafe_directory_rewrite_is_reported() {
    let Some(program) = new_content_mapper_program_with_output_extensions(
        fake_host(|_, _| Ok(result("export {};", ".ts", tsrs_spanmap::new(&[])))),
        &[
            ("/src/components.vue/index.ts", "export const directory = 1;"),
            (
                "/src/main.ts",
                "import { directory } from './components.vue';\nconsole.log(directory);\n",
            ),
        ],
        &["/src/main.ts"],
        CompilerOptions {
            target: tsrs_core::ScriptTarget::ES_NEXT,
            rewrite_relative_import_extensions: Tristate::True,
            allow_arbitrary_extensions: Tristate::True,
            ..options(ModuleKind::PRESERVE, ModuleResolutionKind::BUNDLER)
        },
        &[(".vue", ".js")],
    ) else {
        return;
    };

    let diagnostics = collect_content_mapper_diagnostics(program);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code()
                == diagnostics::THIS_RELATIVE_IMPORT_PATH_IS_UNSAFE_TO_REWRITE_BECAUSE_IT_LOOKS_LIKE_A_FILE_NAME_BUT_ACTUALLY_RESOLVES_TO_0.code()
        }),
        "expected an unsafe extension rewrite diagnostic: {diagnostics:#?}"
    );
}
