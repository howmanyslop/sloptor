//! Type renderers. The slot a type is rendered into alone decides
//! parenthesization, so builders never inspect rendered text.

use crate::render::parens::{cast_needs_parentheses, cast_operand_needs_parentheses};
use crate::render::{RenderResult, Renderer, fail};
use crate::{
	CastExpression, NodeData, NodeId, SyntaxKind, TypeAccess, TypeAlias, TypeArray, TypeFunction,
	TypeFunctionDeclaration, TypeGeneric, TypeGenericPack, TypeIntersection, TypeOptional, TypePack, TypeReference,
	TypeStringSingleton, TypeTable, TypeTableIndexer, TypeTableProp, TypeTypeof, TypeUnion, TypeVariadic,
	is_valid_identifier,
};

/// The syntactic position a type is rendered into. The Luau rules it encodes:
/// `|` and `&` never mix unparenthesized, `A & B?` is a parse error, a function
/// type inside a union, intersection, or optional must be parenthesized (its
/// return type extends as far as possible), and `() -> (A, B)?` is an optional
/// function, never an optional pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSlot {
	/// Annotations, alias bodies, type arguments, fields, pack elements.
	Top,
	/// `A | _`
	UnionMember,
	/// `A & _`
	IntersectionMember,
	/// `_?`
	OptionalInner,
	/// `() -> _`
	FunctionReturn,
	/// `..._`
	VariadicInner,
}

/// The parenthesization table. Atoms (references, singletons, tables,
/// arrays, typeof, packs) never take parentheses.
///
/// ```text
/// child \ slot          Top  Union  Intersection  Optional  FunctionReturn  Variadic
/// Union / Intersection  no   yes    yes           yes       yes             yes
/// Optional              no   no     yes           yes       no              no
/// Function              no   yes    yes           yes       no              no
/// ```
const fn type_needs_parens(kind: SyntaxKind, slot: TypeSlot) -> bool {
	match kind {
		SyntaxKind::TypeUnion | SyntaxKind::TypeIntersection => !matches!(slot, TypeSlot::Top),
		SyntaxKind::TypeOptional => matches!(slot, TypeSlot::IntersectionMember | TypeSlot::OptionalInner),
		SyntaxKind::TypeFunction => {
			matches!(
				slot,
				TypeSlot::UnionMember | TypeSlot::IntersectionMember | TypeSlot::OptionalInner
			)
		}
		_ => false,
	}
}

const fn type_access_prefix(access: TypeAccess) -> &'static str {
	match access {
		TypeAccess::Read => "read ",
		TypeAccess::Write => "write ",
		TypeAccess::None => "",
	}
}

/// Escapes a string singleton or a quoted property name for a double-quoted
/// Luau literal.
fn escape_type_string(value: &str) -> String {
	let mut out = String::with_capacity(value.len());
	for c in value.chars() {
		match c {
			'\\' => out.push_str("\\\\"),
			'"' => out.push_str("\\\""),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			c if c < '\u{20}' || c == '\u{7f}' => {
				const HEX: &[u8; 16] = b"0123456789ABCDEF";
				let byte = u32::from(c);
				out.push_str("\\x");
				out.push(char::from(HEX[(byte >> 4) as usize]));
				out.push(char::from(HEX[(byte & 0xf) as usize]));
			}
			c => out.push(c),
		}
	}
	out
}

fn type_name<'n>(what: &str, name: &'n str) -> RenderResult<&'n str> {
	if !is_valid_identifier(name) {
		return fail(format!("Invalid {what}: \"{name}\""));
	}
	Ok(name)
}

