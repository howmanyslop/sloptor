package flamework

import (
	"testing"

	"rotor/tsgo/printer"
)

// A union holding both boolean literals is a full `boolean`. The bare
// `boolean` type takes a fast path in buildUnion, but inside a union the type
// is no longer identical to checker.GetBooleanType() and used to fall through
// to the literal collector, emitting t.literal(false, true).
func Test_buildFlameworkGuard_collapsesBooleanLiteralPairsInsideUnions(t *testing.T) {
	// Given
	state, sourceFile := newGuardTestState(t, `
type OptionalBool = boolean | undefined;
type BoolOrString = boolean | string;
type BoolOrLiteral = boolean | "ready";
type SingleTrue = true | undefined;
type TrueOrString = true | string;
type Shape = { flag?: boolean; name: string };
`)
	tests := []struct {
		name string
		want string
	}{
		{"OptionalBool", `t["optional"](t["boolean"])`},
		// The collapsed boolean leads, matching the reference, which pushes it
		// onto the type list before walking the remaining members.
		{"BoolOrString", `t["union"](t["boolean"], t["string"])`},
		{"BoolOrLiteral", `t["union"](t["boolean"], t["literal"]("ready"))`},
		// One boolean literal is not a boolean.
		{"SingleTrue", `t["optional"](t["literal"](true))`},
		{"TrueOrString", `t["union"](t["string"], t["literal"](true))`},
		{"Shape", `t["interface"]({ "flag": t["optional"](t["boolean"]), "name": t["string"] })`},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// When
			typeNode := guardTestTypeNode(t, sourceFile, test.name)
			guard, err := buildFlameworkGuard(state, state.checker.GetTypeFromTypeNode(typeNode), typeNode)
			// Then
			if err != nil {
				t.Fatalf("buildFlameworkGuard() error = %v", err)
			}
			got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(guard, sourceFile)
			if got != test.want {
				t.Fatalf("guard = %q, want %q", got, test.want)
			}
		})
	}
}
