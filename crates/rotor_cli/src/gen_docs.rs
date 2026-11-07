//! `sloptor gen-docs <dir>`: the CLI reference as Starlight MDX pages, one per visible command
//! plus an `index.mdx` table, read from the same usage spec as `--help` so the docs cannot drift
//! from it.

use std::fmt::Write as _;

use usage_rs::help;
use usage_rs::spec::{ArgMeta, CommandMeta, FlagMeta};

use crate::cli::Cli;

/// A capital first letter and a closing period.
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

/// JSX and expression syntax escaped outside inline code spans.
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

/// Safe inside a Markdown table cell.
fn cell(text: &str) -> String {
	escape_mdx(text)
		.replace('|', "\\|")
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
}

/// One option row: a positional or a flag, its description and its hidden spellings.
struct Row {
	syntax: String,
	help: String,
	aliases: String,
}

fn options_table(out: &mut String, rows: &[Row]) {
	out.push_str("| Flag | Description | Aliases |\n| --- | --- | --- |\n");
	for row in rows {
		let _ = writeln!(
			out,
			"| {} | {} | {} |",
			cell(&format!("`{}`", row.syntax)),
			cell(&row.help),
			row.aliases
		);
	}
	out.push('\n');
}

fn arguments_table(out: &mut String, rows: &[Row]) {
	out.push_str("| Argument | Description |\n| --- | --- |\n");
	for row in rows {
		let _ = writeln!(out, "| {} | {} |", cell(&format!("`{}`", row.syntax)), cell(&row.help));
	}
	out.push('\n');
}

/// The help text with what help appends to it: the possible values and the default.
fn described(help: Option<&str>, choices: &[&str], hide_choices: bool, default: &[&str], hide_default: bool) -> String {
	let mut text = help.unwrap_or_default().to_owned();
	if !choices.is_empty() && !hide_choices {
		let _ = write!(text, " [{}]", choices.join(", "));
	}
	if !default.is_empty() && !hide_default {
		let _ = write!(text, " (default: {})", default.join(", "));
	}
	text
}

fn arg_row(meta: &ArgMeta<'_>) -> Row {
	let name = meta.arg().name;
	let mut syntax = if meta.required() {
		format!("<{name}>")
	} else {
		format!("[{name}]")
	};
	if meta.arg().var {
		syntax.push_str("...");
	}
	Row {
		syntax,
		help: described(
			meta.help(),
			meta.choices(),
			meta.hide_possible_values(),
			meta.default(),
			meta.hide_default_value(),
		),
		aliases: String::new(),
	}
}

fn flag_row(meta: &FlagMeta<'_>) -> Row {
	let flag = meta.flag();
	let mut names: Vec<String> = flag
		.shorts
		.iter()
		.filter(|short| !meta.hidden_shorts().contains(short))
		.map(|short| format!("-{}", char::from(*short)))
		.collect();
	names.extend(
		flag.longs
			.iter()
			.filter(|long| !meta.hidden_longs().contains(long))
			.map(|long| format!("--{long}")),
	);
	let mut syntax = names.join(", ");
	if let Some(negation) = flag.negate {
		let _ = write!(syntax, " / --{negation}");
	}
	if flag.takes_value {
		let name = meta.value_name().unwrap_or(flag.name);
		let _ = match (meta.value_optional(), flag.require_equals) {
			(true, true) => write!(syntax, "[=<{name}>]"),
			(true, false) => write!(syntax, " [<{name}>]"),
			(false, _) => write!(syntax, " <{name}>"),
		};
	}
	Row {
		syntax,
		help: described(
			meta.help(),
			meta.choices(),
			meta.hide_possible_values(),
			meta.default(),
			meta.hide_default_value(),
		),
		aliases: meta
			.hidden_longs()
			.iter()
			.map(|alias| format!("`--{alias}`"))
			.collect::<Vec<_>>()
			.join(", "),
	}
}

