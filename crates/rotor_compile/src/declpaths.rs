//! `declpaths.go`.
//!
//! The native port of the sidecar's `transformPaths`. Every module specifier a `.d.ts` carries is
//! rewritten from its `paths`/`baseUrl` spelling to a path relative to the emitting file, because
//! the Luau runtime resolves neither. The rewrite is applied to the emitted text, driven by a
//! reparse of that text.
//!
//! As in Go, an unresolved specifier falls back to a real resolver pass, nested `import()` types
//! are visited, and `rootDirs` applies unconditionally (inherited from rbxtsc).

use rotor_tsrs::ast::{self, Kind, Node, SourceFile, SourceFileParseOptions};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::module::{ResolutionHost, ResolvedModule, mode_aware_cache_get};
use rotor_tsrs::tscore::{JsxEmit, ResolutionMode, ScriptKind};
use rotor_tsrs::vfs::FS;
use rotor_tsrs::{CompilerOptions, P, tspath};

/// `textEdit`: a half-open byte range of the emitted text and its replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextEdit {
	pub(crate) start: usize,
	pub(crate) end: usize,
	pub(crate) text: String,
}

/// `nonOverlappingTextEdits`: sorted by start (stably), dropping any that starts inside the
/// previous one. The splice and the map fixup consume the same list.
pub(crate) fn non_overlapping_text_edits(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
	if edits.len() < 2 {
		return edits;
	}
	edits.sort_by_key(|edit| edit.start);
	let mut kept: Vec<TextEdit> = Vec::with_capacity(edits.len());
	for edit in edits {
		if kept.last().is_some_and(|last| edit.start < last.end) {
			continue;
		}
		kept.push(edit);
	}
	kept
}

/// `applyTextEdits`: splices sorted, non-overlapping edits into `text`.
pub(crate) fn apply_text_edits(text: &str, edits: &[TextEdit]) -> String {
	if edits.is_empty() {
		return text.to_owned();
	}
	let mut builder = String::with_capacity(text.len());
	let mut cursor = 0;
	for edit in edits {
		if edit.start < cursor {
			continue;
		}
		builder.push_str(&text[cursor..edit.start]);
		builder.push_str(&edit.text);
		cursor = edit.end;
	}
	builder.push_str(&text[cursor..]);
	builder
}

/// The program host's file system, as a module resolution host.
struct HostResolution {
	program: &'static Program,
}

impl ResolutionHost for HostResolution {
	fn fs(&self) -> &dyn FS {
		self.program.host().fs()
	}

	fn get_current_directory(&self) -> &str {
		self.program.host().get_current_directory()
	}
}

/// `declarationPathRewriter`
pub(crate) struct DeclarationPathRewriter {
	program: &'static Program,
	options: P<CompilerOptions>,
	implicit_extensions: Vec<&'static str>,
	root_dirs: Vec<String>,
	compare: tspath::ComparePathsOptions,
	current_directory: String,
	use_case_sensitive: bool,
}

impl DeclarationPathRewriter {
	/// `newDeclarationPathRewriter`: `None` for a project whose declarations can carry no alias
	/// spelling (no `baseUrl`, no `paths`).
	pub(crate) fn new(program: &'static Program) -> Option<Self> {
		let options = program.options();
		if options.base_url.is_empty() && options.paths.as_ref().is_none_or(|paths| paths.is_empty()) {
			return None;
		}
		let mut extensions = vec![tspath::EXTENSION_TS, tspath::EXTENSION_DTS];
		let allows_js = options.allow_js.is_true();
		let allows_jsx = options.jsx != JsxEmit::NONE;
		if allows_js {
			extensions.push(tspath::EXTENSION_JS);
		}
		if allows_jsx {
			extensions.push(tspath::EXTENSION_TSX);
		}
		if allows_js && allows_jsx {
			extensions.push(tspath::EXTENSION_JSX);
		}
		if options.resolve_json_module.is_true() {
			extensions.push(tspath::EXTENSION_JSON);
		}
		let use_case_sensitive = program.host().fs().use_case_sensitive_file_names();
		let current_directory = program.host().get_current_directory().to_owned();
		// Upstream filters rootDirs with path.isAbsolute; a relative entry is dropped.
		let root_dirs = options
			.root_dirs
			.iter()
			.flatten()
			.filter(|dir| tspath::is_rooted_disk_path(&tspath::normalize_slashes(dir)))
			.map(|dir| tspath::normalize_path(dir))
			.collect();
		Some(Self {
			program,
			options,
			implicit_extensions: extensions,
			root_dirs,
			compare: tspath::ComparePathsOptions {
				use_case_sensitive_file_names: use_case_sensitive,
				current_directory: String::new(),
			},
			current_directory,
			use_case_sensitive,
		})
	}

