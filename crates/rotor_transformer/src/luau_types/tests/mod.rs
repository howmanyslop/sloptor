//! Unit tests ported from Go's `luautypes*_test.go` and `nativetypes*_test.go`, with the
//! expected strings kept verbatim.

use std::collections::HashMap;
use std::path::Path;

use rotor_rojo::{PathTranslator, RojoResolver};
use rotor_tsrs::ast as tsast;

use crate::nodemodulespath::NodeModulesPathMapping;
use crate::sourcefile::transform_source_file;
use crate::state::RojoContext;
use crate::tests::render_list;

use super::luautypes::{
	BuiltType, EntryId, FidelityLossKind, TypeAliasEntry, TypeContext, TypePosition, luau_type_losses, type_alias_for,
	types,
};
use crate::prelude::*;
use crate::tests::{render_node, transformer_testdata, with_state};

mod alias;
mod analyze;
mod builder;
mod class;
mod emit;
mod golden;
mod native;
mod policy;
mod typefunction;

/// Go's `typeFixture`: one fixture file of `testdata/luautypes` with its top-level declarations
/// by name.
pub struct Fixture<'s, 'a> {
	pub state: &'s mut State<'a>,
	declarations: HashMap<String, P<Node>>,
}

/// Go's `loadTypeFixture`.
pub fn with_fixture<R: Send>(rel_path: &str, test: impl FnOnce(&mut Fixture<'_, '_>) -> R + Send) -> R {
	with_state(&transformer_testdata("luautypes"), rel_path, |state| {
		let mut declarations = HashMap::new();
		for &statement in state.source_file.statements().nodes() {
			if tsast::is_variable_statement(statement) {
				let list = statement.as_variable_statement().declaration_list;
				for &declaration in list.as_variable_declaration_list().declarations.nodes() {
					declarations.insert(
						declaration.as_variable_declaration().name.text().to_owned(),
						declaration,
					);
				}
			} else if (tsast::is_function_declaration(statement)
				|| tsast::is_interface_declaration(statement)
				|| tsast::is_type_alias_declaration(statement)
				|| tsast::is_enum_declaration(statement)
				|| tsast::is_class_declaration(statement))
				&& let Some(name) = statement.name()
			{
				declarations.entry(name.text().to_owned()).or_insert(statement);
			}
		}
		let mut fixture = Fixture { state, declarations };
		test(&mut fixture)
	})
}

impl Fixture<'_, '_> {
	pub fn declaration(&self, name: &str) -> P<Node> {
		*self
			.declarations
			.get(name)
			.unwrap_or_else(|| panic!("fixture has no top-level declaration {name:?}"))
	}

	/// `variableType`: a declared variable's type at a local position.
	pub fn variable_type(&mut self, name: &str) -> BuiltType {
		let declaration = self.declaration(name);
		let t = self
			.state
			.ck()
			.get_type_at_location(declaration.name().expect("a named declaration"));
		types(self.state).build(t, TypeContext::at(TypePosition::Local, Some(declaration)))
	}

	/// `parameterType`: a function parameter's type at a parameter position.
	pub fn parameter_type(&mut self, function: &str, index: usize) -> BuiltType {
		let declaration = self.declaration(function);
		let parameter = declaration.parameters()[index];
		let t = self
			.state
			.ck()
			.get_type_at_location(parameter.name().expect("a named parameter"));
		types(self.state).build(t, TypeContext::at(TypePosition::Parameter, Some(declaration)))
	}

	/// `functionType`: a declared function's signature as a function type.
	pub fn function_type(&mut self, name: &str) -> NodeId {
		let declaration = self.declaration(name);
		let signature = self.state.ck().get_signature_from_declaration(declaration);
		types(self.state).build_signature(
			signature,
			None,
			TypeContext::at(TypePosition::Nested, Some(declaration)),
		)
	}

	/// `symbol`: the symbol a top-level declaration declares.
	pub fn symbol(&mut self, name: &str) -> P<Symbol> {
		let declaration = self.declaration(name);
		self.state
			.ck()
			.get_symbol_at_location(declaration.name().expect("a named declaration"))
			.unwrap_or_else(|| panic!("no symbol for {name:?}"))
	}

	/// `alias`: registers (on first use) and returns a named type's alias entry.
	pub fn alias(&mut self, name: &str) -> EntryId {
		let symbol = self.symbol(name);
		type_alias_for(self.state, Some(symbol)).unwrap_or_else(|| panic!("{name:?} was not aliased"))
	}

	/// The entry behind `id`.
	pub fn entry(&mut self, id: EntryId) -> TypeAliasEntry {
		types(self.state).b().entry(id).clone()
	}

	/// `aliasStatement`: a named type's `type` statement, rendered.
	pub fn alias_statement(&mut self, name: &str) -> String {
		let id = self.alias(name);
		let statement = self.entry(id).statement.expect("an alias statement");
		self.text(statement).trim().to_owned()
	}

	/// `text`
	pub fn text(&self, node: NodeId) -> String {
		render_node(self.state, node)
	}

	/// The number of losses recorded so far.
	pub fn loss_count(&mut self) -> usize {
		luau_type_losses(self.state).len()
	}

	/// `distinctLosses`: the loss kinds recorded since `from`, in order of first appearance.
	pub fn distinct_losses(&mut self, from: usize) -> Vec<FidelityLossKind> {
		let mut kinds = Vec::new();
		for kind in self.losses_since(from) {
			if !kinds.contains(&kind) {
				kinds.push(kind);
			}
		}
		kinds
	}

	/// `lossesSince`
	pub fn losses_since(&mut self, from: usize) -> Vec<FidelityLossKind> {
		luau_type_losses(self.state)[from..]
			.iter()
			.map(|loss| loss.kind)
			.collect()
	}

