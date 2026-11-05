//! `transform_macro_types.go`: macro kinds, runtime, results and errors, and macro metadata.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node, Symbol, SymbolFlags};
use rotor_tsrs::checker::{LiteralValue, Type, TypeId};

use crate::transform_state::TransformState;
use crate::{Error, ErrorDetail, ErrorKind};

/// `macroKind`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum MacroKind {
	#[default]
	Generic,
	Caller,
	Many,
	Literal,
	Intrinsic,
}

/// `literalValue`
#[derive(Clone, Debug, Default)]
pub(crate) struct LiteralMacroValue {
	pub(crate) text: String,
	pub(crate) number: String,
	pub(crate) boolean: bool,
	pub(crate) kind: Option<Kind>,
}

/// `userMacro`
#[derive(Clone, Debug, Default)]
pub(crate) struct UserMacro {
	pub(crate) kind: MacroKind,
	pub(crate) metadata: String,
	pub(crate) order: Option<TypeId>,
	pub(crate) target: Option<P<Type>>,
	pub(crate) items: Option<Vec<Self>>,
	pub(crate) properties: Option<Vec<MacroProperty>>,
	pub(crate) literal: LiteralMacroValue,
	pub(crate) intrinsic: String,
	pub(crate) inputs: Vec<P<Type>>,
}

/// `macroProperty`
#[derive(Clone, Debug)]
pub(crate) struct MacroProperty {
	pub(crate) name: String,
	pub(crate) macro_: UserMacro,
}

/// `GuardBuildResult`: a guard expression and the statements to emit before the statement
/// holding it.
#[derive(Clone, Debug, Default)]
pub struct GuardBuildResult {
	/// The guard.
	pub expression: Option<P<Node>>,
	/// Hoisted statements.
	pub statements: Vec<P<Node>>,
}

/// `GuardBuilder`
pub type GuardBuilder = fn(&mut TransformState<'_>, P<Node>, P<Type>) -> Result<GuardBuildResult, Error>;

/// `MacroRuntime.RandomIndex`
pub type RandomIndex = Rc<dyn Fn(usize) -> Result<usize, Error>>;

/// `MacroRuntime`: the macros' random inputs and guard builder, replaceable for tests.
#[derive(Clone, Default)]
pub struct MacroRuntime {
	/// `UUID`
	pub uuid: Option<Rc<dyn Fn() -> Result<String, Error>>>,
	/// `RandomIndex`: an index in `[0, upper_bound)`.
	pub random_index: Option<RandomIndex>,
	/// `BuildGuard`
	pub build_guard: Option<GuardBuilder>,
}

impl std::fmt::Debug for MacroRuntime {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("MacroRuntime").finish_non_exhaustive()
	}
}

/// `MacroTransformResult`
#[derive(Clone, Debug, Default)]
pub(crate) struct MacroTransformResult {
	pub(crate) expression: Option<P<Node>>,
	pub(crate) prerequisites: Vec<P<Node>>,
	pub(crate) imports: Vec<MacroImport>,
}

/// `MacroImport`
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MacroImport {
	/// The module.
	pub module: String,
	/// The exported name.
	pub export: String,
	/// The local name.
	pub local: String,
}

/// `MacroError`
#[derive(Clone, Debug)]
pub struct MacroError {
	/// The node.
	pub node: Option<P<Node>>,
	/// The message.
	pub message: String,
	/// The related information.
	pub related_information: Vec<MacroRelatedInformation>,
}

/// `MacroRelatedInformation`
#[derive(Clone, Debug)]
pub struct MacroRelatedInformation {
	/// The node.
	pub node: Option<P<Node>>,
	/// The message.
	pub message: String,
}

/// A `*MacroError` with `cause`: `errors.Is` matches `ErrMacroDiagnostic` and the cause.
pub(crate) fn macro_error(
	node: Option<P<Node>>,
	message: String,
	cause: ErrorKind,
	related: Vec<MacroRelatedInformation>,
) -> Error {
	Error::with_detail(
		message.clone(),
		ErrorDetail::Macro(MacroError {
			node,
			message,
			related_information: related,
		}),
	)
	.also(ErrorKind::MacroDiagnostic)
	.also(cause)
}

