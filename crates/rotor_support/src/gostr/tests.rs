use super::quote;

#[test]
fn quote_matches_strconv_quote() {
	assert_eq!(quote("plain"), "\"plain\"");
	assert_eq!(quote("a\"b\\c"), r#""a\"b\\c""#);
	assert_eq!(quote("tab\there\nline"), r#""tab\there\nline""#);
	assert_eq!(quote("nul\0del\x7f"), r#""nul\x00del\x7f""#);
	assert_eq!(quote("café $internal"), "\"café $internal\"");
	assert_eq!(quote("nbsp\u{a0}"), "\"nbsp\\u00a0\"");
	assert_eq!(quote(""), "\"\"");
}

#[cfg(unix)]
#[test]
fn io_errors_read_like_go() {
	use std::io;

	use super::{io_error_text, path_error};

	let missing = io::Error::from_raw_os_error(2);
	assert_eq!(io_error_text(&missing), "no such file or directory");
	assert_eq!(
		path_error("open", "/x/rotor.toml", &missing),
		"open /x/rotor.toml: no such file or directory"
	);
	let denied = io::Error::from_raw_os_error(13);
	assert_eq!(io_error_text(&denied), "permission denied");
}
