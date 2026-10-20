package main

import (
	"fmt"
	"slices"
	"strings"
)

const (
	residentSynthetic  = "invented-synthetic"
	residentIncomplete = "incomplete-unmeasured"
	residentUnverified = "external-unverified"
	residentCost       = "semantic-cost"
	residentAssumed    = "synthetic-assumed"
	residentRecorded   = "admitted-recorded"
	residentDeclared   = "root-declared"
	residentPolicySHA  = "27053c5e368cd98e22911c87cab9135a0fbc9f40857d38e006a3343301231c0d"
)

// ResidentKey describes the frozen statistical population, not a raw collector
// schema or a source/fixture seal. Role is checked against the policy inventory.
type ResidentKey struct {
	Comparator string `json:"comparator"`
	Scenario   string `json:"scenario"`
	Settings   string `json:"settings"`
	Role       string `json:"role"`
}

func (k ResidentKey) policyRole() (string, bool) {
	original := []string{"API_L32F32", "API_L1F128", "Body_L32F32", "Rewrite_L32F32", "Invalidate_L32F32", "EmptyDrain_L32F32", "Chain32", "API_X1", "API_X10"}
	mixed := []string{"MixedAPI_M0", "MixedBody_M0", "MixedInvalidate_M0", "MixedAPI_M1", "MixedBody_M1", "MixedInvalidate_M1"}
	if k.Settings != "C1B1" && k.Settings != "C4B4" {
		return "", false
	}
	if !slices.Contains([]string{"B:S", "B:C", "S:C"}, k.Comparator) ||
		(!slices.Contains(original, k.Scenario) && (!slices.Contains(mixed, k.Scenario) || k.Comparator == "B:S")) {
		return "", false
	}
	if k.Comparator != "S:C" && (k.Scenario == "API_L32F32" || k.Scenario == "API_L1F128" || strings.HasPrefix(k.Scenario, "MixedAPI_")) {
		return residentCost, true
	}
	if k.Comparator != "S:C" && k.Scenario == "Chain32" {
		if k.Settings == "C1B1" {
			return "primary", true
		}
		return "secondary", true
	}
	if k.Comparator == "S:C" {
		if k.Scenario == "MixedAPI_M1" && k.Settings == "C1B1" {
			return "primary", true
		}
		if k.Scenario == "MixedAPI_M0" && k.Settings == "C4B4" {
			return "secondary", true
		}
	}
	return "control", true
}

func (k ResidentKey) ratioOnlyTail() bool {
	return strings.HasPrefix(k.Scenario, "Rewrite_") || strings.HasPrefix(k.Scenario, "Invalidate_") ||
		strings.HasPrefix(k.Scenario, "EmptyDrain_") || strings.HasPrefix(k.Scenario, "MixedInvalidate_")
}

// Pointers distinguish absent required native measurements from measured zero.
// CPU components are unsigned native 100 ns ticks; their sum can exceed uint64.
type ResidentResources struct {
	UserTicks   *uint64    `json:"user_ticks_100ns"`
	KernelTicks *uint64    `json:"kernel_ticks_100ns"`
	PeakBytes   *uint64    `json:"peak_job_commit_bytes"`
	IO          [6]*uint64 `json:"io"` // read/write/other ops, read/write/other bytes
}

type ResidentCycle struct {
	Position    int    `json:"position"`
	Nanoseconds uint64 `json:"nanoseconds"`
}

type ResidentSession struct {
	ID        string            `json:"id"`
	Launch    int               `json:"launch"` // one-based chronological position within batch
	Pair      int               `json:"pair"`   // one-based AB,BA,AB,BA... pair
	Order     string            `json:"order"`
	Arm       string            `json:"arm"` // A reference, B comparison; BA launches B first
	Semantic  string            `json:"semantic"`
	Cycles    []ResidentCycle   `json:"cycles"`
	Resources ResidentResources `json:"resources"`
}

type ResidentBlock struct {
	Position int               `json:"position"`
	Sessions []ResidentSession `json:"sessions"` // exactly four, in A B B A launch order
}