/// `invalidMacro`
pub(crate) fn invalid_macro(node: impl Into<Option<P<Node>>>, message: impl Into<String>) -> Error {
	macro_error(node.into(), message.into(), ErrorKind::InvalidMacro, Vec::new())
}

/// `invalidMacroWithRelated`
pub(crate) fn invalid_macro_with_related(node: P<Node>, message: &str, related: Vec<MacroRelatedInformation>) -> Error {
	macro_error(Some(node), message.to_owned(), ErrorKind::InvalidMacro, related)
}

/// `macroMetadata`
#[derive(Clone, Debug, Default)]
pub(crate) struct MacroMetadata {
	pub(crate) flags: HashSet<String>,
	pub(crate) symbols: HashMap<String, Vec<P<Symbol>>>,
}

impl MacroMetadata {
	pub(crate) fn requested(&self, name: &str) -> bool {
		if self.flags.contains(&format!("~{name}")) {
			return false;
		}
		self.flags.contains(name) || self.flags.contains("*")
	}

	pub(crate) fn symbols(&self, key: &str) -> &[P<Symbol>] {
		self.symbols.get(key).map_or(&[], Vec::as_slice)
	}
}

/// `readMacroMetadata`: a declaration's `@metadata` flags and `{@link symbol key}` symbols.
pub(crate) fn read_macro_metadata(state: &mut TransformState<'_>, declaration: Option<P<Node>>) -> MacroMetadata {
	let mut metadata = MacroMetadata::default();
	let Some(declaration) = declaration else {
		return metadata;
	};
	for jsdoc in declaration.jsdoc(None) {
		for tag in jsdoc_tags(*jsdoc) {
			if tag.tag_name().text() != "metadata" {
				continue;
			}
			for comment in tag.comments() {
				if rotor_tsrs::ast::is_jsdoc_link_like(*comment) {
					read_metadata_link(state, &mut metadata, *comment);
					continue;
				}
				read_metadata_flags(&mut metadata, &rotor_tsrs::scanner::get_text_of_node(*comment));
			}
		}
	}
	metadata
}

/// A `JSDoc` node's tags (`JSDoc.Tags.Nodes`), empty when it has none.
pub(crate) fn jsdoc_tags(jsdoc: P<Node>) -> &'static [P<Node>] {
	jsdoc.as_jsdoc().tags.map_or(&[], |tags| tags.nodes())
}

fn read_metadata_flags(metadata: &mut MacroMetadata, text: &str) {
	for field in text.split_whitespace() {
		let field = field.trim_matches(|c| matches!(c, '{' | '}' | '@' | '*' | '/' | ','));
		if !field.is_empty() {
			metadata.flags.insert(field.to_owned());
		}
	}
}

