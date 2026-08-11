package flamework

import (
	"strings"
	"testing"

	"rotor/tsgo/printer"
)

func TestTransformFlameworkClass_stripsAndLowersAccessorDecoratorsInSourceOrder(t *testing.T) {
	// Given
	state, file, plan := newDecoratorTransformFixture(t, strings.Join([]string{
		`type FW = MethodDecorator & { _flamework_Decorator: "Class" };`,
		`declare const Decorate: (label: string) => FW;`,
		`class Counter {`,
		` @Decorate("get") get value(): number { return 1 }`,
		` @Decorate("set") set value(next: number) { void next }`,
		`}`,
	}, "\n"), ClassPlan{InternalID: "fixture:out/service@Counter", Decorators: []DecoratorPlan{{Name: "Decorate", InternalID: "fixture:out/service@Decorate"}}})
	classNode := findNamedClass(t, file.AsNode(), "Counter")

	// When
	transformed, err := transformFlameworkClass(state, plan, classNode)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkClass() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(
		state.factory.UpdateSourceFile(file, state.factory.NewNodeList(replaceClassSyntaxList(file, classNode, transformed)), file.EndOfFileToken).AsSourceFile(),
	)
	if strings.Contains(printed, "\n    @Decorate") || strings.Count(printed, `Reflect["decorate"]`) != 2 {
		t.Fatalf("accessor decorator lowering mismatch:\n%s", printed)
	}
	getIndex := strings.Index(printed, `["get"], "value", false`)
	setIndex := strings.Index(printed, `["set"], "value", false`)
	if getIndex < 0 || setIndex <= getIndex {
		t.Fatalf("accessor decorator order mismatch:\n%s", printed)
	}
}