	/// `specifierEdits`: the replacements every rewritable module specifier of `decl_text`
	/// needs. Specifiers resolve relative to `original_file`, the source the declaration came
	/// from.
	pub(crate) fn specifier_edits(
		&self,
		original_file: P<SourceFile>,
		decl_file_name: &str,
		decl_text: &str,
	) -> Vec<TextEdit> {
		if !decl_text.contains(['"', '\'']) {
			return Vec::new();
		}
		let parsed = rotor_tsrs::parser::parse_source_file(
			SourceFileParseOptions {
				file_name: decl_file_name.to_owned(),
				path: tspath::to_path(decl_file_name, &self.current_directory, self.use_case_sensitive),
				..SourceFileParseOptions::default()
			},
			decl_text,
			ScriptKind::TS,
		);
		let resolutions = self.program.get_resolved_modules().get(original_file.path());
		// The reparse carries no resolution metadata; the mode is the original file's default.
		let mode = self.program.get_default_resolution_mode_for_file(original_file);
		let file_name = tspath::normalize_path(original_file.file_name());
		let file_directory = tspath::get_directory_path(&file_name);

		let mut edits = Vec::new();
		let mut record = |literal: Option<P<Node>>| {
			let Some(literal) = literal.filter(|&literal| ast::is_string_literal(literal)) else {
				return;
			};
			let resolved = resolutions.and_then(|resolutions| {
				let found = |mode| {
					mode_aware_cache_get(resolutions, literal.text(), mode)
						.copied()
						.filter(|resolved: &P<ResolvedModule>| resolved.is_resolved())
				};
				found(mode).or_else(|| {
					(mode != ResolutionMode::NONE)
						.then(|| found(ResolutionMode::NONE))
						.flatten()
				})
			});
			let Some(replacement) = self.resolve_specifier(resolved, mode, &file_name, &file_directory, literal.text())
			else {
				return;
			};
			let (Some(pos), Some(end)) = (literal.pos().source_offset(), literal.end().source_offset()) else {
				return;
			};
			let start = rotor_tsrs::scanner::skip_trivia(decl_text, pos) as usize;
			let end = end as usize;
			if start >= end || !matches!(decl_text.as_bytes()[start], b'"' | b'\'') {
				return;
			}
			edits.push(TextEdit {
				start,
				end,
				text: quote_specifier(&replacement),
			});
		};
		parsed.as_node().for_each_child(&mut |child| visit(child, &mut record));
		edits
	}

	/// `resolveFresh`: a resolver pass for specifiers the program never resolved, such as the
	/// import types the declaration emitter synthesizes for inferred types.
	fn resolve_fresh(&self, containing_file: &str, specifier: &str, mode: ResolutionMode) -> Option<P<ResolvedModule>> {
		let host = HostResolution { program: self.program };
		let resolver = rotor_tsrs::module::new_resolver(rotor_tsrs::module::ResolverOptions::new(&host, self.options));
		resolver
			.resolve_module_name(specifier, containing_file, mode, None)
			.ok()
			.map(|(resolved, _)| resolved)
	}

