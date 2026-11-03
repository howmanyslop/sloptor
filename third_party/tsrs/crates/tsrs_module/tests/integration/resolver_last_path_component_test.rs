use tsrs_core::tspath;

use tsrs_module::resolver::*;

#[test]
fn matches_the_last_path_component() {
    for path in [
        "/",
        "/a",
        "/a/",
        "/a/.",
        "/a/..",
        "/a/./",
        "/a/../",
        "/a//",
        "/a/b/c.ts",
        "/a/b//",
        "c:/",
        "c:/x/..",
        "//server/share/x",
        "file:///a/b",
        "a",
        "a/",
        "",
        ".",
        "..",
        "./",
        "/.",
        "/..",
    ] {
        let parts = tspath::get_path_components(path, "");
        assert_eq!(last_path_component(path), parts[parts.len() - 1], "{path:?}");
    }
}
