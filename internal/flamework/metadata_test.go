package flamework

import (
	"reflect"
	"testing"
)

func TestMetadataRequests_whenWildcardAndExclusionArePresent(t *testing.T) {
	// Given: upstream metadata tokens with a wildcard and explicit exclusion.
	metadata := NewMetadata([]string{"*", "~guard", "macro"})

	// When: request membership is queried.
	got := []bool{metadata.Requested("guard"), metadata.Requested("macro"), metadata.Requested("constructor")}

	// Then: exclusions win, explicit and wildcard requests remain enabled.
	want := []bool{false, true, true}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Requested = %v, want %v", got, want)
	}
}

func TestMetadataTokens_whenWhitespaceContainsRuns(t *testing.T) {
	// Given: metadata text with the whitespace shapes accepted by upstream.
	metadata := ParseMetadataText("  macro\tconstructor\nproperties  ")

	// When: the immutable token snapshot is requested.
	got := metadata.Tokens()

	// Then: each token is retained once in deterministic order.
	want := []string{"constructor\nproperties", "macro"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Tokens = %v, want %v", got, want)
	}
	got[0] = "mutated"
	if reflect.DeepEqual(metadata.Tokens(), got) {
		t.Fatal("Tokens exposed mutable internal storage")
	}
}

func TestAnalysisPlans_whenInputsAreUnordered(t *testing.T) {
	// Given: independent file analyses in nondeterministic discovery order.
	state := NewAnalysisState()
	inputs := []FileAnalysis{
		{FileID: "src/z.ts", Metadata: NewMetadata([]string{"macro"}), Classes: []ClassPlan{{InternalID: "pkg:z@Z"}}},
		{FileID: "src/a.ts", Metadata: NewMetadata([]string{"constructor"}), Classes: []ClassPlan{{InternalID: "pkg:a@A"}}},
	}

	// When: serial analysis freezes the file plans.
	plans, err := state.Analyze(inputs)
	if err != nil {
		t.Fatalf("Analyze: %v", err)
	}

	// Then: plans are sorted and returned data cannot mutate cached state.
	if got, want := []string{plans[0].FileID(), plans[1].FileID()}, []string{"src/a.ts", "src/z.ts"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("plan order = %v, want %v", got, want)
	}
	classes := plans[0].Classes()
	classes[0].InternalID = "mutated"
	if got := state.Plans()[0].Classes()[0].InternalID; got != "pkg:a@A" {
		t.Fatalf("cached class plan mutated to %q", got)
	}
}

func TestAnalysisPlans_whenFileIsDuplicated(t *testing.T) {
	// Given: two analysis results for one source file.
	state := NewAnalysisState()
	inputs := []FileAnalysis{{FileID: "src/a.ts"}, {FileID: "src/./a.ts"}}

	// When: serial analysis attempts to freeze both results.
	_, err := state.Analyze(inputs)

	// Then: ambiguous state is rejected.
	if err == nil {
		t.Fatal("Analyze succeeded for duplicate file IDs")
	}
}
