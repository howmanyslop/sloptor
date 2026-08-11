package flamework

import (
	"errors"
	"fmt"
	"sort"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/diagnostics"
)

var (
	ErrInvalidClassDeclaration = errors.New("invalid Flamework class declaration")
	ErrUnplannedClass          = errors.New("unplanned Flamework class")
	ErrInvalidDecorator        = errors.New("invalid Flamework decorator")
	ErrUnplannedDecorator      = errors.New("unplanned Flamework decorator")
	ErrUnsupportedType         = errors.New("unsupported Flamework reflected type")
	ErrCircularDependency      = errors.New("circular Flamework constructor dependency")
)

type CircularDependencyError struct {
	Path []string
}

func (e *CircularDependencyError) Error() string {
	return fmt.Sprintf("%s: %s", ErrCircularDependency, strings.Join(e.Path, " -> "))
}

func (e *CircularDependencyError) Is(target error) bool {
	return target == ErrCircularDependency
}

type dependencyClass struct {
	internalID   string
	dependencies []string
}

func addClassTransformDiagnostic(state *TransformState, node *ast.Node, err error) bool {
	if !errors.Is(err, ErrInvalidDecorator) {
		return false
	}
	state.AddDiagnostic(ast.NewDiagnostic(
		ast.GetSourceFileOfNode(node), nodeRange(node), diagnostics.Decorators_are_not_valid_here,
	))
	return true
}

func validateDependencyCycles(classes []dependencyClass) error {
	graph := make(map[string][]string, len(classes))
	ids := make([]string, 0, len(classes))
	for _, class := range classes {
		ids = append(ids, class.internalID)
		graph[class.internalID] = append([]string(nil), class.dependencies...)
		sort.Strings(graph[class.internalID])
	}
	sort.Strings(ids)
	visiting := make(map[string]int, len(classes))
	visited := make(map[string]bool, len(classes))
	path := make([]string, 0, len(classes)+1)
	var visit func(string) error
	visit = func(internalID string) error {
		if index, found := visiting[internalID]; found {
			cycle := append([]string(nil), path[index:]...)
			cycle = append(cycle, internalID)
			return &CircularDependencyError{Path: cycle}
		}
		if visited[internalID] {
			return nil
		}
		visiting[internalID] = len(path)
		path = append(path, internalID)
		for _, dependency := range graph[internalID] {
			if _, local := graph[dependency]; !local {
				continue
			}
			if err := visit(dependency); err != nil {
				return err
			}
		}
		path = path[:len(path)-1]
		delete(visiting, internalID)
		visited[internalID] = true
		return nil
	}
	for _, internalID := range ids {
		if err := visit(internalID); err != nil {
			return err
		}
	}
	return nil
}
