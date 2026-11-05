//! `macro_inputs.go`: the `$env` names and `$file` contents the sources read, which the
//! incremental salt hashes and watch mode watches.

use std::collections::{BTreeMap, BTreeSet};

use rotor_config::dotenv::Env;
use rotor_support::gopath;
use rotor_transformer::FileResolver as _;
use rotor_tsrs::ast::{self, Kind, Node, SourceFile, SourceFileParseOptions};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::{P, tspath};

use crate::filemacro::FileResolver;
use crate::incremental_paths::relative_incremental_path;
use crate::package_layout::hex_sha256;
use crate::project::project_source_files;

/// `macroInputs`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MacroInputs {
	pub(crate) files: BTreeSet<String>,
	pub(crate) values: BTreeMap<String, String>,
}

/// `collectMacroInputs`
pub(crate) fn collect_macro_inputs(dir: &str, sources: &[P<SourceFile>]) -> MacroInputs {
	let mut inputs = MacroInputs::default();
	let env = Env::load(dir);
	let resolver = FileResolver::new(dir);
	for &source in sources {
		let text = source.text();
		if source.is_declaration_file() || (!text.contains("$env") && !text.contains("$file")) {
			continue;
		}
		let mut uses_env = false;
		let record_env = |values: &mut BTreeMap<String, String>, key: &str| {
			let value = env
				.lookup(key)
				.map_or_else(|| "unset".to_owned(), |value| format!("set:{value}"));
			values.insert(format!("env:{key}"), value);
		};
		let mut visit_node = |node: P<Node>, inputs: &mut MacroInputs| match node.kind() {
			Kind::CallExpression => {
				let args = node.arguments();
				let Some(callee) = node.expression().map(ast::skip_downwards) else {
					return;
				};
				if !ast::is_identifier(callee) || args.is_empty() {
					return;
				}
				let argument = ast::skip_downwards(args[0]);
				if !ast::is_string_literal_like(argument) {
					return;
				}
				match callee.text() {
					"$env" => {
						uses_env = true;
						record_env(&mut inputs.values, argument.text());
					}
					"$file" => {
						if let Some(path) = resolver.resolve_path_public(source.file_name(), argument.text()) {
							inputs.files.insert(path.clone());
							let value = resolver
								.read(source.file_name(), argument.text())
								.map_or_else(|_| "unreadable".to_owned(), |data| hex_sha256(&data));
							inputs
								.values
								.insert(format!("file:{}", relative_incremental_path(dir, &path)), value);
						}
					}
					_ => {}
				}
			}
			Kind::PropertyAccessExpression | Kind::ElementAccessExpression => {
				let Some(base) = node.expression().map(ast::skip_downwards) else {
					return;
				};
				if !ast::is_identifier(base) || base.text() != "$env" {
					return;
				}
				uses_env = true;
				if node.kind() == Kind::PropertyAccessExpression {
					if let Some(name) = node.name() {
						record_env(&mut inputs.values, name.text());
					}
				} else {
					let argument = ast::skip_downwards(node.as_element_access_expression().argument_expression);
					if ast::is_string_literal_like(argument) {
						record_env(&mut inputs.values, argument.text());
					}
				}
			}
			_ => {}
		};
		source
			.as_node()
			.for_each_child(&mut |child| walk(child, &mut |node| visit_node(node, &mut inputs)));
		if uses_env {
			inputs.files.extend(env.files());
		}
	}
	inputs
}

/// Visits `node` and then its descendants.
fn walk(node: P<Node>, visit: &mut dyn FnMut(P<Node>)) -> bool {
	visit(node);
	node.for_each_child(&mut |child| walk(child, visit));
	false
}

/// `macroInputValues`: the values, read from the tsconfig's directory.
pub(crate) fn macro_input_values(dir: &str, program: &Program) -> BTreeMap<String, String> {
	let options = program.options();
	let dir = if options.config_file_path.is_empty() {
		dir.to_owned()
	} else {
		gopath::dir(&gopath::from_slash(&options.config_file_path))
	};
	collect_macro_inputs(&dir, &project_source_files(program)).values
}

/// `MacroWatchFiles`: the files the macros of `config_path`'s sources read.
#[must_use]
pub fn macro_watch_files(config_path: &str) -> Vec<String> {
	// The config and the sources are read only for the paths; in a long-lived session they go to
	// a region freed on return, not to the thread's arena, which keeps them for good.
	let region = rotor_tsrs::project::long_lived().then(|| rotor_tsrs::tscore::arena::Region::new_scratch(1 << 16));
	let _region = region.as_ref().map(rotor_tsrs::tscore::arena::Region::enter);
	let Ok(config) = crate::project_config::parse_project_config(config_path) else {
		return Vec::new();
	};
	let mut sources = Vec::new();
	for name in config.file_names() {
		if tspath::is_declaration_file_name(name) {
			continue;
		}
		let Ok(text) = std::fs::read_to_string(gopath::from_slash(name).as_ref()) else {
			continue;
		};
		if !text.contains("$env") && !text.contains("$file") {
			continue;
		}
		sources.push(rotor_tsrs::parser::parse_source_file(
			SourceFileParseOptions {
				file_name: name.clone(),
				path: tspath::to_path(
					name,
					config.get_current_directory(),
					config.use_case_sensitive_file_names(),
				),
				..SourceFileParseOptions::default()
			},
			&text,
			rotor_tsrs::tscore::get_script_kind_from_file_name(name),
		));
	}
	macro_input_files(config.get_current_directory(), &sources)
}

/// `MacroInputFiles`: the files, sorted.
#[must_use]
pub fn macro_input_files(dir: &str, sources: &[P<SourceFile>]) -> Vec<String> {
	collect_macro_inputs(dir, sources).files.into_iter().collect()
}
