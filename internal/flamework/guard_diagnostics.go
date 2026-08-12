package flamework

import (
	"errors"
	"fmt"
	"strings"
)

var ErrUnsupportedGuardType = errors.New("unsupported Flamework guard type")

type GuardGenerationError struct {
	TypeName           string
	Reason             string
	FileName           string
	Start              int
	End                int
	Path               []string
	RelatedInformation []GuardRelatedInformation
}

type GuardRelatedInformation struct {
	TypeName string
	FileName string
	Start    int
	End      int
}

func (e *GuardGenerationError) Error() string {
	if e.Reason == "template literal types are unsupported" {
		return fmt.Sprintf("Flamework encountered a template literal which is unsupported: %s", e.TypeName)
	}
	if e.Reason == "could not find constraint of type parameter" {
		return e.Reason
	}
	if e.Reason == "unknown type has no symbol" {
		return fmt.Sprintf("An unknown type was encountered with no symbol: %s", e.TypeName)
	}
	if e.Reason == "intersection between nominal types is forbidden" {
		return "Intersection between nominal types is forbidden."
	}
	if strings.HasPrefix(e.Reason, "class ") && strings.HasSuffix(e.Reason, " is unsupported") {
		return fmt.Sprintf("Class %q was encountered. Flamework does not support generating guards for classes.", e.TypeName)
	}
	if len(e.Path) == 0 {
		return fmt.Sprintf("generate guard for %s: %s", e.TypeName, e.Reason)
	}
	return fmt.Sprintf("generate guard for %s: %s (type path: %v)", e.TypeName, e.Reason, e.Path)
}

func (e *GuardGenerationError) Is(target error) bool { return target == ErrUnsupportedGuardType }
