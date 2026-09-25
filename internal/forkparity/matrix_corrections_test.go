package forkparity

import (
	"strings"
	"testing"
)

func TestBehavioralVerificationRejectsUnexecutedTests(t *testing.T) {
	t.Setenv("ROTOR_RANDOMNESS_PATH", "")
	for _, test := range []struct {
		name string
		test string
	}{
		{name: "missing test", test: "TestNoSuchCompatibilityTest"},
		{name: "skipped test", test: "TestRandomnessAcceptance"},
	} {
		t.Run(test.name, func(t *testing.T) {
			err := VerifyBehavioralTest(t.Context(), repoRoot(t), test.test)
			if err == nil || !strings.Contains(err.Error(), "missing pass event") {
				t.Fatalf("unexecuted test verification = %v, want missing pass event", err)
			}
		})
	}
}
