use tsrs_ast::{ExternalModuleIndicatorOptions, SourceFileParseOptions};
use tsrs_core::{ScriptKind, tspath::Path};

#[test]
fn unicode_jsdoc_indentation() {
    for (source, expected) in [
        (
            "/**\n     * @see example\n     * @example\n     ```javascript\n     var e = 1;\n     \u{200b}\n     use(e);\n     ```\n     */\nexport const value = 1;",
            "```javascript\nvar e = 1;\n\u{fffd}\nuse(e);\n```",
        ),
        (
            "/**\n     * example\n     \u{200b}\n     * @see example\n     */\nexport const value = 1;",
            "example\n\u{fffd}",
        ),
        (
            "/**\n * @deprecated\n \u{200b}\u{200b}\u{200b}example\n */\nexport const value = 1;",
            "\u{fffd}\u{200b}\u{200b}example",
        ),
        ("/**\n * @deprecated\n  \u{a0}example\n */\nexport const value = 1;", "\u{fffd}example"),
        (
            "/**\n * @deprecated\n  \u{200b}example\n */\nexport const value = 1;",
            "\u{fffd}\u{fffd}example",
        ),
        (
            "/**\n * @deprecated\n  \u{feff}example\n */\nexport const value = 1;",
            "\u{fffd}\u{fffd}example",
        ),
    ] {
        for (file_name, script_kind) in [("/a.ts", ScriptKind::TS), ("/a.js", ScriptKind::JS)] {
            let file = tsrs_parser::parse_source_file(
                SourceFileParseOptions {
                    file_name: file_name.to_owned(),
                    path: Path::new(file_name.to_owned()),
                    external_module_indicator_options: ExternalModuleIndicatorOptions::default(),
                },
                source,
                script_kind,
            );
            let docs = file.statements().nodes()[0].jsdoc(Some(file.get()));
            assert_eq!(docs.len(), 1);
            let doc = docs[0];
            let mut comments: Vec<_> = doc.comments().iter().map(|part| part.text()).collect();
            if let Some(tags) = doc.as_jsdoc().tags() {
                for tag in tags.nodes() {
                    comments.extend(tag.comments().iter().map(|part| part.text()));
                }
            }
            assert_eq!(comments, [expected]);
            assert_eq!(file.diagnostics(), []);
        }
    }
}
