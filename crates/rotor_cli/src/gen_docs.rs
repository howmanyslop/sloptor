//! `sloptor gen-docs <dir>` (`cmd/rotor/gen_docs.go`): the CLI reference as Starlight MDX pages,
//! one per visible command plus an `index.mdx` table, built from the same clap tree as `--help` so
//! the docs cannot drift from it.

use std::fmt::Write as _;

use crate::cli;

/// `sentence`: a capital first letter and a closing period.
fn sentence(summary: &str) -> String {
	let summary = summary.trim();
	let mut characters = summary.chars();
	let Some(first) = characters.next() else {
		return String::new();
	};
	let mut out: String = first.to_uppercase().chain(characters).collect();
	if !out.ends_with('.') {
		out.push('.');
	}
	out
}

/// `escapeMDX`: JSX and expression syntax escaped outside inline code spans.
#[must_use]
pub fn escape_mdx(text: &str) -> String {
	let escape = |segment: &str| {
		segment
			.replace('<', "&lt;")
			.replace('>', "&gt;")
			.replace('{', "&#123;")
			.replace('}', "&#125;")
	};
	let segments: Vec<&str> = text.split('`').collect();
	let last = segments.len() - 1;
	segments
		.iter()
		.enumerate()
		.map(|(index, segment)| {
			// Even segments sit outside code spans, and so does an unmatched final one.
			if index % 2 == 0 || (index == last && last % 2 == 1) {
				escape(segment)
			} else {
				(*segment).to_owned()
			}
		})
		.collect::<Vec<_>>()
		.join("`")
}

/// `docsCell`: safe inside a Markdown table cell.
fn cell(text: &str) -> String {
	escape_mdx(text)
		.replace('|', "\\|")
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
}

fn about(command: &clap::Command) -> String {
	command.get_about().map(ToString::to_string).unwrap_or_default()
}

fn flag_syntax(arg: &clap::Arg) -> String {
	let mut parts = Vec::new();
	if let Some(short) = arg.get_short() {
		parts.push(format!("-{short}"));
	}
	if let Some(long) = arg.get_long() {
		parts.push(format!("--{long}"));
	}
	let mut syntax = if parts.is_empty() {
		arg.get_value_names()
			.and_then(|names| names.first())
			.map_or_else(|| arg.get_id().to_string(), |name| format!("[{name}]"))
	} else {
		parts.join(", ")
	};
	let takes_value = arg.get_num_args().is_some_and(|range| range.takes_values()) && arg.get_long().is_some();
	if takes_value && let Some(name) = arg.get_value_names().and_then(|names| names.first()) {
		if arg.get_num_args().is_some_and(|range| range.min_values() == 0) {
			let _ = write!(syntax, " [<{name}>]");
		} else {
			let _ = write!(syntax, " <{name}>");
		}
	}
	syntax
}

fn options_table(out: &mut String, args: &[&clap::Arg]) {
	out.push_str("| Flag | Description | Aliases |\n| --- | --- | --- |\n");
	for arg in args {
		let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
		let aliases = arg
			.get_all_aliases()
			.unwrap_or_default()
			.iter()
			.map(|alias| format!("`--{alias}`"))
			.collect::<Vec<_>>()
			.join(", ");
		let _ = writeln!(
			out,
			"| {} | {} | {} |",
			cell(&format!("`{}`", flag_syntax(arg))),
			cell(&help),
			aliases
		);
	}
	out.push('\n');
}

