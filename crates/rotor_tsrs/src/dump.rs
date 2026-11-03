//! The Go-vs-Rust differential: one canonical line per expression and declaration node.
//!
//! `tools/tsrs-oracle` writes the same lines from rotor's Go checker, so the two dumps of a project
//! must agree line for line. A line is tab-separated:
//!
//! ```text
//! file:pos:end:Kind  t=  o=  s=  sym=  tsym=  ap=  cs=  ks=  d=  p=  x=  w=  fds=  fcs=
//! ```
//!
//! - the key: the file relative to the project, the node's span, and its kind;
//! - `t` and `o`: the type flags and object flags of `getTypeAtLocation`, in hex; `o` keeps only
//!   the flags that describe the type ([`object_flags`]), in typescript-go's layout;
//! - `s`: `typeToString`, with `\`, newlines and tabs escaped;
//! - `sym` and `tsym`: the symbol at the location and the type's symbol, as
//!   `name,flags,declarations` or `-`; internal names start with `#`;
//! - `ap`: the property count of the apparent type; `cs` and `ks`: the call and construct
//!   signature counts;
//! - `d` and `p`: masks of [`is_definitely_type`] and [`is_possibly_type`] for each of
//!   [`PREDICATES`], bit `i` for `PREDICATES[i]`;
//! - `x`: bit 0 `Object | NonPrimitive` definitely, bit 1 `String | Number` definitely, bit 2
//!   [`is_shared_table_type`];
//! - `w`: the leaves [`walk_types`] visits; `fds` and `fcs`: [`get_first_defined_symbol`] and
//!   [`get_first_construct_symbol`].
//!
//! A node whose queries panic gets `panic` in place of its fields. The project directory is
//! written as `<project>` everywhere but the key. The program is checked
//! first, as rotor's precheck pass does, and the predicates run in a second walk after every type
//! the first one prints exists.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use regex::Regex;

use tsrs_ast::{INTERNAL_SYMBOL_NAME_PREFIX, Node, SourceFile, Symbol, is_declaration, is_expression};
use tsrs_core::jsnum::Number;
use tsrs_core::tspath::{ComparePathsOptions, get_relative_path_from_directory};
use tsrs_core::{Context, P};

use crate::checker::{ObjectFlags, SignatureKind, Type, TypeChecker, TypeFlags};
use crate::project::{Project, for_each_checker_group};
use crate::types::{
	AmbientSymbols, TypeCheck, TypeState, get_first_construct_symbol, get_first_defined_symbol, is_definitely_type,
	is_possibly_type, is_shared_table_type, walk_types,
};

/// The predicates the `d` and `p` masks record, in bit order.
pub const PREDICATES: [TypeCheck; 21] = [
	TypeCheck::Any,
	TypeCheck::Boolean,
	TypeCheck::BooleanLiteral(false),
	TypeCheck::BooleanLiteral(true),
	TypeCheck::Number,
	TypeCheck::NumberLiteral(Number(0.0)),
	TypeCheck::NaN,
	TypeCheck::String,
	TypeCheck::Object,
	TypeCheck::NonPrimitive,
	TypeCheck::Undefined,
	TypeCheck::EmptyString,
	TypeCheck::Array,
	TypeCheck::LuaTuple,
	TypeCheck::Set,
	TypeCheck::Map,
	TypeCheck::Generator,
	TypeCheck::IterableFunction,
	TypeCheck::Iterable,
	TypeCheck::IterableFunctionLuaTuple,
	TypeCheck::Roblox,
];

fn walk(node: P<Node>, visit: &mut dyn FnMut(P<Node>)) {
	visit(node);
	node.for_each_child(&mut |child| {
		walk(child, &mut *visit);
		false
	});
}

fn dumped(node: P<Node>) -> bool {
	is_expression(node) || is_declaration(node)
}

