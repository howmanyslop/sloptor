#[cfg(all(compressed_ptrs, unix))]
mod arena_retired_test;
mod arena_test;
mod bfs_test;
mod collections {
    mod cow_test;
    mod ordered_map_test;
    mod ordered_set_test;
    mod set_test;
    mod syncmap_test;
}
mod compileroptions_equality_tests;
mod context_test;
mod core_test;
mod debug {
    mod debug_test;
}
mod frozen_test;
mod glob {
    mod glob_test;
}
mod gocompat_test;
mod goslices_test;
mod jsnum {
    mod big_test;
    mod jsnum_test;
    mod pseudobigint_test;
    mod ryu_test;
    mod string_test;
}
mod json {
    mod json_test;
    mod map_test;
    mod plain_test;
}
mod linkstore_test;
mod pattern_test;
mod ptr_test;
#[cfg(compressed_ptrs)]
mod reserve_test;
mod runslots_test;
mod semver {
    mod version_range_test;
    mod version_test;
}
mod stringutil {
    mod compare_test;
    mod js_case_test;
    mod norm_test;
    mod rangetable_test;
    mod util_test;
}
mod text_test;
mod tspath {
    mod extension_test;
    mod extensionrewrite_test;
    mod path_test;
}
