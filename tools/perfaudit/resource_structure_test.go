package main

import (
	"bytes"
	"fmt"
	"math"
	"testing"
)

func TestResourceNativeStructureLayoutNeighbors(t *testing.T) {
	for _, component := range []string{"JobAccounting", "JobMemory"} {
		for _, size := range []uint64{0, 1, 95, 96, 97, 111, 112, 113, 143, 144, 145, math.MaxInt32, math.MaxInt32 + 1} {
			t.Run(fmt.Sprintf("%s/%d", component, size), func(t *testing.T) {
				m, j, r := resourceAdmissionFixture(t, 10, 20)
				r = resourceAdmissionNativeMetadata(t, r) // Child GOARCH=amd64 does not forbid a 112-byte collector layout.
				changed := changeResourceJournal(t, j, 1, func(event map[string]any) {
					a := event["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
					a[component].(map[string]any)["Values"].(map[string]any)["StructureBytes"] = size
					// Available required counters stay usable even if optional
					// attribution fails. This must not bypass size validation.
					a["Status"] = "partial"
					a["OtherOwnedCpu"] = map[string]any{"Status": "failed", "Error": "optional attribution failed", "Values": nil}
				})
				b := admitResourceBatch(m, changed, r, config{cold, .95, 10, 20})
				wantValid := component == "JobAccounting" && size == 96 || component == "JobMemory" && (size == 112 || size == 144)
				if resourceAdmissionHasKind(b, invalidEvidence) == wantValid || !bytes.Equal(b.RawJournal.Bytes, changed) || len(b.Observations) != 122 {
					t.Fatalf("layout neighbor admitted/dropped incorrectly: %+v", b.Issues)
				}
				if wantValid && (len(b.Resources) != 8 || b.ResourceStatus != "complete") {
					t.Fatal("valid required counters lost because optional attribution failed")
				}
			})
		}
		for _, status := range []string{"failed", "unsupported", "unavailable"} {
			t.Run(component+"/"+status, func(t *testing.T) {
				m, j, r := resourceAdmissionFixture(t, 10, 20)
				j = changeResourceJournal(t, j, 1, func(event map[string]any) {
					a := event["result"].(map[string]any)["ResourceAccounting"].(map[string]any)
					a["Status"] = "partial"
					a[component] = map[string]any{"Status": status, "Error": "native getter unavailable", "Values": nil}
					if component == "JobAccounting" {
						a["OtherOwnedCpu"] = map[string]any{"Status": "unavailable", "Error": "job unavailable", "Values": nil}
					}
				})
				b := admitResourceBatch(m, j, r, config{cold, .95, 10, 20})
				if resourceAdmissionHasKind(b, invalidEvidence) || b.ResourceStatus != incompleteResource {
					t.Fatalf("truthful missing snapshot rejected or filled: %+v", b.Issues)
				}
			})
		}
	}
}
