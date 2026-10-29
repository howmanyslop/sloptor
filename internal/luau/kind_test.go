package luau

import "testing"

func TestKindOrdering(t *testing.T) {
	// category ranges mirror upstream enums.ts, with rotor's extensions
	// appended after each upstream category's last kind
	if FirstIndexableExpression != KindIdentifier || LastIndexableExpression != KindParenthesizedExpression {
		t.Error("indexable expression range wrong")
	}
	// LastExpression is KindCastExpression, appended after upstream's last
	// expression kind (KindMixedTable).
	if FirstExpression != KindIdentifier || LastExpression != KindCastExpression {
		t.Error("expression range wrong")
	}
	// LastStatement is KindTypeFunctionDeclaration; the type statements
	// follow upstream's last statement kind (KindComment).
	if FirstStatement != KindAssignment || LastStatement != KindTypeFunctionDeclaration {
		t.Error("statement range wrong")
	}
	if FirstField != KindMapField || LastField != KindInterpolatedStringPart {
		t.Error("field range wrong")
	}
	if FirstType != KindTypeReference || LastType != KindTypeRaw {
		t.Error("type range wrong")
	}
	if FirstTypeField != KindTypeTableProp || LastTypeField != KindTypeTableIndexer {
		t.Error("type field range wrong")
	}
	if LastField >= FirstType || LastType >= FirstTypeField {
		t.Error("type ranges must follow the field range")
	}
}

func TestKindName(t *testing.T) {
	cases := map[SyntaxKind]string{
		KindIdentifier:              "Identifier",
		KindParenthesizedExpression: "ParenthesizedExpression",
		KindSet:                     "Set",
		KindCastExpression:          "CastExpression",
		KindAssignment:              "Assignment",
		KindComment:                 "Comment",
		KindTypeAlias:               "TypeAlias",
		KindTypeFunctionDeclaration: "TypeFunctionDeclaration",
		KindMapField:                "MapField",
		KindInterpolatedString:      "InterpolatedString",
		KindNumericForStatement:     "NumericForStatement",
		KindTypeReference:           "TypeReference",
		KindTypeRaw:                 "TypeRaw",
		KindTypeTableIndexer:        "TypeTableIndexer",
	}
	for k, want := range cases {
		if got := k.String(); got != want {
			t.Errorf("kind %d String() = %q, want %q", k, got, want)
		}
	}
	for k := SyntaxKind(0); k <= LastTypeField; k++ {
		if k.String() == "" {
			t.Errorf("kind %d has no name", k)
		}
	}
}
