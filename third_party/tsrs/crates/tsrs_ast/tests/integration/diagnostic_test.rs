use std::cmp::Ordering;
use tsrs_core::TextRange;
use tsrs_diagnostics as diagnostics;
use tsrs_diagnostics::Category;

use tsrs_ast::diagnostic::*;

#[test]
fn diagnostics_collection_deduplicates_exact_diagnostics_on_add() {
    let mut collection = DiagnosticsCollection::default();
    let first =
        new_compiler_diagnostic(&diagnostics::CANNOT_FIND_NAME_0, &[&"x"]).add_related_info(Some(
            new_compiler_diagnostic(&diagnostics::X_0_IS_DECLARED_HERE, &[&"first"]),
        ));
    let second =
        new_compiler_diagnostic(&diagnostics::CANNOT_FIND_NAME_0, &[&"x"]).add_related_info(Some(
            new_compiler_diagnostic(&diagnostics::X_0_IS_DECLARED_HERE, &[&"first"]),
        ));
    let different =
        new_compiler_diagnostic(&diagnostics::CANNOT_FIND_NAME_0, &[&"x"]).add_related_info(Some(
            new_compiler_diagnostic(&diagnostics::X_0_IS_DECLARED_HERE, &[&"second"]),
        ));

    assert_eq!(collection.add(first), first);
    let canonical = collection.add(second);
    assert_eq!(canonical, first);
    assert_eq!(collection.add(different), different);

    canonical.add_related_info(Some(new_compiler_diagnostic(
        &diagnostics::X_0_IS_DECLARED_HERE,
        &[&"third"],
    )));
    let collected = collection.get_global_diagnostics();
    assert_eq!(collected.len(), 2);
    assert_eq!(first.related_information().len(), 2);
}

#[test]
fn diagnostics_collection_preserves_distinct_ad_hoc_messages() {
    let mut collection = DiagnosticsCollection::default();
    let first = new_compiler_diagnostic(new_ad_hoc_message("first"), &[]);
    let second = new_compiler_diagnostic(new_ad_hoc_message("second"), &[]);
    collection.add(first);
    collection.add(second);
    assert_eq!(collection.get_global_diagnostics().len(), 2);
}

#[test]
fn external_diagnostic_identity() {
    let loc = TextRange::new(1, 2);
    let first = new_external_diagnostic(None, loc, "mapper-a", Category::Error, 0, "first");
    let all = [
        first,
        new_external_diagnostic(None, loc, "mapper-a", Category::Error, 0, "second"),
        new_external_diagnostic(None, loc, "mapper-b", Category::Error, 0, "first"),
        new_external_diagnostic(None, loc, "mapper-a", Category::Warning, 0, "first"),
    ];
    let mut collection = DiagnosticsCollection::default();
    for &diagnostic in &all {
        assert!(!equal_diagnostics_no_related_info(first, diagnostic) || diagnostic == first);
        assert!(compare_diagnostics(first, diagnostic) != Ordering::Equal || diagnostic == first);
        collection.add(diagnostic);
    }
    assert_eq!(collection.get_diagnostics().len(), all.len());
}

// displayMessageArgs (diagnostic.go:134): a diagnostic on exactly a virtual alias shows the original name instead of
// the virtual one; a partial span or an external source shows the stored arguments, which never change.
#[test]
fn display_message_args_substitutes_complete_alias() {
    let f = tsrs_ast::NodeFactory::default();
    let statements = f.new_node_list(vec![]);
    let eof = f.new_token(tsrs_ast::Kind::EndOfFile);
    let opts =
        tsrs_ast::SourceFileParseOptions { file_name: "/a.vue".to_string(), ..Default::default() };
    let file =
        f.new_source_file(opts.clone(), "let __alias = 1;", statements, eof).as_source_file_p();
    let span_map = tsrs_spanmap::new(&[tsrs_spanmap::Segment {
        virtual_start: 4,
        virtual_end: 11,
        original_start: 5,
        original_end: 6,
        kind: tsrs_spanmap::Kind::ALIAS,
        features: tsrs_spanmap::Feature::All,
    }]);
    file.set_content_mapper_info(tsrs_ast::ContentMapperSourceFileInfo {
        content_mapper: "mapper@1.0.0",
        parse_options: opts,
        original_text: "<a b=x>",
        span_map: Some(span_map),
        ..Default::default()
    });

    let alias = new_diagnostic(
        Some(file),
        TextRange::new(4, 11),
        &diagnostics::CANNOT_FIND_NAME_0,
        &[&"__alias"],
    );
    assert_eq!(alias.localize(), "Cannot find name 'x'.");
    assert_eq!(alias.message_args(), ["__alias"]);
    let partial = new_diagnostic(
        Some(file),
        TextRange::new(5, 11),
        &diagnostics::CANNOT_FIND_NAME_0,
        &[&"__alias"],
    );
    assert_eq!(partial.localize(), "Cannot find name '__alias'.");
    let external = new_diagnostic(
        Some(file),
        TextRange::new(4, 11),
        &diagnostics::CANNOT_FIND_NAME_0,
        &[&"__alias"],
    )
    .set_external_data("mapper", "");
    assert_eq!(external.localize(), "Cannot find name '__alias'.");
}
