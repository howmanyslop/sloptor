use tsrs_core::tspath::extension::*;

#[test]
fn declaration_file_extension_is_a_slice_of_the_path() {
    // The slice is taken from the path, not the base name, so the trailing-separator and Windows cases
    // that make the base name a copy must still land on the extension.
    for (path, ext) in [
        ("a/b.d.ts", ".d.ts"),
        ("a/b.d.mts", ".d.mts"),
        ("b.d.json.ts", ".d.json.ts"),
        ("a/b.d.ts/", ".d.ts"),
        ("C:\\x\\b.d.cts", ".d.cts"),
        ("a/b.ts", ""),
        ("a/b.d.ts//", ""),
        ("/", ""),
        ("", ""),
    ] {
        assert_eq!(get_declaration_file_extension(path), ext, "{path:?}");
        assert_eq!(is_declaration_file_name(path), !ext.is_empty(), "{path:?}");
    }
}
