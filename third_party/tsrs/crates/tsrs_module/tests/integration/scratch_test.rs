use tsrs_module::scratch::*;

#[test]
fn nested_and_concurrent_paths_do_not_overwrite_each_other() {
    with_path_buffer(PathBuffer::Extension, |outer| {
        outer.push_str("/outer/é.ts");
        with_path_buffer(PathBuffer::Extension, |inner| {
            assert_eq!(inner, "");
            inner.push_str("/inner.ts");
        });
        std::thread::spawn(|| {
            with_path_buffer(PathBuffer::Extension, |other| other.push_str("/other.ts"));
        })
        .join()
        .unwrap();
        assert_eq!(outer, "/outer/é.ts");
    });
    with_path_buffer(PathBuffer::Extension, |path| assert_eq!(path, ""));
}

#[test]
fn long_paths_and_unwinding_leave_the_buffer_usable() {
    with_path_buffer(PathBuffer::Suffix, |path| {
        path.push_str(&"é".repeat(40_000));
    });
    with_path_buffer(PathBuffer::Suffix, |path| {
        assert_eq!(path, "");
        assert!(path.capacity() <= 64 * 1024);
    });
    assert!(
        std::panic::catch_unwind(|| with_path_buffer(PathBuffer::Suffix, |path| {
            path.push_str("/abandoned");
            panic!("host callback");
        }))
        .is_err()
    );
    with_path_buffer(PathBuffer::Suffix, |path| {
        assert_eq!(path, "");
        assert!(path.capacity() <= 64 * 1024);
        path.push_str("/next.ts");
    });
}
