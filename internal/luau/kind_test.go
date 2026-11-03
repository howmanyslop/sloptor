package luau

import "testing"

func TestKindOrdering(t *testing.T) {
	// category ranges must mirror upstream enums.ts exactly
	if FirstIndexableExpression != KindIdentifier || LastIndexableExpression != KindParenthesizedExpression {
		t.Error("indexable expression range wrong")
	}
	if FirstExpression != KindIdentifier || LastExpression != KindMixedTable {
		t.Error("expression range wrong")
	}
	// LastStatement is KindTypeAlias, a rotor extension appended after
	// upstream's last statement kind (KindComment).
	if FirstStatement != KindAssignment || LastStatement != KindTypeAlias {
		t.Error("statement range wrong")
	}
	if FirstField != KindMapField || LastField != KindInterpolatedStringPart {
		t.Error("field range wrong")
	}
}

func TestKindName(t *testing.T) {
	cases := map[SyntaxKind]string{
		KindIdentifier:              "Identifier",
		KindParenthesizedExpression: "ParenthesizedExpression",
		KindSet:                     "Set",
		KindAssignment:              "Assignment",
		KindComment:                 "Comment",
		KindTypeAlias:               "TypeAlias",
		KindMapField:                "MapField",
		KindInterpolatedString:      "InterpolatedString",
		KindNumericForStatement:     "NumericForStatement",
	}
	for k, want := range cases {
		if got := k.String(); got != want {
			t.Errorf("kind %d String() = %q, want %q", k, got, want)
		}
	}
}