/// The flags a page lists: visible, and not the parser's own `--help`.
fn listed<'a>(flags: &'a [FlagMeta<'a>]) -> impl Iterator<Item = &'a FlagMeta<'a>> {
	flags.iter().filter(|meta| !meta.hide() && !meta.builtin())
}

fn page(root: &CommandMeta<'_>, command: &CommandMeta<'_>) -> String {
	let name = command.cmd().name;
	let about = command.about().unwrap_or_default();
	let mut out = String::new();
	let _ = write!(
		out,
		"---\ntitle: {name}\ndescription: {}\n---\n\n",
		serde_json::Value::String(sentence(about))
	);
	let _ = write!(
		out,
		"```sh\n{}\n```\n\n{}\n\n",
		help::usage_line(&["sloptor", name], command),
		escape_mdx(&sentence(about))
	);
	let args: Vec<Row> = command.args().iter().filter(|meta| !meta.hide()).map(arg_row).collect();
	let mut sections: Vec<(Option<&str>, Vec<Row>)> = Vec::new();
	for meta in listed(command.flags()) {
		let heading = meta.help_heading();
		match sections.iter_mut().find(|(seen, _)| *seen == heading) {
			Some((_, rows)) => rows.push(flag_row(meta)),
			None => sections.push((heading, vec![flag_row(meta)])),
		}
	}
	let globals: Vec<Row> = listed(root.flags())
		.filter(|meta| meta.flag().global)
		.map(flag_row)
		.collect();
	out.push_str("## Options\n\n");
	if !args.is_empty() {
		out.push_str("### Arguments\n\n");
		arguments_table(&mut out, &args);
	}
	for (heading, rows) in &sections {
		let _ = write!(out, "### {}\n\n", heading.unwrap_or("Flags"));
		options_table(&mut out, rows);
	}
	out.push_str("### Global flags\n\n");
	options_table(&mut out, &globals);
	if !command.examples().is_empty() {
		out.push_str("## Examples\n\n```sh\n");
		for example in command.examples() {
			match example.help {
				Some(help) => {
					let _ = writeln!(out, "# {help}\n{}", example.code);
				}
				None => {
					let _ = writeln!(out, "{}", example.code);
				}
			}
		}
		out.push_str("```\n");
	}
	out.trim_end().to_owned() + "\n"
}

fn index(commands: &[&CommandMeta<'_>]) -> String {
	let mut out = String::from(
		"---\ntitle: Commands\ndescription: Every sloptor command, generated from the CLI.\nsidebar:\n  order: 0\n  label: \
		 Overview\n---\n\n| Command | Summary |\n| --- | --- |\n",
	);
	for command in commands {
		let _ = writeln!(
			out,
			"| [`{name}`](./{name}/) | {} |",
			cell(command.about().unwrap_or_default()),
			name = command.cmd().name
		);
	}
	out
}

/// Writes `index.mdx` and one page per visible command into `dir`.
///
/// # Errors
///
/// The directory or a page cannot be written.
pub fn write(dir: &str) -> Result<(), String> {
	std::fs::create_dir_all(dir).map_err(|error| format!("create {dir}: {error}"))?;
	let root = Cli::spec().root;
	let commands: Vec<&CommandMeta<'_>> = root
		.subcommands()
		.iter()
		.copied()
		.filter(|command| !command.hide() && command.cmd().name != "help")
		.collect();
	let mut pages = vec![("index.mdx".to_owned(), index(&commands))];
	pages.extend(
		commands
			.iter()
			.map(|command| (format!("{}.mdx", command.cmd().name), page(root, command))),
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
			"### Arguments",
			"`--optimized-loops[=<BOOL>]`",
			"`--optimizedLoops`",
			"`--clear / --no-clear`",
			"### Global flags",
			"`--color <WHEN>`",
			"## Examples",
			"sloptor build",
		] {
			assert!(build.contains(want), "build.mdx lacks {want}:\n{build}");
		}
		for hidden in ["--no-luau", "--help"] {
			assert!(
				!build.contains(&format!("`{hidden}")),
				"build.mdx lists {hidden}:\n{build}"
			);
		}
	}
}