	/// `resolveSpecifier`: the relative spelling for `specifier`, or `None` to leave it alone (an
	/// external-library import, or anything that does not resolve).
	fn resolve_specifier(
		&self,
		resolved: Option<P<ResolvedModule>>,
		mode: ResolutionMode,
		containing_file: &str,
		file_directory: &str,
		specifier: &str,
	) -> Option<String> {
		let resolved = match resolved {
			Some(resolved) => resolved,
			None => self.resolve_fresh(containing_file, specifier, mode)?,
		};
		if !resolved.is_resolved() || resolved.is_external_library_import {
			return None;
		}

		let resolved_file_name = tspath::normalize_path(resolved.resolved_file_name);
		let mut directory = file_directory.to_owned();
		let mut module_directory = tspath::get_directory_path(&resolved_file_name);
		if !self.root_dirs.is_empty() {
			let file_root = self.longest_containing_root(containing_file);
			let module_root = self.longest_containing_root(&resolved_file_name);
			if !file_root.is_empty() && !module_root.is_empty() {
				directory = self.relative_from(file_root, &directory);
				module_directory = self.relative_from(module_root, &module_directory);
			}
		}
		let relative = self.relative_from(&directory, &module_directory);
		let mut resolved_path = slash_join(&relative, &tspath::get_base_file_name(&resolved_file_name));
		if let Some(extension) = self
			.implicit_extensions
			.iter()
			.find(|&&extension| extension == resolved.extension)
			&& let Some(stripped) = resolved_path.strip_suffix(extension)
		{
			resolved_path = stripped.to_owned();
		}
		if resolved_path.is_empty() {
			return None;
		}
		if !resolved_path.starts_with('.') {
			resolved_path = format!("./{resolved_path}");
		}
		Some(resolved_path)
	}

	/// `relativeFrom`: `path.relative` with the platform's case rule.
	fn relative_from(&self, from: &str, to: &str) -> String {
		if (tspath::get_root_length(from) > 0) != (tspath::get_root_length(to) > 0) {
			return to.to_owned();
		}
		tspath::get_relative_path_from_directory(from, to, &self.compare)
	}

	/// `longestContainingRoot`: the longest `rootDirs` entry containing `candidate`.
	fn longest_containing_root(&self, candidate: &str) -> &str {
		let mut best = "";
		for root in &self.root_dirs {
			if tspath::contains_path(root, candidate, &self.compare) && root.len() > best.len() {
				best = root;
			}
		}
		best
	}
}

/// The specifier visitor of `specifierEdits`: every module specifier in the tree, nested `import()`
/// types included.
fn visit(node: P<Node>, record: &mut dyn FnMut(Option<P<Node>>)) -> bool {
	match node.kind() {
		Kind::ImportDeclaration | Kind::ExportDeclaration => record(node.module_specifier()),
		Kind::ExternalModuleReference => record(node.expression()),
		Kind::ImportType => {
			let argument = node.as_import_type_node().argument;
			if argument.kind() == Kind::LiteralType {
				record(Some(argument.as_literal_type_node().literal));
			}
		}
		Kind::CallExpression => {
			let call = node.as_call_expression();
			let callee = call.expression;
			let arguments = call.arguments.nodes();
			if arguments.len() == 1 {
				let is_require = ast::is_identifier(callee) && callee.text() == "require";
				let is_import = callee.kind() == Kind::ImportKeyword;
				if is_require || is_import {
					record(Some(arguments[0]));
				}
			}
		}
		_ => {}
	}
	node.for_each_child(&mut |child| visit(child, record));
	false
}

/// Go's `path.Join(a, b)`: slash-separated and cleaned.
fn slash_join(directory: &str, name: &str) -> String {
	let joined = if directory.is_empty() {
		name.to_owned()
	} else {
		format!("{directory}/{name}")
	};
	clean_slash_path(&joined)
}

/// Go's `path.Clean`.
fn clean_slash_path(path: &str) -> String {
	if path.is_empty() {
		return ".".to_owned();
	}
	let rooted = path.starts_with('/');
	let mut parts: Vec<&str> = Vec::new();
	for part in path.split('/') {
		match part {
			"" | "." => {}
			".." => {
				if parts.last().is_some_and(|last| *last != "..") {
					parts.pop();
				} else if !rooted {
					parts.push("..");
				}
			}
			part => parts.push(part),
		}
	}
	let joined = parts.join("/");
	match (rooted, joined.is_empty()) {
		(true, _) => format!("/{joined}"),
		(false, true) => ".".to_owned(),
		(false, false) => joined,
	}
}

/// `quoteSpecifier`: a double-quoted TypeScript string literal, escaping `"` and `\`.
pub(crate) fn quote_specifier(specifier: &str) -> String {
	let mut builder = String::with_capacity(specifier.len() + 4);
	builder.push('"');
	for character in specifier.chars() {
		if matches!(character, '"' | '\\') {
			builder.push('\\');
		}
		builder.push(character);
	}
	builder.push('"');
	builder
}
