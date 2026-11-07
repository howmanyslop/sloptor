//! The one argv rewrite that runs before usage parses: a leading `--build` selects `build`.

use std::ffi::OsString;

/// A leading `--build` or `-b` (the shape TypeScript build integrations such as Nx use) selects
/// the build command.
#[must_use]
pub fn normalize_compiler_invocation(mut args: Vec<OsString>) -> Vec<OsString> {
	let leading_build = args.first().and_then(|first| first.to_str()).is_some_and(|first| {
		first == "--build" || first == "-b" || first.starts_with("--build=") || first.starts_with("-b=")
	});
	if leading_build {
		args.insert(0, OsString::from("build"));
	}
	args
}

#[cfg(test)]
mod tests {
	use std::ffi::OsString;

	use super::normalize_compiler_invocation;

	fn args(list: &[&str]) -> Vec<OsString> {
		list.iter().map(OsString::from).collect()
	}

	fn strings(list: Vec<OsString>) -> Vec<String> {
		list.into_iter().map(|arg| arg.to_string_lossy().into_owned()).collect()
	}

	#[test]
	fn a_leading_build_flag_selects_the_build_command() {
		for flag in ["--build", "-b", "--build=tsconfig.json", "-b=x"] {
			assert_eq!(strings(normalize_compiler_invocation(args(&[flag]))), ["build", flag]);
		}
		assert_eq!(
			strings(normalize_compiler_invocation(args(&["check", "--build"]))),
			["check", "--build"]
		);
	}
}
