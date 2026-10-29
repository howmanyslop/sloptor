package luau

import "testing"

func TestTypeConstructorsAdopt(t *testing.T) {
	check := func(name string, parent Node, children ...Node) {
		t.Helper()
		for _, child := range children {
			if child.Parent() != parent {
				t.Errorf("%s: child %T not adopted", name, child)
			}
		}
	}
	argument := TypeName("A")
	reference := NewTypeReference("Map", NewList[TypeNode](argument))
	check("reference", reference, argument)

	inner := TypeName("number")
	optional := NewTypeOptional(inner)
	check("optional", optional, inner)

	a, b := TypeName("A"), TypeName("B")
	union := NewTypeUnion(NewList[TypeNode](a, b))
	check("union", union, a, b)

	c, d := TypeName("C"), TypeName("D")
	intersection := NewTypeIntersection(NewList[TypeNode](c, d))
	check("intersection", intersection, c, d)

	value := TypeName("number")
	property := NewTypeTableProp(TypeAccessRead, "x", value)
	check("prop", property, value)
	key, indexed := TypeName("string"), TypeName("boolean")
	indexer := NewTypeTableIndexer(TypeAccessNone, key, indexed)
	check("indexer", indexer, key, indexed)
	tableType := NewTypeTable(NewList[TypeTableField](property, indexer))
	check("table", tableType, property, indexer)

	element := TypeName("number")
	array := NewTypeArray(element)
	check("array", array, element)

	parameterType, varArgs, returnType, defaultType := TypeName("T"), NewTypeGenericPack("U"), NewTypePack(NewList[TypeNode]()), TypeName("number")
	function := NewTypeFunction(
		[]TypeGeneric{{Name: "T", Default: defaultType}, {Name: "U", IsPack: true}},
		[]TypeFunctionParameter{{Name: "value", Type: parameterType}},
		varArgs, returnType)
	check("function", function, parameterType, varArgs, returnType, defaultType)
	if function.Generics[0].Default != TypeNode(defaultType) || function.Parameters[0].Type != TypeNode(parameterType) {
		t.Error("function must keep the adopted nodes in place")
	}

	expression := ID("x")
	typeofNode := NewTypeTypeof(expression)
	check("typeof", typeofNode, expression)

	first, rest := TypeName("A"), NewTypeVariadic(TypeName("B"))
	packNode := NewTypePack(NewList[TypeNode](first, rest))
	check("pack", packNode, first, rest)
	check("variadic", rest, rest.Inner)

	castInner, castType := ID("v"), TypeName("Foo")
	cast := NewCast(castInner, castType)
	check("cast", cast, castInner, castType)

	body, aliasDefault := TypeName("number"), TypeName("string")
	alias := NewTypeAlias("T", []TypeGeneric{{Name: "K", Default: aliasDefault}}, body)
	check("alias", alias, body, aliasDefault)
	late := NewTypeAlias("Late", nil, nil)
	if late.Body != nil {
		t.Error("nil body must stay nil")
	}
	lateBody := TypeName("number")
	late.SetBody(lateBody)
	check("set body", late, lateBody)

	parameter := ID("t")
	statement := NewReturn(parameter)
	declaration := NewTypeFunctionDeclaration("F", NewList[AnyIdentifier](parameter), true, NewList[Statement](statement))
	check("type function", declaration, statement)
	if declaration.Parameters.Head.Value.Parent() != Node(declaration) {
		t.Error("type function parameters must be adopted")
	}
	params, hasDotDotDot, varArgsType := declaration.ParamData()
	if params != declaration.Parameters || !hasDotDotDot || varArgsType != nil || declaration.ReturnTypeData() != nil {
		t.Error("type function ParamData")
	}
}

func TestTypeConstructorsCloneParentedChildren(t *testing.T) {
	shared := TypeName("number")
	first := NewTypeOptional(shared)
	second := NewTypeArray(shared)
	if first.Inner != TypeNode(shared) {
		t.Error("first use must keep the original node")
	}
	if second.Element == TypeNode(shared) {
		t.Error("second use must be a clone")
	}
	if second.Element.(*TypeReference).Name != "number" || second.Element.Parent() != Node(second) {
		t.Error("clone must preserve fields and be adopted")
	}
	if shared.Parent() != Node(first) {
		t.Error("original parent untouched")
	}
}

