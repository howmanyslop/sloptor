// tsoptions/contentmappers_test.go

use tsrs_core::collections::OrderedMap;
use tsrs_core::tspath::ComparePathsOptions;
use tsrs_core::tspath::ExtensionRewrite;
use tsrs_core::{CompilerOptions, JsxEmit, P, ScriptTarget, Tristate};
use tsrs_diagnostics as diagnostics;

use tsrs_tsoptions::contentmappers::{
    Definition, Manifest, Mapper, resolve_content_mapper_manifest,
};
use tsrs_tsoptions::parsedcommandline::{ParsedCommandLine, new_parsed_command_line};
use tsrs_tsoptions::parsedoptions::ParsedOptions;
use tsrs_tsoptions::tsoptionstest::new_vfs_parse_config_host;

fn mapper(package: &str, extensions: &[&str]) -> Mapper {
    Mapper {
        definition: Definition {
            package: package.to_string(),
            extensions: extensions.iter().map(std::string::ToString::to_string).collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// contentmappers_test.go:20 (Go compares the *Mapper pointers; here the elements of the command line's list.)
#[test]
fn test_get_content_mapper_for_file_name_uses_longest_extension() {
    let command_line = ParsedCommandLine {
        parsed_config: ParsedOptions {
            content_mappers: vec![mapper("z", &[".z"]), mapper("yz", &[".y.z"])].into(),
            ..Default::default()
        },
        ..ParsedCommandLine::empty()
    };
    let z_mapper = &command_line.content_mappers()[0];
    let yz_mapper = &command_line.content_mappers()[1];

    assert!(std::ptr::eq(
        command_line.get_content_mapper_for_file_name("/src/Component.y.z").unwrap(),
        yz_mapper
    ));
    assert!(std::ptr::eq(
        command_line.get_content_mapper_for_file_name("/src/Component.z").unwrap(),
        z_mapper
    ));
}

// contentmappers_test.go:30
#[test]
fn test_get_content_mapper_for_file_name_uses_host_case_sensitivity() {
    let insensitive = ParsedCommandLine {
        parsed_config: ParsedOptions {
            content_mappers: vec![mapper("", &[".vue"])].into(),
            ..Default::default()
        },
        compare_paths_options: ComparePathsOptions {
            use_case_sensitive_file_names: false,
            ..Default::default()
        },
        ..ParsedCommandLine::empty()
    };
    let sensitive = ParsedCommandLine {
        parsed_config: ParsedOptions {
            content_mappers: vec![mapper("", &[".vue"])].into(),
            ..Default::default()
        },
        compare_paths_options: ComparePathsOptions {
            use_case_sensitive_file_names: true,
            ..Default::default()
        },
        ..ParsedCommandLine::empty()
    };

    assert!(std::ptr::eq(
        insensitive.get_content_mapper_for_file_name("/src/Component.VUE").unwrap(),
        &raw const insensitive.content_mappers()[0]
    ));
    assert!(sensitive.get_content_mapper_for_file_name("/src/Component.VUE").is_none());
}

// contentmappers_test.go:46
#[test]
fn test_get_output_file_names_excludes_mapper_owned_outputs() {
    let mut command_line = new_parsed_command_line(
        P::new(CompilerOptions {
            out_dir: "/dist".to_string(),
            declaration: Tristate::True,
            declaration_map: Tristate::True,
            source_map: Tristate::True,
            ..Default::default()
        }),
        vec!["/src/Component.vue".to_string()],
        Vec::new(),
        ComparePathsOptions {
            current_directory: "/".to_string(),
            use_case_sensitive_file_names: true,
        },
    );
    command_line.parsed_config.content_mappers = vec![mapper("", &[".vue"])].into();

    assert_eq!(command_line.get_output_file_names(), vec!["/dist/Component.d.vue.ts".to_string()]);
}

#[test]
fn test_content_mapper_extension_rewrites_preserve_longest_extension() {
    let mut command_line = new_parsed_command_line(
        P::new(CompilerOptions {
            out_dir: "/dist".into(),
            declaration: Tristate::True,
            ..Default::default()
        }),
        vec![
            "/src/Widget.y.z".into(),
            "/src/Widget.identity.y.z".into(),
            "/src/Widget.unmapped.y.z".into(),
        ],
        Vec::new(),
        ComparePathsOptions { current_directory: "/".into(), use_case_sensitive_file_names: true },
    );
    let mut mapper = mapper("", &[".z", ".y.z", ".identity.y.z", ".unmapped.y.z"]);
    mapper.definition.output_extensions = [
        (".z".into(), ".js".into()),
        (".y.z".into(), ".mjs".into()),
        (".identity.y.z".into(), ".identity.y.z".into()),
    ]
    .into_iter()
    .collect();
    command_line.parsed_config.content_mappers = vec![mapper].into();
    let rewrites = command_line.content_mapper_extension_rewrites();
    assert_eq!(rewrites.len(), 4);
    assert!(rewrites.windows(2).all(|pair| pair[0].source.len() >= pair[1].source.len()));
    for file in ["./Widget.identity.y.z", "./Widget.unmapped.y.z"] {
        assert_eq!(
            tsrs_core::tspath::rewrite_extension(file, rewrites, true),
            (file.into(), false)
        );
    }
    assert_eq!(
        command_line.get_output_file_names(),
        vec![
            "/dist/Widget.d.mts",
            "/dist/Widget.d.identity.y.z.ts",
            "/dist/Widget.d.unmapped.y.z.ts",
        ]
    );
    assert_eq!(
        tsrs_core::tspath::rewrite_extension("./Widget.y.z", rewrites, true),
        ("./Widget.mjs".into(), true),
    );
    assert!(std::ptr::eq(
        rewrites.as_ptr(),
        command_line.content_mapper_extension_rewrites().as_ptr(),
    ));
}

#[test]
fn test_content_mapper_output_paths_case_insensitive() {
    let mut command_line = new_parsed_command_line(
        P::new(CompilerOptions { out_dir: "/dist".into(), ..Default::default() }),
        vec!["/src/Widget.Y.Z".into()],
        Vec::new(),
        ComparePathsOptions { current_directory: "/".into(), use_case_sensitive_file_names: false },
    );
    let mut mapper = mapper("", &[".y.z"]);
    mapper.definition.output_extensions.insert(".y.z".into(), ".mjs".into());
    command_line.parsed_config.content_mappers = vec![mapper].into();
    assert_eq!(
        command_line.content_mapper_extension_rewrites(),
        &[ExtensionRewrite { source: ".y.z".into(), target: ".mjs".into() }]
    );
    let options = command_line.compiler_options().unwrap();
    assert_eq!(
        tsrs_tsoptions::outputpaths::get_external_output_file_name(
            "/src/Widget.Y.Z",
            &options,
            &command_line
        ),
        "/dist/Widget.mjs"
    );
    assert_eq!(
        tsrs_tsoptions::outputpaths::get_output_declaration_file_name_worker(
            "/src/Widget.Y.Z",
            &options,
            &command_line
        ),
        "/dist/Widget.d.mts"
    );
}

#[test]
fn test_transform_identity_includes_output_extensions() {
    let mut mapper = mapper("vue", &[".vue"]);
    mapper.manifest.name = "vue".into();
    let original = mapper.transform_identity(None);
    mapper.definition.output_extensions.insert(".vue".into(), ".js".into());
    assert_ne!(mapper.transform_identity(None), original);
    let mapped = mapper.transform_identity(None);
    mapper.manifest.default_output_extensions.insert(".vue".into(), ".cjs".into());
    assert_eq!(mapper.transform_identity(None), mapped);
}

#[test]
fn test_empty_output_extension_map_differs_from_missing_in_mapper_equality() {
    let missing = Mapper::default();
    let mut explicit_empty = missing.clone();
    explicit_empty.definition.output_extensions_present = true;
    assert_ne!(missing, explicit_empty);
    let mut manifest_empty = missing.clone();
    manifest_empty.manifest.default_output_extensions_present = true;
    assert_ne!(missing, manifest_empty);
}

// contentmappers_test.go:68
#[test]
fn test_resolve_content_mapper_manifest() {
    let host = new_vfs_parse_config_host(
        &[
            (
                "/home/project/node_modules/vue-ts-mapper/package.json",
                r#"{
            "name": "vue-ts-mapper",
            "version": "1.2.3",
            "typescript": { "contentMapper": { "exec": ["node", "./dist/mapper.js"], "compilerOptions": ["target", "jsx"], "outputExtensions": { ".vue": ".js" } } }
        }"#,
            ),
            (
                "/home/node_modules/@scope/noversion/package.json",
                r#"{
            "name": "@scope/noversion",
            "typescript": { "contentMapper": { "exec": ["run"] } }
        }"#,
            ),
            (
                "/home/project/node_modules/no-name/package.json",
                r#"{
            "version": "1.0.0"
        }"#,
            ),
            (
                "/home/project/node_modules/no-manifest/package.json",
                r#"{
            "name": "no-manifest"
        }"#,
            ),
            (
                "/home/project/node_modules/no-exec/package.json",
                r#"{
            "name": "no-exec",
            "typescript": { "contentMapper": {} }
        }"#,
            ),
            (
                "/home/project/node_modules/bad-exec/package.json",
                r#"{
            "name": "bad-exec",
            "typescript": { "contentMapper": { "exec": "node ./mapper.js" } }
        }"#,
            ),
        ],
        "/home/project",
        true, /*useCaseSensitiveFileNames*/
    );

    // Name, version, and the verbatim exec argv are preserved.
    let (manifest, package_directory, diagnostic) =
        resolve_content_mapper_manifest(host, "/home/project/tsconfig.json", "vue-ts-mapper");
    assert!(diagnostic.is_none());
    assert_eq!(manifest.name, "vue-ts-mapper");
    assert_eq!(manifest.version, "1.2.3");
    assert_eq!(package_directory, "/home/project/node_modules/vue-ts-mapper");
    assert_eq!(manifest.exec, ["node", "./dist/mapper.js"]);
    assert_eq!(manifest.compiler_options, ["target", "jsx"]);
    assert_eq!(manifest.default_output_extensions.get(".vue").map(String::as_str), Some(".js"));
    assert!(manifest.default_output_extensions_present);

    // Resolution walks up node_modules; a package with no version resolves to a name and empty version.
    let (manifest, _, diagnostic) = resolve_content_mapper_manifest(
        host,
        "/home/project/src/tsconfig.json",
        "@scope/noversion",
    );
    assert!(diagnostic.is_none());
    assert_eq!(manifest.name, "@scope/noversion");
    assert_eq!(manifest.version, "");

    // A package that is not installed reports a resolution diagnostic.
    let (_, _, diagnostic) =
        resolve_content_mapper_manifest(host, "/home/project/tsconfig.json", "missing-mapper");
    assert_eq!(
        diagnostic.unwrap().code(),
        diagnostics::THE_CONTENT_MAPPER_PACKAGE_0_COULD_NOT_BE_RESOLVED.code()
    );

    // A package whose package.json has no name reports a diagnostic.
    let (_, package_directory, diagnostic) =
        resolve_content_mapper_manifest(host, "/home/project/tsconfig.json", "no-name");
    assert_eq!(package_directory, "/home/project/node_modules/no-name");
    assert_eq!(
        diagnostic.unwrap().code(),
        diagnostics::THE_PACKAGE_JSON_OF_THE_CONTENT_MAPPER_PACKAGE_0_DOES_NOT_SPECIFY_A_NAME
            .code()
    );

    // A package that does not declare a "typescript.contentMapper" object reports a diagnostic.
    let (_, _, diagnostic) =
        resolve_content_mapper_manifest(host, "/home/project/tsconfig.json", "no-manifest");
    assert_eq!(
        diagnostic.unwrap().code(),
        diagnostics::THE_PACKAGE_JSON_OF_THE_CONTENT_MAPPER_PACKAGE_0_DOES_NOT_DECLARE_A_TYPESCRIPT_CONTENT_MAPPER_OBJECT.code()
    );

    // A "typescript.contentMapper" with no "exec", or an "exec" of the wrong type, reports a diagnostic.
    for pkg in ["no-exec", "bad-exec"] {
        let (_, _, diagnostic) =
            resolve_content_mapper_manifest(host, "/home/project/tsconfig.json", pkg);
        let Some(diagnostic) = diagnostic else {
            panic!("expected a diagnostic for {pkg}");
        };
        assert_eq!(
            diagnostic.code(),
            diagnostics::THE_TYPESCRIPT_CONTENT_MAPPER_EXEC_OF_THE_CONTENT_MAPPER_PACKAGE_0_MUST_BE_A_NON_EMPTY_ARRAY_OF_STRINGS.code()
        );
    }
}

