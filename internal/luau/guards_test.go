package luau

import "testing"

func TestRangeGuards(t *testing.T) {
	if !IsExpression(ID("x")) || !IsIndexableExpression(ID("x")) {
		t.Error("identifier guards")
	}
	if IsStatement(ID("x")) || IsExpression(NewBreak()) {
		t.Error("category confusion")
	}
	if !IsStatement(NewBreak()) || !IsField(NewMapField(Str("k"), Nil())) {
		t.Error("statement/field guards")
	}
}

func TestCompositeGuards(t *testing.T) {
	if !IsSimple(Str("s")) || !IsSimple(TempID("")) || IsSimple(NewArray(NewList[Expression]())) {
		t.Error("IsSimple")
	}
	if !IsSimplePrimitive(Nil()) || IsSimplePrimitive(ID("x")) {
		t.Error("IsSimplePrimitive")
	}
	if !IsTable(NewSet(NewList[Expression]())) || IsTable(Str("s")) {
		t.Error("IsTable")
	}
	if !IsFinalStatement(NewBreak()) || !IsFinalStatement(NewContinue()) || IsFinalStatement(NewDo(NewList[Statement]())) {
		t.Error("IsFinalStatement")
	}
	c := NewCall(ID("f"), NewList[Expression]())
	if !IsCall(c) || !IsCall(NewMethodCall("m", ID("o"), NewList[Expression]())) {
		t.Error("IsCall")
	}
	if !IsFunctionLike(NewFunctionExpression(NewList[AnyIdentifier](), false, NewList[Statement]())) {
		t.Error("IsFunctionLike")
	}
	if !HasStatements(NewDo(NewList[Statement]())) || HasStatements(NewBreak()) {
		t.Error("HasStatements")
	}
	if !IsExpressionWithPrecedence(NewBinary(ID("a"), "+", ID("b"))) || IsExpressionWithPrecedence(ID("a")) {
		t.Error("IsExpressionWithPrecedence")
	}
}

func TestTypeGuards(t *testing.T) {
	if !IsTypeNode(TypeName("x")) || !IsTypeNode(NewTypeRaw("x")) || IsTypeNode(ID("x")) || IsTypeNode(NewTypeTableProp(TypeAccessNone, "a", TypeName("x"))) {
		t.Error("IsTypeNode")
	}
	if !IsTypePack(NewTypePack(NewList[TypeNode]())) || !IsTypePack(NewTypeVariadic(TypeName("x"))) || !IsTypePack(NewTypeGenericPack("T")) {
		t.Error("IsTypePack must accept every pack shape")
	}
	if IsTypePack(TypeName("x")) || IsTypePack(NewTypeOptional(TypeName("x"))) || IsTypePack(ID("x")) {
		t.Error("IsTypePack must reject types and expressions")
	}
	cast := NewCast(ID("x"), TypeName("T"))
	if !IsExpression(cast) || IsIndexableExpression(cast) || IsStatement(cast) || IsExpressionWithPrecedence(cast) {
		t.Error("CastExpression guards")
	}
	declaration := NewTypeFunctionDeclaration("F", NewList[AnyIdentifier](), false, NewList[Statement]())
	if !IsStatement(declaration) || !IsFunctionLike(declaration) || !HasStatements(declaration) || StatementsOf(declaration) != declaration.Statements {
		t.Error("TypeFunctionDeclaration guards")
	}
	alias := NewTypeAlias("T", nil, TypeName("x"))
	if !IsStatement(alias) || IsFunctionLike(alias) || HasStatements(alias) || StatementsOf(alias) != nil {
		t.Error("TypeAlias guards")
	}
}
