use std::time::Duration;

use tsrs_core::context::*;

#[test]
fn cancel_propagates_to_children_and_runs_after_funcs() {
    let bg = Context::background();
    assert!(bg.done_is_nil());
    let ctx = with_request_id(&with_checker_lifetime(&bg, CheckerLifetime::Diagnostics), "42");
    let (parent, cancel) = ctx.with_cancel();
    let (child, _child_cancel) = with_request_id(&parent, "43").with_cancel();
    assert_eq!(get_request_id(&child), "43");
    assert_eq!(get_request_id(&parent), "42");
    assert_eq!(get_checker_lifetime(&child), CheckerLifetime::Diagnostics);
    assert!(!child.done_is_nil());
    let (tx, rx) = std::sync::mpsc::channel();
    child.after_func(move || tx.send(()).unwrap());
    assert!(child.err().is_none());
    cancel.call();
    assert_eq!(parent.err(), Some(ContextError::Canceled));
    assert_eq!(child.err(), Some(ContextError::Canceled));
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (late, _) = parent.with_cancel();
    assert!(late.is_canceled());
    assert!(!with_request_id(&bg, "x").is_canceled());
}

#[test]
fn timeout_expires() {
    let (ctx, _cancel) = Context::background().with_timeout(Duration::from_millis(10));
    assert!(ctx.wait_timeout(Duration::from_secs(5)));
    assert_eq!(ctx.err(), Some(ContextError::DeadlineExceeded));
}
