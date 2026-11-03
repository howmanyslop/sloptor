use tsrs_core::debug::debug::*;

fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let err = std::panic::catch_unwind(f).unwrap_err();
    err.downcast_ref::<String>().cloned().unwrap_or_default()
}

#[test]
fn messages() {
    assert_eq!(panic_message(|| fail("")), "Debug failure.");
    assert_eq!(panic_message(|| fail("x")), "Debug failure. x");
    assert_eq!(panic_message(|| assert(false, &[])), "Debug failure. False expression.");
    assert_eq!(panic_message(|| assert(false, &[&"a", &1])), "Debug failure. False expression: a1");
    assert_eq!(panic_message(|| assert_never(&"v", &[])), "Debug failure. Illegal value: v");
    assert_eq!(
        panic_message(|| fail_bad_syntax_kind("KindFoo", &[])),
        "Debug failure. Unexpected node.\nNode KindFoo was unexpected."
    );
    assert(true, &[]);
}