fn page(command: &clap::Command) -> String {
	let mut out = String::new();
	let _ = write!(
		out,
		"---\ntitle: {}\ndescription: {}\n---\n\n",
		command.get_name(),
		serde_json::Value::String(sentence(&about(command)))
	);
	let usage = command.clone().render_usage().to_string();
	let usage = usage.strip_prefix("Usage: ").unwrap_or(&usage);
	let _ = write!(
		out,
		"```sh\n{usage}\n```\n\n{}\n\n",
		escape_mdx(&sentence(&about(command)))
	);
	let args: Vec<&clap::Arg> = command.get_arguments().filter(|arg| !arg.is_hide_set()).collect();
	let mut headings: Vec<Option<&str>> = Vec::new();
	for arg in &args {
		if !headings.contains(&arg.get_help_heading()) {
			headings.push(arg.get_help_heading());
		}
	}
	if !args.is_empty() {
		out.push_str("## Options\n\n");
	}
	for heading in headings {
		let group: Vec<&clap::Arg> = args
			.iter()
			.copied()
			.filter(|arg| arg.get_help_heading() == heading)
			.collect();
		if let Some(heading) = heading {
			let _ = write!(out, "### {heading}\n\n");
		}
		options_table(&mut out, &group);
	}
	if let Some(examples) = command.get_after_long_help() {
		let examples = examples.to_string();
		out.push_str("## Examples\n\n```sh\n");
		for line in examples.lines().filter(|line| line.starts_with("  sloptor ")) {
			let line = line.trim();
			// The comment column of the help text becomes a shell comment.
			let line = line.find("   ").map_or_else(
				|| line.to_owned(),
				|split| format!("{}  # {}", line[..split].trim_end(), line[split..].trim()),
			);
			let _ = writeln!(out, "{line}");
		}
		out.push_str("```\n");
	}
	out.trim_end().to_owned() + "\n"
}

fn index(commands: &[&clap::Command]) -> String {
	let mut out = String::from(
		"---\ntitle: Commands\ndescription: Every sloptor command, generated from the CLI.\nsidebar:\n  order: 0\n  label: \
		 Overview\n---\n\n| Command | Summary |\n| --- | --- |\n",
	);
	for command in commands {
		let _ = writeln!(
			out,
			"| [`{name}`](./{name}/) | {} |",
			cell(&about(command)),
			name = command.get_name()
		);
	}
	out
}

/// `writeCommandDocs`: writes `index.mdx` and one page per visible command into `dir`.
///
/// # Errors
///
/// The directory or a page cannot be written.
pub fn write(dir: &str) -> Result<(), String> {
	std::fs::create_dir_all(dir).map_err(|error| format!("create {dir}: {error}"))?;
	let root = cli::display_command();
	let commands: Vec<&clap::Command> = root
		.get_subcommands()
		.filter(|command| !command.is_hide_set() && command.get_name() != "help")
		.collect();
	let mut pages = vec![("index.mdx".to_owned(), index(&commands))];
	pages.extend(
		commands
			.iter()
			.map(|command| (format!("{}.mdx", command.get_name()), page(command))),
	);
	for (name, content) in pages {
		let path = std::path::Path::new(dir).join(&name);
		std::fs::write(&path, content).map_err(|error| format!("write {}: {error}", path.display()))?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::{escape_mdx, write};

	#[test]
	fn mdx_escaping_leaves_code_spans_alone() {
		assert_eq!(escape_mdx("a <b> {c}"), "a &lt;b&gt; &#123;c&#125;");
		assert_eq!(escape_mdx("`<x>` <y>"), "`<x>` &lt;y&gt;");
		assert_eq!(escape_mdx("`open <z>"), "`open &lt;z&gt;");
	}

	#[test]
	fn a_page_per_visible_command() {
		let dir = tempfile::tempdir().expect("temp dir");
		let path = dir.path().to_string_lossy().into_owned();
		write(&path).expect("write");
		let mut names: Vec<String> = std::fs::read_dir(dir.path())
			.expect("read")
			.map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
			.collect();
		names.sort();
		assert_eq!(
			names,
			[
				"build.mdx",
				"check.mdx",
				"clean.mdx",
				"completion.mdx",
				"diagnostics.mdx",
				"index.mdx",
				"lsp.mdx",
				"schema.mdx",
				"version.mdx"
			]
		);
		let build = std::fs::read_to_string(dir.path().join("build.mdx")).expect("read");
		for want in [
			"title: build",
			"## Options",
			"### Input",
			"`--optimized-loops`",
			"`--optimizedLoops`",
			"## Examples",
			"sloptor build",
		] {
			assert!(build.contains(want), "build.mdx lacks {want}:\n{build}");
		}
		assert!(!build.contains("[=<BOOL>]"));
	}
}