#[test]
fn test_resolve_content_mapper_manifest_rejects_malformed_output_extensions() {
    let values = [
        (r#""invalid""#, 18122),
        ("null", 18122),
        ("[]", 18122),
        (r#"{".vue": 1}"#, 18122),
        (r#"{".vue": null}"#, 18121),
        (r#"{".vue/foo": ".js"}"#, 18123),
        (r#"{".vue": ".js/foo"}"#, 18123),
        (r#"{".vue": "js"}"#, 18121),
    ];
    for (value, expected_code) in values {
        let package_json = format!(
            r#"{{"name":"mapper","typescript":{{"contentMapper":{{"exec":["run"],"outputExtensions":{value}}}}}}}"#
        );
        let host = new_vfs_parse_config_host(
            &[("/home/project/node_modules/mapper/package.json", package_json.leak())],
            "/home/project",
            true,
        );
        let (_, _, diagnostic) =
            resolve_content_mapper_manifest(host, "/home/project/tsconfig.json", "mapper");
        assert_eq!(diagnostic.expect(value).code(), expected_code, "{value}");
    }
}

// contentmapper/host_test.go:300 (TestMapperDiagnosticName: Mapper.DiagnosticName lives in this crate.)
#[test]
fn test_mapper_diagnostic_name() {
    let with = |package: &str, name: &str, contribution_id: &str| Mapper {
        definition: Definition { package: package.to_string(), ..Default::default() },
        manifest: Manifest { name: name.to_string(), ..Default::default() },
        contribution_id: contribution_id.to_string(),
        ..Default::default()
    };
    let tests = [
        (with("configured", "resolved", "contributed"), "resolved"),
        (with("configured", "", "contributed"), "configured"),
        (with("", "", "contributed"), "contributed"),
    ];
    for (mapper, want) in &tests {
        assert_eq!(mapper.diagnostic_name(), *want);
    }
}

// execute/incremental/buildinfo_contentmapper_test.go:25 (TestStaticContentMapperTransformIdentity: Mapper.Identity
// and TransformIdentity live in this crate.)
#[test]
fn test_static_content_mapper_transform_identity() {
    let named =
        |package: &str, options: &str, name: &str, version: &str, compiler_options: &[&str]| {
            Mapper {
                definition: Definition {
                    package: package.to_string(),
                    options: options.to_string(),
                    ..Default::default()
                },
                manifest: Manifest {
                    name: name.to_string(),
                    version: version.to_string(),
                    compiler_options: compiler_options
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect(),
                    ..Default::default()
                },
                ..Default::default()
            }
        };
    assert_eq!(named("", "", "vue", "2.0.0", &[]).identity(), "vue@2.0.0");
    assert_eq!(named("anon", "", "", "", &[]).identity(), "");

    let jsx_mapper = named("jsx", "", "jsx", "1.0.0", &["jsx"]);
    let jsx_preserve_identity = jsx_mapper.transform_identity(Some(&CompilerOptions {
        jsx: JsxEmit::PRESERVE,
        ..Default::default()
    }));
    let jsx_react_identity = jsx_mapper
        .transform_identity(Some(&CompilerOptions { jsx: JsxEmit::REACT, ..Default::default() }));
    assert_ne!(jsx_preserve_identity, jsx_react_identity);

    let options_a = named("vue", r#"{"mode":"a"}"#, "vue", "1.0.0", &[]);
    let options_b = named("vue", r#"{"mode":"b"}"#, "vue", "1.0.0", &[]);
    assert_ne!(
        options_a.transform_identity(Some(&tsrs_core::CompilerOptions::default())),
        options_b.transform_identity(Some(&tsrs_core::CompilerOptions::default()))
    );
}

// The bytes TransformIdentity hashes (contentmapper.go:104): the identity, NUL, the tsconfig options, NUL, and the
// declared compiler options as Go's json.Marshal writes them: declared order, zero-valued and unknown names left out.
// A different encoding would change every content-mapped file's cache identity and build-info version.
#[test]
fn test_transform_identity_hashes_declared_options_json() {
    let mapper = Mapper {
        definition: Definition {
            package: "vue".to_string(),
            options: r#"{"mode":"a"}"#.to_string(),
            ..Default::default()
        },
        manifest: Manifest {
            name: "vue".to_string(),
            version: "1.0.0".to_string(),
            compiler_options: ["target", "noSuchOption", "strict", "noEmit", "jsx", "lib", "paths"]
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut paths = OrderedMap::default();
    paths.insert("@/*".to_string(), Some(vec!["src/*".to_string()]));
    let options = CompilerOptions {
        target: ScriptTarget::ES_NEXT,
        strict: Tristate::False,
        jsx: JsxEmit::PRESERVE,
        lib: Some(vec!["es2022".to_string(), "dom".to_string()]),
        paths: Some(std::sync::Arc::new(paths)),
        ..Default::default()
    };

    let declared = mapper.marshal_declared_options(Some(&options)).unwrap();
    let declared: Vec<(&str, &str)> =
        declared.iter().map(|(name, raw)| (name.as_str(), raw.as_str())).collect();
    assert_eq!(
        declared,
        [
            ("target", "99"),
            ("strict", "false"),
            ("jsx", "1"),
            ("lib", r#"["es2022","dom"]"#),
            ("paths", r#"{"@/*":["src/*"]}"#)
        ]
    );
    let hashed = [
        b"vue@1.0.0".as_slice(),
        b"\0",
        br#"{"mode":"a"}"#,
        b"\0",
        br#"{"target":99,"strict":false,"jsx":1,"lib":["es2022","dom"],"paths":{"@/*":["src/*"]}}"#,
    ]
    .concat();
    assert_eq!(mapper.transform_identity(Some(&options)), xxhash_rust::xxh3::xxh3_128(&hashed));

    // Without options, or without declared names, the declared options are the empty object.
    assert!(mapper.marshal_declared_options(None).unwrap().is_empty());
    let undeclared = Mapper {
        manifest: Manifest { name: "vue".to_string(), ..Default::default() },
        ..Default::default()
    };
    assert_eq!(
        undeclared.transform_identity(Some(&options)),
        xxhash_rust::xxh3::xxh3_128(b"vue\0\0{}")
    );
}
