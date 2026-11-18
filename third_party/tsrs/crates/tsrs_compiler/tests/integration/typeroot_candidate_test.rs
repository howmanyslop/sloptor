// A relative type reference that climbs out of a type root (resolver.go:659 `getCandidateFromTypeRoot`).

use std::sync::Arc;

use tsrs_core::P;
use tsrs_tsoptions::{self as tsoptions, ParseConfigHost};
use tsrs_vfs::{FS, vfstest};

use tsrs_compiler::{ProgramOptions, new_compiler_host, new_program};

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

// With project references and the source of referenced projects (the linter and the language server), module
// resolution goes through the dts faking host. The primary lookup of `../../node_modules/foo/index.d.ts` in the
// type root `/app/node_modules/@types` lands in the symlinked package `/app/node_modules/foo`. Go resolves the
// candidate to a normalized path. An unnormalized one (`/app/node_modules/@types/../../node_modules/foo/index.d.ts`)
// does not start with the spelling that the faking host records for the symlink, and the faking host panicked with
// "canonical symlink path did not match its presentation path".
#[test]
fn relative_type_reference_from_a_type_root_into_a_symlinked_package() {
    let files = [
        (
            "/app/tsconfig.json",
            vfstest::MapFile::from(
                r#"{"compilerOptions":{"noLib":true},"files":["x/y/a.ts"],"references":[{"path":"./ref"}]}"#,
            ),
        ),
        (
            "/app/ref/tsconfig.json",
            vfstest::MapFile::from(
                r#"{"compilerOptions":{"composite":true,"noLib":true},"files":["r.ts"]}"#,
            ),
        ),
        ("/app/ref/r.ts", vfstest::MapFile::from("export const r = 1;")),
        (
            "/app/x/y/a.ts",
            vfstest::MapFile::from(
                "/// <reference types=\"../../node_modules/foo/index.d.ts\" />\nexport const a: number = foo;",
            ),
        ),
        // The type root must exist for the primary lookup to use it.
        ("/app/node_modules/@types/README.md", vfstest::MapFile::from("")),
        ("/app/node_modules/foo", vfstest::symlink("/app/packages/foo")),
        ("/app/packages/foo/index.d.ts", vfstest::MapFile::from("declare const foo: number;")),
        (
            "/app/packages/foo/package.json",
            vfstest::MapFile::from(r#"{"name":"foo","version":"1.0.0","types":"index.d.ts"}"#),
        ),
    ];
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files, true));
    let config_host: &'static ParseConfigHostImpl =
        Box::leak(Box::new(ParseConfigHostImpl { fs: Arc::clone(&fs) }));
    let (config, diagnostics) = tsoptions::get_parsed_command_line_of_config_file(
        "/app/tsconfig.json",
        None,
        None,
        config_host,
        None,
    );
    assert_eq!(diagnostics, [] as [P<tsrs_ast::Diagnostic>; 0]);
    let mut options = ProgramOptions::new(
        P::new(config.unwrap()),
        new_compiler_host("/", fs, "", None, None, None),
    );
    options.use_source_of_project_reference = true;
    let program = new_program(options);
    let mut file_names: Vec<&str> =
        program.get_source_files().iter().map(|file| file.file_name()).collect();
    file_names.sort_unstable();
    assert_eq!(file_names, ["/app/packages/foo/index.d.ts", "/app/x/y/a.ts"]);
}
