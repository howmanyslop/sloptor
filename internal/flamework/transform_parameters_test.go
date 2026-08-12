package flamework

import (
	"errors"
	"path/filepath"
	"testing"

	"rotor/internal/config"
)

func TestValidateParameterConstIntrinsic_rejects_spread_at_literal_depth_one(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, `
declare function inspect(value: object): void;
const other = { count: 1 };
inspect({ ...other });`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	calls := collectCallExpressions(file)
	if len(calls) != 1 {
		t.Fatalf("len(calls) = %d, want 1", len(calls))
	}
	signature := state.checker.GetResolvedSignature(calls[0])

	// When
	err := validateParameterConstIntrinsic(calls[0], signature, signature.Parameters())

	// Then
	if !errors.Is(err, ErrMacroDiagnostic) || err.Error() != "Flamework does not support spread expressions in this location." {
		t.Fatalf("validateParameterConstIntrinsic() error = %v", err)
	}
}

func TestValidateParameterConstIntrinsic_accepts_literal_object_at_depth_one(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, `
declare function inspect(value: object): void;
inspect({ nested: dynamicValue });`)
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	call := collectCallExpressions(file)[0]
	signature := state.checker.GetResolvedSignature(call)

	// When
	err := validateParameterConstIntrinsic(call, signature, signature.Parameters())
	// Then
	if err != nil {
		t.Fatalf("validateParameterConstIntrinsic() error = %v", err)
	}
}