/// Keeps a field on one line and free of tabs.
fn escape(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for character in text.chars() {
		match character {
			'\\' => out.push_str("\\\\"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			other => out.push(other),
		}
	}
	out
}

fn symbol_field(symbol: Option<P<Symbol>>) -> String {
	symbol.map_or_else(
		|| "-".to_owned(),
		|symbol| {
			let name = symbol.name();
			let name = name
				.strip_prefix(INTERNAL_SYMBOL_NAME_PREFIX)
				.map_or_else(|| name.to_owned(), |rest| format!("#{rest}"));
			format!(
				"{},{:x},{}",
				escape(&name),
				symbol.flags().bits(),
				symbol.declarations().len()
			)
		},
	)
}

/// tsgo's bit for `ObjectFlags.FromTypeNode`; TypeScript 7.1 moved it down one.
const GO_FROM_TYPE_NODE: u32 = 1 << 30;

/// The object flags that describe a type, in typescript-go's layout.
///
/// That is the kind and propagated flags, the object-only flags and `FromTypeNode`, a union's
/// `ContainsIntersections` and an intersection's `IsConstrainedTypeVariable`. Memo bits (the `Computed` flags, `MembersResolved`,
/// the identical-base-type and tsrs-only caches) record which queries already ran, not what the type
/// is, so they are left out.
#[must_use]
pub fn object_flags(t: P<Type>) -> u32 {
	let flags = t.object_flags();
	let mut out = flags.bits() & ((1 << 19) - 1);
	if t.flags().intersects(TypeFlags::Object) {
		out |= (flags
			& (ObjectFlags::ContainsSpread
				| ObjectFlags::ObjectRestType
				| ObjectFlags::InstantiationExpressionType
				| ObjectFlags::SingleSignatureType
				| ObjectFlags::IsClassInstanceClone))
			.bits();
		if flags.intersects(ObjectFlags::FromTypeNode) {
			out |= GO_FROM_TYPE_NODE;
		}
	} else if t.flags().intersects(TypeFlags::Union) {
		out |= (flags & ObjectFlags::ContainsIntersections).bits();
	} else if t.flags().intersects(TypeFlags::Intersection) {
		out |= (flags & ObjectFlags::IsConstrainedTypeVariable).bits();
	}
	out
}

/// Runs a query the checker may panic on and prints `panic` instead, as the oracle does for
/// typescript-go's panics (`getTypeAtLocation` of a type-only import clause).
fn guarded(query: impl FnOnce() -> String) -> String {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(query)).unwrap_or_else(|_| "panic".to_owned())
}

fn queries(checker: &mut TypeChecker<'_>, node: P<Node>) -> String {
	let t = checker.get_type_at_location(node);
	let apparent = checker.get_apparent_type(t);
	let printed = checker.type_to_string(t);
	let symbol = checker.get_symbol_at_location(node);
	let properties = checker.get_properties_of_type(apparent).len();
	let calls = checker.get_call_signatures(t).len();
	let constructs = checker.get_signatures_of_type(t, SignatureKind::Construct).len();
	format!(
		"t={:x}\to={:x}\ts={}\tsym={}\ttsym={}\tap={properties}\tcs={calls}\tks={constructs}",
		t.flags().bits(),
		object_flags(t),
		escape(&printed),
		symbol_field(symbol),
		symbol_field(t.symbol()),
	)
}

fn predicates(state: &mut TypeState<'_>, node: P<Node>) -> String {
	let t = TypeChecker::new(crate::types::TypeContext::checker(state)).get_type_at_location(node);
	let mut definitely = 0_u32;
	let mut possibly = 0_u32;
	for (index, check) in PREDICATES.iter().enumerate() {
		if is_definitely_type(state, t, std::slice::from_ref(check)) {
			definitely |= 1 << index;
		}
		if is_possibly_type(state, t, std::slice::from_ref(check)) {
			possibly |= 1 << index;
		}
	}
	let mut extra = 0_u32;
	if is_definitely_type(state, t, &[TypeCheck::Object, TypeCheck::NonPrimitive]) {
		extra |= 1;
	}
	if is_definitely_type(state, t, &[TypeCheck::String, TypeCheck::Number]) {
		extra |= 1 << 1;
	}
	if is_shared_table_type(state, t) {
		extra |= 1 << 2;
	}
	let mut leaves = 0_usize;
	walk_types(state, t, &mut |_| leaves += 1);
	format!(
		"d={definitely:x}\tp={possibly:x}\tx={extra:x}\tw={leaves}\tfds={}\tfcs={}",
		symbol_field(get_first_defined_symbol(state, t)),
		symbol_field(get_first_construct_symbol(t)),
	)
}

