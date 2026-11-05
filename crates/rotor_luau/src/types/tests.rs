//! Port of `internal/luau/types_test.go`.

use crate::{
	Ast, NodeData, NodeId, SyntaxKind, TypeAccess, TypeAlias, TypeArray, TypeFunction, TypeFunctionDeclaration,
	TypeFunctionParameter, TypeGeneric, TypeOptional, TypeReference, TypeVariadic, element,
};

fn check(a: &Ast, name: &str, parent: NodeId, children: &[NodeId]) {
	for &child in children {
		assert_eq!(
			a.parent(child),
			Some(parent),
			"{name}: child {} not adopted",
			a.kind(child)
		);
	}
}

#[test]
#[expect(clippy::many_single_char_names, reason = "mirrors the Go test's names")]
fn type_constructors_adopt() {
	let mut a = Ast::new();
	let argument = a.type_name("A");
	let arguments = a.list_from::<element::TypeNode>([argument]);
	let reference = a.type_reference("Map", Some(arguments));
	check(&a, "reference", reference, &[argument]);

	let inner = a.type_name("number");
	let optional = a.type_optional(inner);
	check(&a, "optional", optional, &[inner]);

	let (x, y) = (a.type_name("A"), a.type_name("B"));
	let members = a.list_from::<element::TypeNode>([x, y]);
	let union = a.type_union(members);
	check(&a, "union", union, &[x, y]);

	let (c, d) = (a.type_name("C"), a.type_name("D"));
	let members = a.list_from::<element::TypeNode>([c, d]);
	let intersection = a.type_intersection(members);
	check(&a, "intersection", intersection, &[c, d]);

	let value = a.type_name("number");
	let property = a.type_table_prop(TypeAccess::Read, "x", value);
	check(&a, "prop", property, &[value]);
	let (key, indexed_type) = (a.type_name("string"), a.type_name("boolean"));
	let indexer = a.type_table_indexer(TypeAccess::None, key, indexed_type);
	check(&a, "indexer", indexer, &[key, indexed_type]);
	let fields = a.list_from::<element::TypeTableField>([property, indexer]);
	let table = a.type_table(fields);
	check(&a, "table", table, &[property, indexer]);

	let element = a.type_name("number");
	let array = a.type_array(element);
	check(&a, "array", array, &[element]);

	let parameter_type = a.type_name("T");
	let var_args = a.type_generic_pack("U");
	let empty = a.new_list::<element::TypeNode>();
	let return_type = a.type_pack(empty);
	let default_type = a.type_name("number");
	let function = a.type_function(
		vec![TypeGeneric::new("T").with_default(default_type), TypeGeneric::pack("U")],
		vec![TypeFunctionParameter::new("value", parameter_type)],
		Some(var_args),
		return_type,
	);
	check(
		&a,
		"function",
		function,
		&[parameter_type, var_args, return_type, default_type],
	);
	let payload = a.get::<TypeFunction>(function).expect("function");
	assert!(
		payload.generics[0].default == Some(default_type) && payload.parameters[0].ty == parameter_type,
		"function must keep the adopted nodes in place"
	);

	let expression = a.id("x");
	let typeof_node = a.type_typeof(expression);
	check(&a, "typeof", typeof_node, &[expression]);

	let first = a.type_name("A");
	let rest_inner = a.type_name("B");
	let rest = a.type_variadic(rest_inner);
	let elements = a.list_from::<element::TypeNode>([first, rest]);
	let pack = a.type_pack(elements);
	check(&a, "pack", pack, &[first, rest]);
	let inner = a.get::<TypeVariadic>(rest).expect("variadic").inner;
	check(&a, "variadic", rest, &[inner]);

	let (cast_inner, cast_type) = (a.id("v"), a.type_name("Foo"));
	let cast = a.cast(cast_inner, cast_type);
	check(&a, "cast", cast, &[cast_inner, cast_type]);

	let (body, alias_default) = (a.type_name("number"), a.type_name("string"));
	let alias = a.type_alias("T", vec![TypeGeneric::new("K").with_default(alias_default)], Some(body));
	check(&a, "alias", alias, &[body, alias_default]);
	let late = a.type_alias("Late", Vec::new(), None);
	assert!(
		a.get::<TypeAlias>(late).expect("alias").body.is_none(),
		"nil body must stay nil"
	);
	let late_body = a.type_name("number");
	a.set_type_alias_body(late, late_body);
	check(&a, "set body", late, &[late_body]);

	let parameter = a.id("t");
	let statement = a.return_statement(parameter);
	let params = a.list_from::<element::AnyIdentifier>([parameter]);
	let statements = a.list_from::<element::Statement>([statement]);
	let declaration = a.type_function_declaration("F", params, true, statements);
	check(&a, "type function", declaration, &[statement]);
	let payload = a.get::<TypeFunctionDeclaration>(declaration).expect("declaration");
	let head = a.link_value(a.list_head(payload.parameters).expect("head"));
	assert_eq!(
		a.parent(head),
		Some(declaration),
		"type function parameters must be adopted"
	);
	let params = a.param_data(declaration).expect("param data");
	assert!(
		params.params == payload.parameters
			&& params.has_dot_dot_dot
			&& params.var_args_type.is_none()
			&& a.return_type_data(declaration).is_none(),
		"type function ParamData"
	);
}

#[test]
fn type_constructors_clone_parented_children() {
	let mut a = Ast::new();
	let shared = a.type_name("number");
	let first = a.type_optional(shared);
	let second = a.type_array(shared);
	assert_eq!(
		a.get::<TypeOptional>(first).expect("optional").inner,
		Some(shared),
		"first use must keep the original node"
	);
	let element = a.get::<TypeArray>(second).expect("array").element;
	assert_ne!(element, shared, "second use must be a clone");
	assert!(
		a.get::<TypeReference>(element)
			.is_some_and(|reference| reference.name == "number")
			&& a.parent(element) == Some(second),
		"clone must preserve fields and be adopted"
	);
	assert_eq!(a.parent(shared), Some(first), "original parent untouched");
}

