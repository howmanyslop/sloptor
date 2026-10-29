package luau

import "testing"

func TestCategoryMarkers(t *testing.T) {
	var _ IndexableExpression = (*Identifier)(nil)
	var _ IndexableExpression = (*TemporaryIdentifier)(nil)
	var _ IndexableExpression = (*ComputedIndexExpression)(nil)
	var _ IndexableExpression = (*PropertyAccessExpression)(nil)
	var _ IndexableExpression = (*CallExpression)(nil)
	var _ IndexableExpression = (*MethodCallExpression)(nil)
	var _ IndexableExpression = (*ParenthesizedExpression)(nil)

	var _ Expression = (*NilLiteral)(nil)
	var _ Expression = (*MixedTable)(nil)

	var _ Statement = (*Assignment)(nil)
	var _ Statement = (*Comment)(nil)

	var _ FieldNode = (*MapField)(nil)
	var _ FieldNode = (*InterpolatedStringPart)(nil)

	var _ AnyIdentifier = (*Identifier)(nil)
	var _ AnyIdentifier = (*TemporaryIdentifier)(nil)

	var _ WritableExpression = (*Identifier)(nil)
	var _ WritableExpression = (*TemporaryIdentifier)(nil)
	var _ WritableExpression = (*PropertyAccessExpression)(nil)
	var _ WritableExpression = (*ComputedIndexExpression)(nil)

	var _ HasParameters = (*FunctionExpression)(nil)
	var _ HasParameters = (*FunctionDeclaration)(nil)
	var _ HasParameters = (*MethodDeclaration)(nil)
	var _ HasParameters = (*TypeFunctionDeclaration)(nil)

	var _ Expression = (*CastExpression)(nil)
	var _ Statement = (*TypeAlias)(nil)
	var _ Statement = (*TypeFunctionDeclaration)(nil)

	var _ TypeNode = (*TypeReference)(nil)
	var _ TypeNode = (*TypeStringSingleton)(nil)
	var _ TypeNode = (*TypeBooleanSingleton)(nil)
	var _ TypeNode = (*TypeOptional)(nil)
	var _ TypeNode = (*TypeUnion)(nil)
	var _ TypeNode = (*TypeIntersection)(nil)
	var _ TypeNode = (*TypeTable)(nil)
	var _ TypeNode = (*TypeArray)(nil)
	var _ TypeNode = (*TypeFunction)(nil)
	var _ TypeNode = (*TypeTypeof)(nil)
	var _ TypeNode = (*TypePack)(nil)
	var _ TypeNode = (*TypeVariadic)(nil)
	var _ TypeNode = (*TypeGenericPack)(nil)
	var _ TypeNode = (*TypeRaw)(nil)

	var _ TypeTableField = (*TypeTableProp)(nil)
	var _ TypeTableField = (*TypeTableIndexer)(nil)
}

// Type nodes are neither statements nor expressions: the renderer indents
// statements, and solveTempIDs reserves identifier names as values.
func TestTypeNodesAreNotStatementsOrExpressions(t *testing.T) {
	for _, node := range []Node{TypeName("x"), NewTypeTableProp(TypeAccessNone, "a", TypeName("x")), NewTypeRaw("x")} {
		if IsStatement(node) || IsExpression(node) || IsField(node) {
			t.Errorf("%T must only be a type node", node)
		}
		if _, ok := node.(Statement); ok {
			t.Errorf("%T must not implement Statement", node)
		}
		if _, ok := node.(Expression); ok {
			t.Errorf("%T must not implement Expression", node)
		}
	}
}

func TestKinds(t *testing.T) {
	if (&Identifier{}).Kind() != KindIdentifier {
		t.Error("Identifier kind")
	}
	if (&InterpolatedStringPart{}).Kind() != KindInterpolatedStringPart {
		t.Error("InterpolatedStringPart kind")
	}
	if (&TypeFunctionDeclaration{}).Kind() != KindTypeFunctionDeclaration || (&CastExpression{}).Kind() != KindCastExpression {
		t.Error("rotor extension kinds")
	}
}