/// Runs `visit` on every dumped node of every file, on the file's checker, and returns what it
/// produced per file, in the order of `files`.
fn per_file<T: Send>(
	project: Project,
	files: &[P<SourceFile>],
	visit: impl Fn(&mut TypeChecker<'_>, P<SourceFile>) -> Vec<T> + Sync,
) -> Vec<Vec<T>> {
	let results: Mutex<Vec<Option<Vec<T>>>> = Mutex::new((0..files.len()).map(|_| None).collect());
	for_each_checker_group(project.program, files, |checker, index, file| {
		let produced = visit(checker, file);
		if let Ok(mut results) = results.lock()
			&& let Some(slot) = results.get_mut(index)
		{
			*slot = Some(produced);
		}
	});
	results
		.into_inner()
		.unwrap_or_default()
		.into_iter()
		.map(Option::unwrap_or_default)
		.collect()
}

/// Writes the differential lines for every source file rotor compiles in `project`.
///
/// Must run on a large stack ([`crate::with_large_stack`]).
#[must_use]
pub fn dump_project(project: Project) -> Vec<String> {
	let program = project.program;
	let _ = program.get_semantic_diagnostics(&Context::default(), None);
	let directory = project.directory().to_owned();
	let files = project.source_files();

	let first = per_file(project, &files, |checker, file| {
		let relative = get_relative_path_from_directory(&directory, file.file_name(), &ComparePathsOptions::default());
		let mut lines = Vec::new();
		walk(file.as_node(), &mut |node| {
			if dumped(node) {
				let key = format!("{relative}:{}:{}:{:?}", node.pos(), node.end(), node.kind());
				lines.push((key, guarded(|| queries(checker, node))));
			}
		});
		lines
	});

	let second = per_file(project, &files, |checker, file| {
		let mut ambient = AmbientSymbols::default();
		let mut state = TypeState::new(checker.checker(), &mut ambient);
		let mut lines = Vec::new();
		walk(file.as_node(), &mut |node| {
			if dumped(node) {
				lines.push(guarded(|| predicates(&mut state, node)));
			}
		});
		lines
	});

	first
		.into_iter()
		.zip(second)
		.flat_map(|(first, second)| first.into_iter().zip(second))
		.map(|((key, queries), predicates)| {
			format!(
				"{key}\t{}",
				format!("{queries}\t{predicates}").replace(&directory, "<project>")
			)
		})
		.collect()
}

/// A documented difference between TypeScript 7.1 (tsrs) and rotor's typescript-go pin. Each is
/// explained, with evidence, in `crates/rotor_tsrs/ORACLE.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Deviation {
	/// `JsxSpreadAttribute` has a `DeclarationBase`, so it is a declaration with a line of its own.
	JsxSpreadAttributeDeclaration,
	/// Interface `extends` and class `implements` elements parse as type references, so the pin's
	/// `ExpressionWithTypeArguments` lines go and the names inside become type positions.
	HeritageTypeReference,
	/// A type-only import clause without a default name is no longer a type declaration; the pin
	/// panics asking for its type.
	TypeOnlyImportClause,
	/// The node builder checks the truncation budget on entering every type, so `typeToString`
	/// elides (`...`) in other places.
	TypeToStringTruncation,
	/// The `name?: never` members TypeScript adds to normalize a union of object literals print in
	/// another place among the other members.
	ObjectMemberOrder,
	/// A type name in a printed type gets its namespace qualifier (`t.check` for `check`).
	EntityNameQualification,
	/// Enum members with dynamic names stay out of the enum type.
	EnumDynamicMemberName,
	/// The declarations of a synthetic union or intersection property are deduplicated.
	UnionPropertyDeclarations,
	/// An instantiation expression's type symbol carries the declarations of the type it
	/// instantiates.
	InstantiationExpressionDeclarations,
	/// The type literal symbol of a synthetic default import carries the module's declarations.
	SyntheticDefaultDeclarations,
	/// Instantiating a member with a mapper that maps only `this` returns the member itself when it
	/// does not use `this`, so it is not transient.
	ThislessMemberInstantiation,
	/// A type alias's own conditional type nested four or more levels deep has no base constraint.
	DeclaredConditionalConstraint,
}

