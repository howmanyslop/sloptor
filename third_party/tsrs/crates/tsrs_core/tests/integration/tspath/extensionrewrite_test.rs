use tsrs_core::tspath::extensionrewrite::*;

#[test]
fn longest_extension_and_identity_stop_shorter_rewrite() {
    let rewrites = [
        ExtensionRewrite { source: ".z".into(), target: ".js".into() },
        ExtensionRewrite { source: ".y.z".into(), target: ".mjs".into() },
        ExtensionRewrite { source: ".identity.y.z".into(), target: ".identity.y.z".into() },
    ];
    assert_eq!(rewrite_extension("./Widget.y.z", &rewrites, true), ("./Widget.mjs".into(), true));
    assert_eq!(
        rewrite_extension("./Widget.identity.y.z", &rewrites, true),
        ("./Widget.identity.y.z".into(), false)
    );
    assert_eq!(rewrite_extension("./Widget.Y.Z", &rewrites, false), ("./Widget.mjs".into(), true));
    assert!(should_rewrite_extension("./Widget.y.z", &rewrites, true));
    assert!(!should_rewrite_extension("pkg/Widget.y.z", &rewrites, true));
    assert!(!should_rewrite_extension("./Widget.d.ts", &rewrites, true));
}