#[test]
fn optional_type() {
	let mut a = Ast::new();
	let inner = a.type_name("A");
	let already = a.type_optional(inner);
	assert_eq!(a.optional_type(already), already, "an optional stays as it is");
	for name in ["nil", "any", "unknown"] {
		let reference = a.type_name(name);
		assert_eq!(a.optional_type(reference), reference, "{name} must stay unchanged");
	}
	let number = a.type_name("number");
	let wrapped = a.optional_type(number);
	assert_eq!(
		a.get::<TypeOptional>(wrapped).and_then(|optional| optional.inner),
		Some(number),
		"other types are wrapped"
	);
	let qualified = a.type_qualified_reference("M", "any", None);
	let wrapped = a.optional_type(qualified);
	assert_eq!(
		a.kind(wrapped),
		SyntaxKind::TypeOptional,
		"a qualified `any` is an ordinary reference"
	);
	let argument = a.type_name("T");
	let arguments = a.list_from::<element::TypeNode>([argument]);
	let generic = a.type_reference("any", Some(arguments));
	let wrapped = a.optional_type(generic);
	assert_eq!(
		a.kind(wrapped),
		SyntaxKind::TypeOptional,
		"an instantiated `any` is an ordinary reference"
	);
}

#[test]
fn binding_type() {
	let mut a = Ast::new();
	let identifier = a.id("x");
	assert_eq!(a.binding_type(identifier), None, "unannotated identifier");
	let annotation = a.type_name("number");
	a.set_binding_type(identifier, Some(annotation));
	assert!(
		a.binding_type(identifier) == Some(annotation) && a.parent(annotation) == Some(identifier),
		"annotation must be stored and adopted"
	);
	let clone = a.with_origin(identifier, None);
	assert_eq!(a.binding_type(clone), Some(annotation), "a clone keeps the annotation");
	a.set_binding_type(identifier, None);
	assert_eq!(a.binding_type(identifier), None, "None clears the annotation");

	let temp = a.temp_id("param");
	let temp_annotation = a.type_name("Foo");
	a.set_binding_type(temp, Some(temp_annotation));
	assert!(
		a.binding_type(temp) == Some(temp_annotation) && a.parent(temp_annotation) == Some(temp),
		"temporary identifiers carry annotations too"
	);
	// A parented annotation is cloned on adoption, like any other child.
	a.set_binding_type(identifier, Some(temp_annotation));
	let got = a.binding_type(identifier).expect("annotation");
	assert!(
		got != temp_annotation
			&& a.get::<TypeReference>(got)
				.is_some_and(|reference| reference.name == "Foo"),
		"a parented annotation must be cloned"
	);
}

#[test]
#[expect(clippy::many_single_char_names, reason = "mirrors the Go test's names")]
fn type_kinds() {
	let mut a = Ast::new();
	let mut cases: Vec<(NodeId, SyntaxKind)> = Vec::new();
	let mut push = |node, kind| cases.push((node, kind));
	push(a.type_name("x"), SyntaxKind::TypeReference);
	push(a.type_string_singleton("a"), SyntaxKind::TypeStringSingleton);
	push(a.type_boolean_singleton(true), SyntaxKind::TypeBooleanSingleton);
	let x = a.type_name("x");
	push(a.type_optional(x), SyntaxKind::TypeOptional);
	let empty = a.new_list::<element::TypeNode>();
	push(a.type_union(empty), SyntaxKind::TypeUnion);
	let empty = a.new_list::<element::TypeNode>();
	push(a.type_intersection(empty), SyntaxKind::TypeIntersection);
	let empty = a.new_list::<element::TypeTableField>();
	push(a.type_table(empty), SyntaxKind::TypeTable);
	let x = a.type_name("x");
	push(a.type_array(x), SyntaxKind::TypeArray);
	push(
		a.alloc(TypeFunction {
			generics: Vec::new(),
			parameters: Vec::new(),
			var_args: None,
			ret: None,
		}),
		SyntaxKind::TypeFunction,
	);
	let x = a.id("x");
	push(a.type_typeof(x), SyntaxKind::TypeTypeof);
	let empty = a.new_list::<element::TypeNode>();
	push(a.type_pack(empty), SyntaxKind::TypePack);
	let x = a.type_name("x");
	push(a.type_variadic(x), SyntaxKind::TypeVariadic);
	push(a.type_generic_pack("T"), SyntaxKind::TypeGenericPack);
	let x = a.type_name("x");
	push(a.type_table_prop(TypeAccess::None, "a", x), SyntaxKind::TypeTableProp);
	let (k, v) = (a.type_name("k"), a.type_name("v"));
	push(
		a.type_table_indexer(TypeAccess::None, k, v),
		SyntaxKind::TypeTableIndexer,
	);
	let (x, t) = (a.id("x"), a.type_name("T"));
	push(a.cast(x, t), SyntaxKind::CastExpression);
	let x = a.type_name("x");
	push(a.type_alias("T", Vec::new(), Some(x)), SyntaxKind::TypeAlias);
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	push(
		a.type_function_declaration("F", params, false, body),
		SyntaxKind::TypeFunctionDeclaration,
	);
	for (node, want) in cases {
		assert_eq!(a.kind(node), want, "kind of {:?}", a.data(node));
		let clone = a.shallow_clone(node);
		assert!(clone != node && a.kind(clone) == want, "{want} shallow_clone");
		assert!(!matches!(a.data(clone), NodeData::None), "{want} clone keeps its data");
	}
}