// Typed consistency is distinct from raw admission. Recorded data additionally
// require the reader's explicit declaration binding; neither label attests facts.
type ResidentBatch struct {
	Label      string          `json:"label"`
	ID         string          `json:"id"`
	Chronology int             `json:"chronology"` // caller's positive campaign journal ordinal
	Key        ResidentKey     `json:"key"`
	Purpose    string          `json:"purpose"` // calibration or comparison
	Stage      string          `json:"stage"`   // initial, follow_up or confirmation
	Blocks     []ResidentBlock `json:"blocks"`
}

func residentTypedIssues(b ResidentBatch) []issue {
	var issues []issue
	add := func(kind, path, message string) { issues = append(issues, issue{kind, path, message}) }
	role, ok := b.Key.policyRole()
	if !ok || role != b.Key.Role {
		add(invalidEvidence, "key", "population/role is outside the frozen comparator inventory")
	}
	if b.Label != residentSynthetic && b.Label != residentRecorded {
		add(invalidEvidence, "label", "explicit synthetic or admitted-recorded typed label required")
	}
	if b.ID == "" || len(b.ID) > 256 || b.Chronology <= 0 {
		add(invalidEvidence, "identity", "bounded batch identity and positive chronology required")
	}
	if b.Purpose != "calibration" && b.Purpose != "comparison" {
		add(invalidEvidence, "purpose", "calibration or comparison required")
	}
	want := 5
	switch b.Stage {
	case "follow_up":
		want = 10
	case "initial", "confirmation":
	default:
		add(invalidEvidence, "stage", "unknown stage")
	}
	if b.Purpose == "calibration" && b.Stage == "confirmation" {
		add(invalidEvidence, "stage", "calibration has no confirmation stage")
	}
	if len(b.Blocks) != want {
		kind := invalidEvidence
		if len(b.Blocks) < want {
			kind = residentIncomplete
		}
		add(kind, "blocks", fmt.Sprintf("require exactly %d complete ABBA blocks", want))
	}
	// Bound traversal even for a malformed oversized typed input. It is retained
	// by the caller/report, but cannot cause unbounded statistical work.
	if len(b.Blocks) > 10 {
		return issues
	}
	seen := map[string]bool{}
	for i, block := range b.Blocks {
		path := fmt.Sprintf("blocks[%d]", i)
		if block.Position != i+1 || len(block.Sessions) != 4 {
			add(invalidEvidence, path, "block position or four-process ABBA count is inconsistent")
		}
		if len(block.Sessions) != 4 {
			continue
		}
		for j, s := range block.Sessions {
			p := fmt.Sprintf("%s.sessions[%d]", path, j)
			order, arm := "AB", "A"
			if j >= 2 {
				order = "BA"
			}
			if j == 1 || j == 2 {
				arm = "B"
			}
			if s.ID == "" || len(s.ID) > 256 || seen[s.ID] || s.Launch != 4*i+j+1 || s.Pair != 2*i+j/2+1 || s.Order != order || s.Arm != arm {
				add(invalidEvidence, p, "session identity, chronology or paired arm/order is inconsistent")
			}
			seen[s.ID] = true
			semantic := "same-output"
			if role == residentCost && b.Purpose == "comparison" {
				semantic = "expected-stale-B"
			}
			if s.Semantic != semantic {
				add(invalidEvidence, p+".semantic", "unexpected semantic admission; never a statistical retry")
			}
			if len(s.Cycles) != 30 {
				add(invalidEvidence, p+".cycles", "exactly 30 matched positions required")
			}
			if len(s.Cycles) <= 30 {
				for c, cycle := range s.Cycles {
					if cycle.Position != c+1 || cycle.Nanoseconds == 0 {
						add(invalidEvidence, p+".cycles", "positive integer ns and original position order required")
					}
				}
			}
			missing := s.Resources.UserTicks == nil || s.Resources.KernelTicks == nil || s.Resources.PeakBytes == nil
			for _, value := range s.Resources.IO {
				missing = missing || value == nil
			}
			if missing {
				add(residentIncomplete, p+".resources", "all whole-session native CPU, peak and six IO counters required")
			}
		}
	}
	return issues
}

func residentIssueStatus(issues []issue) string {
	status := criterionMet
	for _, i := range issues {
		if i.Kind == invalidEvidence {
			return invalidEvidence
		}
		status = residentIncomplete
	}
	return status
}