/// A line the two dumps disagree on that no [`Deviation`] explains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
	/// The node key.
	pub key: String,
	/// The Go line's fields after the key, or `None` if Go has no such node.
	pub go: Option<String>,
	/// The Rust line's fields after the key, or `None` if Rust has no such node.
	pub rust: Option<String>,
}

/// How a Rust dump compares with the Go oracle's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
	/// Lines in the Go dump.
	pub go_lines: usize,
	/// Lines in the Rust dump.
	pub rust_lines: usize,
	/// Lines that agree exactly.
	pub matched: usize,
	/// Lines that differ only by documented deviations.
	pub deviating: usize,
	/// For each deviation, the lines it explains; a line can need two.
	pub deviations: BTreeMap<Deviation, usize>,
	/// Lines that differ for no documented reason.
	pub mismatches: Vec<Mismatch>,
}

fn split_line(line: &str) -> (&str, &str) {
	line.split_once('\t').unwrap_or((line, ""))
}

fn fields(rest: &str) -> Vec<(&str, &str)> {
	rest.split('\t')
		.map(|field| field.split_once('=').unwrap_or((field, "")))
		.collect()
}

fn field<'a>(fields: &[(&str, &'a str)], name: &str) -> &'a str {
	fields
		.iter()
		.find(|(key, _)| *key == name)
		.map_or("", |(_, value)| value)
}

/// A `name,flags,declarations` symbol field.
struct SymbolField<'a> {
	name: &'a str,
	flags: u32,
	declarations: usize,
}

fn symbol_parts(field: &str) -> Option<SymbolField<'_>> {
	let (head, declarations) = field.rsplit_once(',')?;
	let (name, flags) = head.rsplit_once(',')?;
	Some(SymbolField {
		name,
		flags: u32::from_str_radix(flags, 16).ok()?,
		declarations: declarations.parse().ok()?,
	})
}

/// `ast.SymbolFlagsTransient`.
const TRANSIENT: u32 = 1 << 25;

/// Which deviation explains a differing symbol field, if one does.
fn symbol_deviation(column: &str, go: &str, rust: &str) -> Option<Deviation> {
	let (go, rust) = (symbol_parts(go)?, symbol_parts(rust)?);
	if go.name != rust.name {
		return None;
	}
	// A synthetic union or intersection property is transient.
	if matches!(column, "sym" | "tsym")
		&& go.flags == rust.flags
		&& go.flags & TRANSIENT != 0
		&& go.declarations > rust.declarations
		&& rust.declarations > 0
	{
		return Some(Deviation::UnionPropertyDeclarations);
	}
	if go.flags == rust.flags && go.declarations == 0 && rust.declarations > 0 {
		return match go.name {
			"#instantiationExpression" => Some(Deviation::InstantiationExpressionDeclarations),
			"#type" => Some(Deviation::SyntheticDefaultDeclarations),
			_ => None,
		};
	}
	if go.declarations == rust.declarations && go.flags ^ rust.flags == TRANSIENT && go.flags & TRANSIENT != 0 {
		return Some(Deviation::ThislessMemberInstantiation);
	}
	None
}

/// The node builder's truncation budget is 160 on its approximate length, which counts more than
/// the text it prints. A text this long, or one already holding a marker, has reached the budget:
/// no elision in either dump starts in a shorter text without one.
const ELISION_FLOOR: usize = 120;

/// The length past which the printer cuts a type's text and ends it with `...`.
const FINAL_CUT: usize = 160;

/// The length of the elision marker at `index`, and whether it can stand for several members:
/// `... N more ...`, or a bare `...` that is not a rest or spread. Zero when there is none.
fn elision_at(text: &[u8], index: usize) -> (usize, bool) {
	let rest = &text[index..];
	if !rest.starts_with(b"...") {
		return (0, false);
	}
	if let Some(tail) = rest.strip_prefix(b"... ") {
		let digits = tail.iter().take_while(|byte| byte.is_ascii_digit()).count();
		if digits > 0 && tail[digits..].starts_with(b" more ...") {
			return (4 + digits + b" more ...".len(), true);
		}
	}
	match rest.get(3) {
		Some(next) if next.is_ascii_alphanumeric() || b"_$[({".contains(next) => (0, false),
		// `{ ...; }` stands for every member of an object type.
		_ => (3, text[..index].ends_with(b"{ ") && rest[3..].starts_with(b"; }")),
	}
}

