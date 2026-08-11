package flamework

import (
	"errors"
	"testing"
)

func TestValidateDependencyCycles_returnsTypedError_whenConstructorGraphIsCircular(t *testing.T) {
	// Given
	classes := []dependencyClass{
		{internalID: "game:out/a@A", dependencies: []string{"game:out/b@B"}},
		{internalID: "game:out/b@B", dependencies: []string{"game:out/a@A"}},
	}

	// When
	err := validateDependencyCycles(classes)

	// Then
	if !errors.Is(err, ErrCircularDependency) {
		t.Fatalf("validateDependencyCycles() error = %v, want ErrCircularDependency", err)
	}
	var circular *CircularDependencyError
	if !errors.As(err, &circular) {
		t.Fatalf("validateDependencyCycles() error type = %T, want *CircularDependencyError", err)
	}
	want := []string{"game:out/a@A", "game:out/b@B", "game:out/a@A"}
	if len(circular.Path) != len(want) {
		t.Fatalf("circular path = %#v, want %#v", circular.Path, want)
	}
	for index := range want {
		if circular.Path[index] != want[index] {
			t.Fatalf("circular path = %#v, want %#v", circular.Path, want)
		}
	}
}
