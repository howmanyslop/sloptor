//! `output_guard_test.go`

use crate::output::assert_local_output_path;

#[test]
fn assert_local_output_path_cases() {
	for ok in ["out/main.luau", "out/sub/init.luau", "out/weird..name.luau"] {
		assert!(
			assert_local_output_path(ok).is_ok(),
			"assertLocalOutputPath({ok:?}) = error, want nil"
		);
	}
	for bad in ["../escape.luau", "out/../../escape.luau", "/abs/escape.luau", ".."] {
		assert!(
			assert_local_output_path(bad).is_err(),
			"assertLocalOutputPath({bad:?}) = nil, want error"
		);
	}
	if cfg!(windows) {
		assert!(
			assert_local_output_path("C:/abs/escape.luau").is_err(),
			"assertLocalOutputPath(\"C:/abs/escape.luau\") = nil, want error on Windows"
		);
	}
}
