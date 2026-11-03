package luau

import "strings"

// Type is a Luau type annotation attached to a function parameter or return.
// Name is rendered verbatim; Optional appends `?`.
type Type struct {
	Name     string
	Optional bool
}

// String renders the annotation as Luau source.
func (t *Type) String() string {
	if t.Optional {
		return t.Name + "?"
	}
	return t.Name
}

// SetParameterType annotates a parameter identifier. The annotation lives on
// the identifier itself because constructors clone nodes that already have a
// parent (a parameter reused in a default-value check, say), and a clone keeps
// the annotation pointer.
func SetParameterType(parameter AnyIdentifier, t *Type) {
	switch id := parameter.(type) {
	case *Identifier:
		id.Annotation = t
	case *TemporaryIdentifier:
		id.Annotation = t
	}
}

// ParameterType returns the annotation set by SetParameterType, if any.
func ParameterType(parameter AnyIdentifier) *Type {
	switch id := parameter.(type) {
	case *Identifier:
		return id.Annotation
	case *TemporaryIdentifier:
		return id.Annotation
	}
	return nil
}

// GenericsString renders a generic parameter list as `<K, V>`, or "" when the
// list is empty.
func GenericsString(generics []string) string {
	if len(generics) == 0 {
		return ""
	}
	return "<" + strings.Join(generics, ", ") + ">"
}