/// `metadataLinkKey`: `([A-Za-z0-9_-]+)\s*\}?$`, the last word before an optional `}`.
fn metadata_link_key(text: &str) -> Option<&str> {
	let trimmed = text.strip_suffix('}').unwrap_or(text).trim_end();
	let start = trimmed
		.char_indices()
		.rev()
		.take_while(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
		.last()
		.map(|(index, _)| index)?;
	Some(&trimmed[start..])
}

fn read_metadata_link(state: &mut TransformState<'_>, metadata: &mut MacroMetadata, link: P<Node>) {
	let text = rotor_tsrs::scanner::get_text_of_node(link);
	let Some(key) = metadata_link_key(&text) else {
		return;
	};
	let Some(name) = link.name() else {
		return;
	};
	let Some(mut symbol) = state.ck().get_symbol_at_location(name) else {
		return;
	};
	if symbol.flags().intersects(SymbolFlags::Alias) {
		symbol = state.ck().get_aliased_symbol(symbol);
	}
	metadata.symbols.entry(key.to_owned()).or_default().push(symbol);
}

/// `literalExpression`
pub(crate) fn literal_expression(factory: &rotor_tsrs::ast::NodeFactory, value: &LiteralMacroValue) -> P<Node> {
	match value.kind {
		Some(Kind::StringLiteral) => factory.new_string_literal(alloc(&value.text), rotor_tsrs::ast::TokenFlags::None),
		Some(Kind::NumericLiteral) => {
			if let Some(rest) = value.number.strip_prefix('-') {
				return factory.new_prefix_unary_expression(
					Kind::MinusToken,
					factory.new_numeric_literal(alloc(rest), rotor_tsrs::ast::TokenFlags::None),
				);
			}
			factory.new_numeric_literal(alloc(&value.number), rotor_tsrs::ast::TokenFlags::None)
		}
		Some(Kind::TrueKeyword) => factory.new_token(if value.boolean {
			Kind::TrueKeyword
		} else {
			Kind::FalseKeyword
		}),
		_ => factory.new_identifier("undefined"),
	}
}

/// `asNever`: `expression as never`.
pub(crate) fn as_never(factory: &rotor_tsrs::ast::NodeFactory, expression: P<Node>) -> P<Node> {
	factory.new_as_expression(expression, factory.new_keyword_type_node(Kind::NeverKeyword))
}

/// `isUndefinedExpression`
pub(crate) fn is_undefined_expression(node: P<Node>) -> bool {
	rotor_tsrs::ast::is_identifier(node) && node.text() == "undefined"
}

/// `stringLiteralValue`: a string literal type's value, or `""`.
pub(crate) fn string_literal_value(target: P<Type>) -> String {
	match target.as_literal_type().value() {
		Some(LiteralValue::String(text)) => text.to_owned(),
		_ => String::new(),
	}
}

/// Interns text for a node.
pub(crate) fn alloc(text: &str) -> &'static str {
	rotor_tsrs::tscore::alloc_str(text)
}

/// `buildCallerMacro`: the call site's line, character, width, text or a UUID.
pub(crate) fn build_caller_macro(
	state: &TransformState<'_>,
	trace: P<Node>,
	user_macro: &UserMacro,
	runtime: &MacroRuntime,
) -> Result<P<Node>, Error> {
	let Some(source_file) = rotor_tsrs::ast::get_source_file_of_node(trace) else {
		return Ok(state.factory.new_identifier("undefined"));
	};
	let pos = trace.pos().source_offset().unwrap_or(0);
	let start = rotor_tsrs::scanner::skip_trivia(source_file.text(), pos);
	let (line, character) = rotor_tsrs::scanner::get_ecma_line_and_utf16_character_of_position(
		source_file.as_node().as_source_file(),
		start,
	);
	let factory = &state.factory;
	let flags = rotor_tsrs::ast::TokenFlags::None;
	match user_macro.metadata.as_str() {
		"line" => Ok(factory.new_numeric_literal(alloc(&(line + 1).to_string()), flags)),
		"character" => Ok(factory.new_numeric_literal(alloc(&(character + 1).to_string()), flags)),
		"width" => {
			let end = trace.end().source_offset().unwrap_or(0);
			Ok(factory.new_numeric_literal(alloc(&(i64::from(end) - i64::from(start)).to_string()), flags))
		}
		"text" => Ok(factory.new_string_literal(alloc(&rotor_tsrs::scanner::get_text_of_node(trace)), flags)),
		"uuid" => {
			let Some(uuid) = &runtime.uuid else {
				return Err(invalid_macro(trace, "Flamework UUID runtime is not configured"));
			};
			let uuid = uuid().map_err(|error| error.wrap("generate Flamework caller UUID"))?;
			Ok(factory.new_string_literal(alloc(&uuid), flags))
		}
		_ => Ok(factory.new_identifier("undefined")),
	}
}

/// `generateUniversalTypeNode`: the type as a fully qualified type node. Only tests call it, as in
/// Go.
#[cfg(test)]
pub(crate) fn generate_universal_type_node(
	state: &mut TransformState<'_>,
	location: P<Node>,
	target: P<Type>,
) -> Option<(P<Node>, Vec<P<Node>>)> {
	use rotor_tsrs::checker::NodeBuilderFlags;
	let flags = NodeBuilderFlags::UseFullyQualifiedType | NodeBuilderFlags::WriteClassExpressionAsTypeLiteral;
	let type_node = state.ck().type_to_type_node(target, Some(location), flags)?;
	Some((type_node, Vec::new()))
}