impl Renderer<'_> {
	fn is_type_pack(&self, node: Option<NodeId>) -> bool {
		node.is_some_and(|node| self.ast.kind(node).is_type_pack())
	}

	/// Whether `node` lays its members out one per line. The alias renderer
	/// owns the line break before such a body.
	fn is_multiline_union(&self, node: Option<NodeId>) -> bool {
		node.is_some_and(|node| {
			matches!(self.ast.data(node), NodeData::TypeUnion(union) if union.multiline) && !self.s.single_line
		})
	}

	/// Renders a type into `slot`, parenthesized when the slot requires. The
	/// parentheses belong to the node's range, like an expression's.
	pub(crate) fn render_type(&mut self, node: Option<NodeId>, slot: TypeSlot) -> RenderResult {
		let Some(node) = node else {
			return fail("render: nil type");
		};
		if !type_needs_parens(self.ast.kind(node), slot) {
			return self.render(node);
		}
		if !self.s.positions {
			self.write("(");
			self.render_node(node)?;
			self.write(")");
			return Ok(());
		}
		let parent = self.s.current_range;
		let index = self.s.begin_range(node);
		self.s.current_range = index;
		self.write("(");
		self.render_node(node)?;
		self.write(")");
		self.s.end_range(index);
		self.s.current_range = parent;
		Ok(())
	}

	fn type_list(&mut self, types: crate::List<crate::element::TypeNode>) -> RenderResult {
		let ast = self.ast;
		for (index, value) in ast.list_iter(types).enumerate() {
			if index > 0 {
				self.write(", ");
			}
			self.render_type(Some(value), TypeSlot::Top)?;
		}
		Ok(())
	}

	pub(crate) fn render_type_reference(&mut self, node: &TypeReference) -> RenderResult {
		if !node.module.is_empty() {
			self.write(type_name("type module", &node.module)?);
			self.write(".");
		}
		if node.name == "typeof" {
			return fail("render: `typeof` is not a type name; use a TypeTypeof node");
		}
		if node.name == "nil" {
			self.write(&node.name);
		} else {
			self.write(type_name("type name", &node.name)?);
		}
		if let Some(arguments) = node.arguments
			&& self.ast.list_is_non_empty(arguments)
		{
			self.write("<");
			self.type_list(arguments)?;
			self.write(">");
		}
		Ok(())
	}

	pub(crate) fn render_type_string_singleton(&mut self, node: &TypeStringSingleton) {
		self.write("\"");
		self.write(&escape_type_string(&node.value));
		self.write("\"");
	}

	pub(crate) fn render_type_optional(&mut self, node: &TypeOptional) -> RenderResult {
		if self.is_type_pack(node.inner) {
			return fail("render: a type pack cannot be optional");
		}
		self.render_type(node.inner, TypeSlot::OptionalInner)?;
		self.write("?");
		Ok(())
	}

	pub(crate) fn render_type_union(&mut self, node: &TypeUnion) -> RenderResult {
		if self.ast.list_is_empty(node.members) {
			return fail("render: empty union type");
		}
		let multiline = node.multiline && !self.s.single_line;
		let ast = self.ast;
		for (index, member) in ast.list_iter(node.members).enumerate() {
			if multiline {
				// The first member sits where the caller left the cursor (the
				// alias renderer breaks the line and indents); the rest
				// continue at the same indent with a leading bar.
				if index > 0 {
					self.newline();
					self.write_indent();
				}
				self.write("| ");
			} else if index > 0 {
				self.write(" | ");
			}
			self.render_type(Some(member), TypeSlot::UnionMember)?;
		}
		Ok(())
	}

	pub(crate) fn render_type_intersection(&mut self, node: &TypeIntersection) -> RenderResult {
		if self.ast.list_is_empty(node.members) {
			return fail("render: empty intersection type");
		}
		let ast = self.ast;
		for (index, member) in ast.list_iter(node.members).enumerate() {
			if index > 0 {
				self.write(" & ");
			}
			self.render_type(Some(member), TypeSlot::IntersectionMember)?;
		}
		Ok(())
	}

	/// Mirrors the map renderer when multiline: one field per line with
	/// trailing commas and `}` at the enclosing indent.
	pub(crate) fn render_type_table(&mut self, node: &TypeTable) -> RenderResult {
		let ast = self.ast;
		if ast.list_is_empty(node.fields) {
			self.write("{}");
			return Ok(());
		}
		let indexers = ast
			.list_iter(node.fields)
			.filter(|&field| ast.kind(field) == SyntaxKind::TypeTableIndexer)
			.count();
		if indexers > 1 {
			return fail("render: a table type cannot have more than one indexer");
		}
		if node.multiline && !self.s.single_line {
			self.write("{");
			self.newline();
			self.s.push_indent();
			for field in ast.list_iter(node.fields) {
				self.write_indent();
				self.render(field)?;
				self.write(",");
				self.newline();
			}
			self.s.pop_indent();
			self.write_indent();
			self.write("}");
			return Ok(());
		}
		self.write("{ ");
		for (index, field) in ast.list_iter(node.fields).enumerate() {
			if index > 0 {
				self.write(", ");
			}
			self.render(field)?;
		}
		self.write(" }");
		Ok(())
	}

	pub(crate) fn render_type_table_prop(&mut self, node: &TypeTableProp) -> RenderResult {
		self.write(type_access_prefix(node.access));
		if is_valid_identifier(&node.name) {
			self.write(&node.name);
		} else {
			self.write("[\"");
			self.write(&escape_type_string(&node.name));
			self.write("\"]");
		}
		self.write(": ");
		self.render_type(Some(node.value), TypeSlot::Top)
	}

	pub(crate) fn render_type_table_indexer(&mut self, node: &TypeTableIndexer) -> RenderResult {
		self.write(type_access_prefix(node.access));
		self.write("[");
		self.render_type(Some(node.key), TypeSlot::Top)?;
		self.write("]: ");
		self.render_type(Some(node.value), TypeSlot::Top)
	}

	pub(crate) fn render_type_array(&mut self, node: &TypeArray) -> RenderResult {
		self.write("{ ");
		self.write(type_access_prefix(node.access));
		self.render_type(Some(node.element), TypeSlot::Top)?;
		self.write(" }");
		Ok(())
	}

	pub(crate) fn render_type_function(&mut self, node: &TypeFunction) -> RenderResult {
		self.type_generics(&node.generics, false)?;
		self.write("(");
		for (index, parameter) in node.parameters.iter().enumerate() {
			if index > 0 {
				self.write(", ");
			}
			if !parameter.name.is_empty() {
				self.write(type_name("parameter name", &parameter.name)?);
				self.write(": ");
			}
			self.render_type(Some(parameter.ty), TypeSlot::Top)?;
		}
		if let Some(var_args) = node.var_args {
			if !matches!(
				self.ast.kind(var_args),
				SyntaxKind::TypeVariadic | SyntaxKind::TypeGenericPack
			) {
				return fail("render: function type varargs must be `...T` or `T...`");
			}
			if !node.parameters.is_empty() {
				self.write(", ");
			}
			self.render(var_args)?;
		}
		self.write(") -> ");
		if node.ret.is_none() {
			return fail("render: function type without a return; use an empty pack for `()`");
		}
		self.render_type(node.ret, TypeSlot::FunctionReturn)
	}

	pub(crate) fn render_type_typeof(&mut self, node: &TypeTypeof) -> RenderResult {
		self.write("typeof(");
		self.render(node.expression)?;
		self.write(")");
		Ok(())
	}

	pub(crate) fn render_type_pack(&mut self, node: &TypePack) -> RenderResult {
		self.write("(");
		let ast = self.ast;
		for (index, link) in ast.list_links(node.elements).enumerate() {
			if index > 0 {
				self.write(", ");
			}
			let element = ast.link_value(link);
			if ast.kind(element) == SyntaxKind::TypePack {
				return fail("render: a type pack cannot contain a type pack");
			}
			if ast.kind(element).is_type_pack() && ast.link_next(link).is_some() {
				return fail("render: a variadic or generic pack must be the last pack element");
			}
			self.render_type(Some(element), TypeSlot::Top)?;
		}
		self.write(")");
		Ok(())
	}

	pub(crate) fn render_type_variadic(&mut self, node: &TypeVariadic) -> RenderResult {
		if self.is_type_pack(Some(node.inner)) {
			return fail("render: a variadic must wrap a type, not a pack");
		}
		self.write("...");
		self.render_type(Some(node.inner), TypeSlot::VariadicInner)
	}

	pub(crate) fn render_type_generic_pack(&mut self, node: &TypeGenericPack) -> RenderResult {
		self.write(type_name("generic pack name", &node.name)?);
		self.write("...");
		Ok(())
	}

	/// Writes `<K, V = number, U... = ...string>`, or nothing for an empty
	/// list. Type parameters precede packs, a parameter after a defaulted one
	/// needs a default too, and defaults are legal only on type aliases.
	pub(crate) fn type_generics(&mut self, generics: &[TypeGeneric], allow_defaults: bool) -> RenderResult {
		if generics.is_empty() {
			return Ok(());
		}
		self.write("<");
		let (mut seen_pack, mut seen_default) = (false, false);
		for (index, generic) in generics.iter().enumerate() {
			if index > 0 {
				self.write(", ");
			}
			if generic.is_pack {
				seen_pack = true;
			} else if seen_pack {
				return fail("render: generic type parameters must come before generic packs");
			}
			self.write(type_name("generic name", &generic.name)?);
			if generic.is_pack {
				self.write("...");
			}
			let Some(default) = generic.default else {
				if seen_default {
					return fail("render: a generic after a defaulted one needs a default");
				}
				continue;
			};
			if !allow_defaults {
				return fail("render: generic defaults are only allowed on type aliases");
			}
			if generic.is_pack != self.is_type_pack(Some(default)) {
				return fail("render: a generic pack default must be a pack, a type default a type");
			}
			seen_default = true;
			self.write(" = ");
			self.render_type(Some(default), TypeSlot::Top)?;
		}
		self.write(">");
		Ok(())
	}

	pub(crate) fn render_type_alias(&mut self, node: &TypeAlias) -> RenderResult {
		if node.body.is_none() {
			return fail(format!("render: type alias {} has no body", node.name));
		}
		if self.is_type_pack(node.body) {
			return fail("render: a type pack cannot be an alias body");
		}
		if node.export {
			self.write("export ");
		}
		self.write("type ");
		self.write(type_name("type alias name", &node.name)?);
		self.type_generics(&node.generics, true)?;
		self.write(" =");
		if self.is_multiline_union(node.body) {
			self.newline();
			self.s.push_indent();
			self.write_indent();
			self.render_type(node.body, TypeSlot::Top)?;
			self.s.pop_indent();
		} else {
			self.write(" ");
			self.render_type(node.body, TypeSlot::Top)?;
		}
		self.newline();
		Ok(())
	}

	pub(crate) fn render_type_function_declaration(
		&mut self,
		node: NodeId,
		declaration: &TypeFunctionDeclaration,
	) -> RenderResult {
		if declaration.export {
			self.write("export ");
		}
		self.write("type function ");
		self.write(type_name("type function name", &declaration.name)?);
		self.write("(");
		self.parameters(node)?;
		self.write(")");
		self.newline();
		self.block(declaration.statements)?;
		self.write_indent();
		self.closing("end");
		self.newline();
		Ok(())
	}

	pub(crate) fn render_cast_expression(&mut self, node: NodeId, cast: &CastExpression) -> RenderResult {
		let parens = cast_needs_parentheses(self.ast, node);
		if parens {
			self.write("(");
		}
		let operand = cast_operand_needs_parentheses(self.ast, cast.expression);
		if operand {
			self.write("(");
		}
		self.render(cast.expression)?;
		if operand {
			self.write(")");
		}
		self.write(" ");
		self.operator("::");
		self.write(" ");
		self.render_type(Some(cast.ty), TypeSlot::Top)?;
		if parens {
			self.write(")");
		}
		Ok(())
	}
}
