//! Port of `internal/term/term_test.go`.

use super::Styler;

#[test]
fn spark_scales_to_max() {
	let s = Styler { color: true };
	assert_eq!(s.spark(&[0.0, 50.0, 100.0]), "▁▅█");
}

#[test]
fn spark_ascii_fallback() {
	let s = Styler { color: false };
	assert_eq!(s.spark(&[0.0, 100.0]), ".%");
}

#[test]
fn spark_all_zero_and_empty() {
	let s = Styler { color: true };
	assert_eq!(s.spark(&[]), "", "Spark(nil)");
	assert_eq!(s.spark(&[0.0, 0.0]), "▁▁", "Spark(zeros)");
}