fn has_elision(text: &str) -> bool {
	(0..text.len()).any(|index| elision_at(text.as_bytes(), index).0 > 0)
}

/// The ends of the nonempty, bracket-balanced runs of `text` starting at `start`: what one elided
/// type can stand for. A run stops at an unmatched closer, and at a top-level `;` or `,` unless
/// the marker stands for several members.
fn balanced_ends(text: &[u8], start: usize, many: bool) -> Vec<usize> {
	let mut ends = Vec::new();
	let mut closers: Vec<u8> = Vec::new();
	let mut quote: Option<u8> = None;
	let mut index = start;
	while index < text.len() {
		let byte = text[index];
		if let Some(open) = quote {
			if byte == b'\\' {
				index += 2;
				continue;
			}
			if byte == open {
				quote = None;
			}
		} else {
			match byte {
				b'"' | b'\'' | b'`' => quote = Some(byte),
				b'(' => closers.push(b')'),
				b'[' => closers.push(b']'),
				b'{' => closers.push(b'}'),
				b'<' => closers.push(b'>'),
				b'>' if index > 0 && text[index - 1] == b'=' => {}
				b')' | b']' | b'}' | b'>' => {
					if closers.pop() != Some(byte) {
						return ends;
					}
				}
				b';' | b',' if closers.is_empty() && !many => return ends,
				_ => {}
			}
		}
		index += 1;
		if closers.is_empty() && quote.is_none() {
			ends.push(index.min(text.len()));
		}
	}
	ends
}

/// Whether two printed types agree everywhere neither elided: each elision marker on one side may
/// stand for a balanced run on the other, and a string the printer cut at the end (`...` past
/// [`FINAL_CUT`]) may stop matching at any marker of the other.
fn elision_match(go: &str, rust: &str) -> bool {
	struct Matcher<'a> {
		go: &'a [u8],
		rust: &'a [u8],
		go_cut: bool,
		rust_cut: bool,
		memo: HashMap<(usize, usize), bool>,
	}
	impl Matcher<'_> {
		fn matches(&mut self, i: usize, j: usize) -> bool {
			if let Some(&known) = self.memo.get(&(i, j)) {
				return known;
			}
			self.memo.insert((i, j), false);
			let result = self.compute(i, j);
			self.memo.insert((i, j), result);
			result
		}

		fn compute(&mut self, i: usize, j: usize) -> bool {
			let (go, rust) = (self.go, self.rust);
			if i == go.len() && j == rust.len() {
				return true;
			}
			if (self.go_cut && &go[i..] == b"...") || (self.rust_cut && &rust[j..] == b"...") {
				return true;
			}
			let (go_marker, go_many) = if i < go.len() { elision_at(go, i) } else { (0, false) };
			let (rust_marker, rust_many) = if j < rust.len() {
				elision_at(rust, j)
			} else {
				(0, false)
			};
			if (self.go_cut && rust_marker > 0) || (self.rust_cut && go_marker > 0) {
				return true;
			}
			if go_marker == 0 && rust_marker == 0 {
				return i < go.len() && j < rust.len() && go[i] == rust[j] && self.matches(i + 1, j + 1);
			}
			if go_marker > 0
				&& rust_marker > 0
				&& go[i..i + go_marker] == rust[j..j + rust_marker]
				&& self.matches(i + go_marker, j + rust_marker)
			{
				return true;
			}
			if go_marker > 0
				&& balanced_ends(rust, j, go_many)
					.into_iter()
					.any(|end| self.matches(i + go_marker, end))
			{
				return true;
			}
			rust_marker > 0
				&& balanced_ends(go, i, rust_many)
					.into_iter()
					.any(|end| self.matches(end, j + rust_marker))
		}
	}
	let cut = |text: &str| text.len() >= FINAL_CUT && text.ends_with("...");
	Matcher {
		go: go.as_bytes(),
		rust: rust.as_bytes(),
		go_cut: cut(go),
		rust_cut: cut(rust),
		memo: HashMap::new(),
	}
	.matches(0, 0)
}