	/// The names of the registered aliases, in registration order (Go's `aliasNames`).
	pub fn alias_names(&mut self) -> String {
		let mut builder = types(self.state);
		builder
			.aliases()
			.into_iter()
			.map(|id| builder.b().entry(id).name.clone())
			.collect::<Vec<_>>()
			.join(",")
	}
}

/// Go's `luauTypesRojoContext`: just enough Rojo data for a relative import to resolve, a
/// synthetic resolver over a never-written `out/` directory and the matching path translator.
pub fn luau_types_rojo_context(project: &Path) -> &'static RojoContext {
	let dir = project.canonicalize().expect("the project resolves");
	let dir = dir.to_string_lossy().replace('\\', "/");
	let out_dir = format!("{dir}/out");
	Box::leak(Box::new(RojoContext {
		resolver: RojoResolver::synthetic(&out_dir),
		path_translator: PathTranslator::new(&format!("{dir}/src"), &out_dir, "", false, true),
		runtime_lib_rbx_path: None,
		project_path: dir,
		pkg_rojo_resolvers: Vec::new(),
		node_modules_path_mapping: NodeModulesPathMapping::default(),
		project_reference_paths: HashMap::new(),
		node_modules_path: String::new(),
		type_roots: Vec::new(),
		use_case_sensitive_file_names: true,
	}))
}

/// A `testdata/luautypes` fixture through `TransformSourceFile` with `luauTypes` set as given,
/// rendered, and the loss kinds the file recorded by name (Go's `renderLuauTypesLosses`). No
/// diagnostics are allowed.
pub fn render_luau_types_losses(rel_path: &str, luau_types: bool) -> (String, Vec<String>) {
	let project = transformer_testdata("luautypes");
	let rojo = luau_types_rojo_context(&project);
	with_state(&project, rel_path, |state| {
		state.luau_types = luau_types;
		state.set_rojo_context(rojo, None);
		let statements = transform_source_file(state);
		let diagnostics = state.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
		let mut kinds: Vec<String> = Vec::new();
		if state.slices.luau_types.shared.is_some() {
			for loss in luau_type_losses(state) {
				let name = loss.kind.name().to_owned();
				if !kinds.contains(&name) {
					kinds.push(name);
				}
			}
		}
		(render_list(state, statements), kinds)
	})
}

/// Go's `renderLuauTypes`.
pub fn render_luau_types(rel_path: &str, luau_types: bool) -> String {
	render_luau_types_losses(rel_path, luau_types).0
}

/// Go's `luauTypesFixtures`: every `.ts` file under `testdata/luautypes/src`, relative to the
/// project and slash-separated, in walk order.
pub fn luau_types_fixtures() -> Vec<String> {
	fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
		let mut entries: Vec<_> = std::fs::read_dir(dir)
			.expect("the directory reads")
			.filter_map(Result::ok)
			.collect();
		entries.sort_by_key(std::fs::DirEntry::file_name);
		for entry in entries {
			let name = entry.file_name().to_string_lossy().into_owned();
			let path = entry.path();
			if path.is_dir() {
				walk(&path, &format!("{prefix}{name}/"), out);
			} else if path.extension().is_some_and(|extension| extension == "ts") {
				out.push(format!("{prefix}{name}"));
			}
		}
	}
	let mut fixtures = Vec::new();
	walk(&transformer_testdata("luautypes").join("src"), "src/", &mut fixtures);
	assert!(!fixtures.is_empty(), "no fixtures under testdata/luautypes/src");
	fixtures
}

/// `assertLinesInOrder`: every line of `want` appears in `got`, as a whole line, in order.
pub fn assert_lines_in_order(got: &str, want: &[&str]) {
	let lines: Vec<&str> = got.split('\n').collect();
	let mut next = 0;
	let mut failures = Vec::new();
	for &line in want {
		if let Some(index) = lines[next..].iter().position(|&candidate| candidate == line) {
			next += index + 1;
		} else if lines.contains(&line) {
			failures.push(format!("line {line:?} is out of order"));
		} else {
			failures.push(format!("line {line:?} is missing"));
		}
	}
	assert!(failures.is_empty(), "{}\nrendered:\n{got}", failures.join("\n"));
}

/// `sameLossSet`
pub fn same_loss_set(got: &[FidelityLossKind], want: &[FidelityLossKind]) -> bool {
	got.len() == want.len() && want.iter().all(|kind| got.contains(kind))
}

/// `typeCase`: one declaration and the text the builder must spell for it.
pub struct TypeCase {
	pub name: &'static str,
	pub want: &'static str,
	pub losses: &'static [FidelityLossKind],
}

/// Shorthand for a [`TypeCase`].
pub const fn case(name: &'static str, want: &'static str, losses: &'static [FidelityLossKind]) -> TypeCase {
	TypeCase { name, want, losses }
}

/// `runTypeCases`: spells every case and checks text and losses, reporting every mismatch.
pub fn run_type_cases(
	fixture: &mut Fixture<'_, '_>,
	cases: &[TypeCase],
	mut spell: impl FnMut(&mut Fixture<'_, '_>, &str) -> String,
) {
	let mut failures = Vec::new();
	for case in cases {
		let from = fixture.loss_count();
		let got = spell(fixture, case.name);
		if got != case.want {
			failures.push(format!("{}: spelled {got:?}, want {:?}", case.name, case.want));
		}
		let losses = fixture.distinct_losses(from);
		if !same_loss_set(&losses, case.losses) {
			failures.push(format!("{}: losses {losses:?}, want {:?}", case.name, case.losses));
		}
	}
	assert!(failures.is_empty(), "{}", failures.join("\n"));
}
