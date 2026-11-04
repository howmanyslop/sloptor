//! argv rewrites that run before clap parses (`normalizeCompilerInvocation` and the `--no-*` half
//! of `normalizeLegacyArgs` in `cmd/rotor/cli.go`).
//!
//! clap reads every other spelling Go's normalizer rewrote: the optional values of `--build`,
//! `--rojo` and `--max-errors`, and the flag-like token after `--project`, which clap never takes
//! as a value.

use std::ffi::OsString;

use crate::cli;

/// `normalizeCompilerInvocation`: a leading `--build` or `-b` (the shape TypeScript build
/// integrations such as Nx use) selects the build command.
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

/// `sloptor help [COMMAND]`: the same as `sloptor [COMMAND] --help`, so the help renders through
/// the styled path that `--help` takes. `help` counts only as the first token.
#[must_use]
pub fn normalize_help_command(mut args: Vec<OsString>) -> Vec<OsString> {
	if args.first().is_some_and(|first| first == "help") {
		args.remove(0);
		args.truncate(1);
		args.push(OsString::from("--help"));
	}
	args
}

/// The subcommand `args` name: the first token that is not a flag (or the value of the root's
/// `--color`). Tokens after `--` are never commands.
#[must_use]
pub fn resolve_subcommand(root: &clap::Command, args: &[OsString]) -> Option<String> {
	let mut tokens = args.iter().filter_map(|arg| arg.to_str());
	while let Some(token) = tokens.next() {
		if token == "--" {
			return None;
		}
		if token == "--color" {
			tokens.next();
			continue;
		}
		if token.starts_with('-') {
			continue;
		}
		return root
			.get_subcommands()
			.find(|subcommand| {
				subcommand.get_name() == token || subcommand.get_all_aliases().any(|alias| alias == token)
			})
			.map(|subcommand| subcommand.get_name().to_owned());
	}
	None
}

/// The long names (primary and aliases) of the booleans `command` and its parent accept, each
/// with its primary name.
fn boolean_names(root: &clap::Command, subcommand: Option<&str>) -> Vec<(String, String)> {
	let mut names = Vec::new();
	let mut collect = |command: &clap::Command| {
		for arg in command.get_arguments().filter(|arg| cli::is_bool(arg)) {
			let Some(long) = arg.get_long() else {
				continue;
			};
			names.push((long.to_owned(), long.to_owned()));
			for alias in arg.get_all_aliases().into_iter().flatten() {
				names.push((alias.to_owned(), long.to_owned()));
			}
		}
	};
	collect(root);
	if let Some(found) = subcommand.and_then(|name| root.find_subcommand(name)) {
		collect(found);
	}
	names
}

/// The `--no-<boolean>` half of `normalizeLegacyArgs`: `--no-<flag>` becomes `--<flag>=false`,
/// and `--no-<flag>=<value>` the inverted value, for every boolean of the resolved command
/// (camelCase aliases included). Anything else, including an unknown `--no-*`, is left for clap.
#[must_use]
pub fn normalize_negations(root: &clap::Command, args: Vec<OsString>) -> Vec<OsString> {
	let subcommand = resolve_subcommand(root, &args);
	let names = boolean_names(root, subcommand.as_deref());
	let mut out = Vec::with_capacity(args.len());
	let mut rest = false;
	for arg in args {
		if rest {
			out.push(arg);
			continue;
		}
		let Some(text) = arg.to_str() else {
			out.push(arg);
			continue;
		};
		if text == "--" {
			rest = true;
			out.push(arg);
			continue;
		}
		let Some(negated) = text.strip_prefix("--no-") else {
			out.push(arg);
			continue;
		};
		let (name, value) = match negated.split_once('=') {
			Some((name, value)) => (name, Some(value)),
			None => (negated, None),
		};
		let Some((_, primary)) = names.iter().find(|(spelling, _)| spelling == name) else {
			out.push(arg);
			continue;
		};
		let value = value.map_or_else(|| "false".to_owned(), invert_bool);
		out.push(OsString::from(format!("--{primary}={value}")));
	}
	out
}

/// `invertBool`: flips an explicit boolean; anything unrecognized passes through for clap to
/// reject.
fn invert_bool(value: &str) -> String {
	match cli::go_bool(value) {
		Ok(true) => "false".to_owned(),
		Ok(false) => "true".to_owned(),
		Err(_) => value.to_owned(),
	}
}

#[cfg(test)]
mod tests {
	use std::ffi::OsString;

	use super::{normalize_compiler_invocation, normalize_help_command, normalize_negations, resolve_subcommand};
	use crate::cli;

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

	#[test]
	fn the_subcommand_is_the_first_bare_token() {
		let root = cli::command();
		assert_eq!(
			resolve_subcommand(&root, &args(&["--color", "never", "build"])).as_deref(),
			Some("build")
		);
		assert_eq!(
			resolve_subcommand(&root, &args(&["completions", "zsh"])).as_deref(),
			Some("completion")
		);
		assert_eq!(resolve_subcommand(&root, &args(&["--", "build"])), None);
	}

	#[test]
	fn negations_rewrite_every_spelling_of_a_boolean() {
		let root = cli::command();
		let got = strings(normalize_negations(
			&root,
			args(&[
				"build",
				"--no-luau",
				"--no-optimizedLoops",
				"--no-single-threaded=false",
				"--no-clear=maybe",
				"--no-bogus",
				"--",
				"--no-luau",
			]),
		));
		assert_eq!(
			got,
			[
				"build",
				"--luau=false",
				"--optimized-loops=false",
				"--single-threaded=true",
				"--clear=maybe",
				"--no-bogus",
				"--",
				"--no-luau",
			]
		);
	}

	#[test]
	fn a_negation_only_applies_to_the_resolved_command() {
		let root = cli::command();
		assert_eq!(
			strings(normalize_negations(&root, args(&["check", "--no-luau"]))),
			["check", "--no-luau"]
		);
		assert_eq!(
			strings(normalize_negations(&root, args(&["check", "--no-json"]))),
			["check", "--json=false"]
		);
	}

	#[test]
	fn help_command_becomes_a_help_flag() {
		assert_eq!(strings(normalize_help_command(args(&["help"]))), ["--help"]);
		assert_eq!(
			strings(normalize_help_command(args(&["help", "build"]))),
			["build", "--help"]
		);
		assert_eq!(
			strings(normalize_help_command(args(&["help", "build", "x"]))),
			["build", "--help"]
		);
		assert_eq!(
			strings(normalize_help_command(args(&["build", "help"]))),
			["build", "help"]
		);
	}
}