/// `NAME?: never;` members, with the space before them: the ones TypeScript adds when it
/// normalizes a union of object literals.
static NEVER_MEMBER: LazyLock<Option<Regex>> =
	LazyLock::new(|| Regex::new(r#" (?:"[^"]*"|[A-Za-z0-9_$]+)\?: never;"#).ok());

/// `text` without its `?: never` members, and those members in order.
fn split_never_members(text: &str) -> (String, Vec<&str>) {
	let Some(pattern) = NEVER_MEMBER.as_ref() else {
		return (text.to_owned(), Vec::new());
	};
	let mut members: Vec<&str> = pattern.find_iter(text).map(|found| found.as_str()).collect();
	members.sort_unstable();
	(pattern.replace_all(text, "").into_owned(), members)
}

/// Whether `rust` is `go` with namespace qualifiers (`t.`) added before some type names.
fn qualifiers_added(go: &str, rust: &str) -> bool {
	let (go, rust) = (go.as_bytes(), rust.as_bytes());
	let identifier = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$';
	let (mut i, mut j, mut added) = (0, 0, false);
	while j < rust.len() {
		if i < go.len() && go[i] == rust[j] {
			i += 1;
			j += 1;
			continue;
		}
		// A qualifier starts at an identifier boundary and ends with `.` before the name.
		let length = rust[j..].iter().take_while(|&&byte| identifier(byte)).count();
		let at_boundary = j == 0 || !identifier(rust[j - 1]);
		if length == 0 || !at_boundary || rust.get(j + length) != Some(&b'.') {
			return false;
		}
		j += length + 1;
		added = true;
	}
	added && i == go.len()
}

/// The deviations that explain two different `typeToString` results, or `None`.
fn type_string_deviations(type_flags: u32, go: &str, rust: &str) -> Option<Vec<Deviation>> {
	let (go_stripped, go_never) = split_never_members(go);
	let (rust_stripped, rust_never) = split_never_members(rust);
	let object_like = type_flags & (TypeFlags::Object | TypeFlags::Union).bits() != 0;
	if object_like && !go_never.is_empty() && go_stripped == rust_stripped && go_never == rust_never {
		return Some(vec![Deviation::ObjectMemberOrder]);
	}
	// Go reached the budget, Rust elided, and the two agree everywhere neither elided.
	if has_elision(rust) && (go.len() >= ELISION_FLOOR || has_elision(go)) {
		if elision_match(go, rust) {
			return Some(vec![Deviation::TypeToStringTruncation]);
		}
		if object_like
			&& (!go_never.is_empty() || !rust_never.is_empty())
			&& elision_match(&go_stripped, &rust_stripped)
		{
			return Some(vec![Deviation::TypeToStringTruncation, Deviation::ObjectMemberOrder]);
		}
	}
	qualifiers_added(go, rust).then(|| vec![Deviation::EntityNameQualification])
}

/// `IsDefinitelyType` / `IsPossiblyType` bit of [`TypeCheck::Array`] in the `d` and `p` masks.
const ARRAY_BIT: u32 = 1 << 12;

/// Whether a line's predicate fields are what a conditional type without a base constraint gives:
/// no apparent properties, one leaf, no extras, and at most the array check, the same both ways.
fn unconstrained_conditional(rust: &[(&str, &str)]) -> bool {
	let mask = |name| u32::from_str_radix(field(rust, name), 16).ok();
	field(rust, "ap") == "0"
		&& field(rust, "w") == "1"
		&& field(rust, "x") == "0"
		&& mask("d").is_some_and(|definitely| definitely & !ARRAY_BIT == 0 && mask("p") == Some(definitely))
}

/// The deviations that explain every field two lines differ in, or `None` if one field is
/// unexplained.
fn classify_pair(kind: &str, go: &str, rust: &str) -> Option<Vec<Deviation>> {
	let go_fields = fields(go);
	let rust_fields = fields(rust);
	if go_fields.len() != rust_fields.len() {
		return None;
	}
	let mut found = Vec::new();
	let mut add = |deviation| {
		if !found.contains(&deviation) {
			found.push(deviation);
		}
	};
	let type_flags = u32::from_str_radix(field(&go_fields, "t"), 16).unwrap_or(0);
	let computed_enum = field(&go_fields, "tsym").starts_with("#computed,");
	let declared_conditional = matches!(kind, "TypeAliasDeclaration" | "Identifier")
		&& type_flags & TypeFlags::Conditional.bits() != 0
		&& unconstrained_conditional(&rust_fields);
	for ((name, go_value), (rust_name, rust_value)) in go_fields.iter().zip(&rust_fields) {
		if name != rust_name {
			return None;
		}
		if go_value == rust_value {
			continue;
		}
		match *name {
			"s" | "tsym" | "fds" if computed_enum => add(Deviation::EnumDynamicMemberName),
			"s" => type_string_deviations(type_flags, go_value, rust_value)?
				.into_iter()
				.for_each(&mut add),
			"sym" | "tsym" | "fds" => add(symbol_deviation(name, go_value, rust_value)?),
			"ap" | "d" | "p" | "x" | "w" if declared_conditional => add(Deviation::DeclaredConditionalConstraint),
			_ => return None,
		}
	}
	Some(found)
}

/// A node key's kind.
fn kind_of(key: &str) -> &str {
	key.rsplit(':').next().unwrap_or("")
}

/// A node key's file and span.
fn span(key: &str) -> Option<(&str, u32, u32)> {
	let mut parts = key.rsplitn(4, ':');
	let _kind = parts.next()?;
	let end = parts.next()?.parse().ok()?;
	let pos = parts.next()?.parse().ok()?;
	Some((parts.next()?, pos, end))
}

fn record(comparison: &mut Comparison, deviations: &[Deviation]) {
	comparison.deviating += 1;
	for &deviation in deviations {
		*comparison.deviations.entry(deviation).or_default() += 1;
	}
}

/// Compares a Rust dump with the Go oracle's text, classifying every difference as documented
/// [`Deviation`]s or a [`Mismatch`].
#[must_use]
pub fn compare(go: &str, rust: &[String]) -> Comparison {
	let go_lines: Vec<&str> = go.lines().collect();
	let go_by_key: HashMap<&str, &str> = go_lines.iter().map(|line| split_line(line)).collect();
	let rust_keys: HashSet<&str> = rust.iter().map(|line| split_line(line).0).collect();
	// The pin's heritage elements, which TypeScript 7.1 parses as type references, and the entity
	// names under them.
	let mut by_start: HashMap<(&str, u32), Vec<(u32, &str)>> = HashMap::new();
	for line in &go_lines {
		if let Some((file, pos, end)) = span(split_line(line).0) {
			by_start
				.entry((file, pos))
				.or_default()
				.push((end, kind_of(split_line(line).0)));
		}
	}
	let mut heritage: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
	for line in &go_lines {
		let key = split_line(line).0;
		if key.ends_with(":ExpressionWithTypeArguments")
			&& !rust_keys.contains(key)
			&& let Some((file, pos, end)) = span(key)
			&& let Some(name_end) = by_start.get(&(file, pos)).and_then(|starts| {
				starts
					.iter()
					.filter(|(name_end, kind)| {
						*name_end <= end && matches!(*kind, "Identifier" | "PropertyAccessExpression")
					})
					.map(|(name_end, _)| *name_end)
					.max()
			}) {
			heritage.entry(file).or_default().push((pos, name_end));
		}
	}
	let in_heritage = |key: &str| {
		matches!(
			kind_of(key),
			"Identifier" | "PropertyAccessExpression" | "QualifiedName"
		) && span(key).is_some_and(|(file, pos, end)| {
			heritage
				.get(file)
				.is_some_and(|spans| spans.iter().any(|&(start, stop)| start <= pos && end <= stop))
		})
	};
	let is_wrapper = |key: &str| {
		matches!(kind_of(key), "ExpressionWithTypeArguments" | "TypeReference")
			&& span(key).is_some_and(|(file, pos, _)| {
				heritage
					.get(file)
					.is_some_and(|spans| spans.iter().any(|&(start, _)| start == pos))
			})
	};

	let mut comparison = Comparison {
		go_lines: go_lines.len(),
		rust_lines: rust.len(),
		..Comparison::default()
	};
	for line in rust {
		let (key, answers) = split_line(line);
		let deviations = match go_by_key.get(key) {
			Some(&go_rest) if go_rest == answers => {
				comparison.matched += 1;
				continue;
			}
			_ if is_wrapper(key) || in_heritage(key) => Some(vec![Deviation::HeritageTypeReference]),
			Some(&"panic\tpanic") if key.ends_with(":ImportClause") => Some(vec![Deviation::TypeOnlyImportClause]),
			Some(&go_rest) => classify_pair(kind_of(key), go_rest, answers),
			None if key.ends_with(":JsxSpreadAttribute") => Some(vec![Deviation::JsxSpreadAttributeDeclaration]),
			None => None,
		};
		match deviations {
			Some(deviations) => record(&mut comparison, &deviations),
			None => comparison.mismatches.push(Mismatch {
				key: key.to_owned(),
				go: go_by_key.get(key).map(|&go_rest| go_rest.to_owned()),
				rust: Some(answers.to_owned()),
			}),
		}
	}
	for line in &go_lines {
		let (key, answers) = split_line(line);
		if rust_keys.contains(key) {
			continue;
		}
		if is_wrapper(key) || in_heritage(key) {
			record(&mut comparison, &[Deviation::HeritageTypeReference]);
		} else {
			comparison.mismatches.push(Mismatch {
				key: key.to_owned(),
				go: Some(answers.to_owned()),
				rust: None,
			});
		}
	}
	comparison
}

#[cfg(test)]
mod tests {
	use super::*;

	fn line(t: &str, s: &str) -> String {
		format!("t={t}\to=0\ts={s}\tsym=-\ttsym=-\tap=0\tcs=0\tks=0\td=0\tp=0\tx=0\tw=1\tfds=-\tfcs=-")
	}

	fn classify(t: &str, go: &str, rust: &str) -> Option<Vec<Deviation>> {
		classify_pair("Identifier", &line(t, go), &line(t, rust))
	}

	#[test]
	fn truncation_needs_the_budget_and_agreement_outside_elisions() {
		let long = "x".repeat(ELISION_FLOOR);
		let go = format!("{{ a: {long}; b: {{ c: number; }}; }}");
		let rust = format!("{{ a: {long}; b: ...; }}");
		assert_eq!(
			classify("100000", &go, &rust),
			Some(vec![Deviation::TypeToStringTruncation])
		);
		// A difference outside the elided part is a mismatch.
		let wrong = format!("{{ a: {long}y; b: ...; }}");
		assert_eq!(classify("100000", &go, &wrong), None);
		// Below the budget, with no marker in Go, nothing excuses an elision.
		assert_eq!(classify("100000", "{ b: { c: number; }; }", "{ b: ...; }"), None);
		// Go's own marker shows it reached the budget.
		assert_eq!(
			classify("100000", "[[A, B], [...]]", "[[A, B], ...]"),
			Some(vec![Deviation::TypeToStringTruncation])
		);
	}

	#[test]
	fn member_order_moves_only_never_members_of_object_types() {
		let go = "{ kind: 1; value?: never; } | { value: string; kind?: never; }";
		let rust = "{ value?: never; kind: 1; } | { kind?: never; value: string; }";
		assert_eq!(classify("100000", go, rust), Some(vec![Deviation::ObjectMemberOrder]));
		assert_eq!(classify("4", go, rust), None);
		assert_eq!(classify("100000", "{ a: 1; b: 2; }", "{ b: 2; a: 1; }"), None);
	}

	#[test]
	fn qualification_only_adds_namespace_prefixes() {
		assert_eq!(
			classify(
				"100000",
				"{ [index: string]: check<any>; }",
				"{ [index: string]: t.check<any>; }"
			),
			Some(vec![Deviation::EntityNameQualification])
		);
		assert_eq!(classify("100000", "{ a: check<any>; }", "{ a: tcheck<any>; }"), None);
	}

	#[test]
	fn union_property_declarations_need_a_transient_symbol() {
		let with = |sym: &str| line("1", "x").replace("sym=-", &format!("sym={sym}"));
		let transient = format!("{:x}", TRANSIENT | 4);
		assert_eq!(
			classify_pair(
				"Identifier",
				&with(&format!("a,{transient},3")),
				&with(&format!("a,{transient},1"))
			),
			Some(vec![Deviation::UnionPropertyDeclarations])
		);
		assert_eq!(classify_pair("Identifier", &with("a,4,3"), &with("a,4,1")), None);
	}
}