func TestOptionalType(t *testing.T) {
	already := NewTypeOptional(TypeName("A"))
	if OptionalType(already) != TypeNode(already) {
		t.Error("an optional stays as it is")
	}
	for _, name := range []string{"nil", "any", "unknown"} {
		reference := TypeName(name)
		if OptionalType(reference) != TypeNode(reference) {
			t.Errorf("%s must stay unchanged", name)
		}
	}
	number := TypeName("number")
	wrapped, ok := OptionalType(number).(*TypeOptional)
	if !ok || wrapped.Inner != TypeNode(number) {
		t.Error("other types are wrapped")
	}
	qualified := NewTypeQualifiedReference("M", "any", nil)
	if _, ok := OptionalType(qualified).(*TypeOptional); !ok {
		t.Error("a qualified `any` is an ordinary reference")
	}
	generic := NewTypeReference("any", NewList[TypeNode](TypeName("T")))
	if _, ok := OptionalType(generic).(*TypeOptional); !ok {
		t.Error("an instantiated `any` is an ordinary reference")
	}
}

func TestBindingType(t *testing.T) {
	identifier := ID("x")
	if BindingType(identifier) != nil {
		t.Error("unannotated identifier")
	}
	annotation := TypeName("number")
	SetBindingType(identifier, annotation)
	if BindingType(identifier) != TypeNode(annotation) || annotation.Parent() != Node(identifier) {
		t.Error("annotation must be stored and adopted")
	}
	clone := WithOrigin(identifier, &Origin{}).(*Identifier)
	if BindingType(clone) != TypeNode(annotation) {
		t.Error("a clone keeps the annotation pointer")
	}
	SetBindingType(identifier, nil)
	if BindingType(identifier) != nil {
		t.Error("nil clears the annotation")
	}

	temp := TempID("param")
	tempAnnotation := TypeName("Foo")
	SetBindingType(temp, tempAnnotation)
	if BindingType(temp) != TypeNode(tempAnnotation) || tempAnnotation.Parent() != Node(temp) {
		t.Error("temporary identifiers carry annotations too")
	}
	// A parented annotation is cloned on adoption, like any other child.
	SetBindingType(identifier, tempAnnotation)
	if BindingType(identifier) == TypeNode(tempAnnotation) || BindingType(identifier).(*TypeReference).Name != "Foo" {
		t.Error("a parented annotation must be cloned")
	}
}

func TestTypeKinds(t *testing.T) {
	cases := map[Node]SyntaxKind{
		TypeName("x"):                                                     KindTypeReference,
		NewTypeStringSingleton("a"):                                       KindTypeStringSingleton,
		NewTypeBooleanSingleton(true):                                     KindTypeBooleanSingleton,
		NewTypeOptional(TypeName("x")):                                    KindTypeOptional,
		NewTypeUnion(NewList[TypeNode]()):                                 KindTypeUnion,
		NewTypeIntersection(NewList[TypeNode]()):                          KindTypeIntersection,
		NewTypeTable(NewList[TypeTableField]()):                           KindTypeTable,
		NewTypeArray(TypeName("x")):                                       KindTypeArray,
		NewTypeFunction(nil, nil, nil, nil):                               KindTypeFunction,
		NewTypeTypeof(ID("x")):                                            KindTypeTypeof,
		NewTypePack(NewList[TypeNode]()):                                  KindTypePack,
		NewTypeVariadic(TypeName("x")):                                    KindTypeVariadic,
		NewTypeGenericPack("T"):                                           KindTypeGenericPack,
		NewTypeRaw("x"):                                                   KindTypeRaw,
		NewTypeTableProp(TypeAccessNone, "a", TypeName("x")):              KindTypeTableProp,
		NewTypeTableIndexer(TypeAccessNone, TypeName("k"), TypeName("v")): KindTypeTableIndexer,
		NewCast(ID("x"), TypeName("T")):                                   KindCastExpression,
		NewTypeAlias("T", nil, TypeName("x")):                             KindTypeAlias,
		NewTypeFunctionDeclaration("F", NewList[AnyIdentifier](), false, NewList[Statement]()): KindTypeFunctionDeclaration,
	}
	for node, want := range cases {
		if node.Kind() != want {
			t.Errorf("%T kind = %v, want %v", node, node.Kind(), want)
		}
		clone := node.shallowClone()
		if clone == node || clone.Kind() != want {
			t.Errorf("%T shallowClone", node)
		}
	}
}
