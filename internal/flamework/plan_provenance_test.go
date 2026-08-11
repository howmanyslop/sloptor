package flamework

import "testing"

func TestClassPlanClone_preservesLegacyDecoratorProvenanceAndDecoratorIsolation(t *testing.T) {
	// Given: a class plan carrying legacy-decorator provenance and mutable decorators.
	original := ClassPlan{
		InternalID:              "fixture:out/service@Service",
		containsLegacyDecorator: true,
		Decorators:              []DecoratorPlan{{Name: "Service", InternalID: "fixture:out/service@Service"}},
	}

	// When: the plan is cloned for an immutable analysis snapshot.
	cloned := cloneClass(original)

	// Then: provenance survives while the cloned decorator slice is independent.
	if !cloned.containsLegacyDecorator {
		t.Fatal("clone lost legacy-decorator provenance")
	}
	cloned.Decorators[0].Name = "mutated"
	if original.Decorators[0].Name != "Service" {
		t.Fatalf("clone shared decorator storage; original name = %q", original.Decorators[0].Name)
	}
}
