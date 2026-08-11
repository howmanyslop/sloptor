package flamework

import (
	"errors"
	"fmt"
	"path/filepath"
	"sort"
	"strings"
)

var (
	ErrDuplicateFile  = errors.New("duplicate Flamework file analysis")
	ErrInvalidFileID  = errors.New("invalid Flamework file ID")
	ErrDuplicateClass = errors.New("duplicate Flamework class")
)

type DecoratorPlan struct {
	Name       string
	InternalID string
}

type ClassPlan struct {
	FilePath                string
	InternalID              string
	Decorators              []DecoratorPlan
	containsLegacyDecorator bool
}

type FileAnalysis struct {
	FileID   string
	Metadata Metadata
	Classes  []ClassPlan
}

type FilePlan struct {
	fileID   string
	metadata Metadata
	classes  []ClassPlan
}

func (p FilePlan) FileID() string {
	return p.fileID
}

func (p FilePlan) Metadata() Metadata {
	return NewMetadata(p.metadata.Tokens())
}

func (p FilePlan) Classes() []ClassPlan {
	return cloneClasses(p.classes)
}

type AnalysisState struct {
	plans      []FilePlan
	classCache map[string]ClassPlan
	macroCache map[string]bool
}

func NewAnalysisState() *AnalysisState {
	return &AnalysisState{
		classCache: make(map[string]ClassPlan),
		macroCache: make(map[string]bool),
	}
}

func (s *AnalysisState) Analyze(inputs []FileAnalysis) ([]FilePlan, error) {
	ordered := append([]FileAnalysis(nil), inputs...)
	for index := range ordered {
		fileID, err := normalizeFileID(ordered[index].FileID)
		if err != nil {
			return nil, err
		}
		ordered[index].FileID = fileID
	}
	sort.Slice(ordered, func(left, right int) bool {
		return ordered[left].FileID < ordered[right].FileID
	})

	plans := make([]FilePlan, 0, len(ordered))
	classCache := make(map[string]ClassPlan)
	for index, input := range ordered {
		if index > 0 && input.FileID == ordered[index-1].FileID {
			return nil, fmt.Errorf("%w: %s", ErrDuplicateFile, input.FileID)
		}
		classes := cloneClasses(input.Classes)
		for classIndex := range classes {
			classes[classIndex].FilePath = input.FileID
			if _, exists := classCache[classes[classIndex].InternalID]; exists {
				return nil, fmt.Errorf("%w: %s", ErrDuplicateClass, classes[classIndex].InternalID)
			}
			classCache[classes[classIndex].InternalID] = cloneClass(classes[classIndex])
		}
		plans = append(plans, FilePlan{
			fileID:   input.FileID,
			metadata: NewMetadata(input.Metadata.Tokens()),
			classes:  classes,
		})
	}

	s.plans = plans
	s.classCache = classCache
	return clonePlans(plans), nil
}

func (s *AnalysisState) Plans() []FilePlan {
	return clonePlans(s.plans)
}

func (s *AnalysisState) Class(internalID string) (ClassPlan, bool) {
	class, ok := s.classCache[internalID]
	return cloneClass(class), ok
}

func (s *AnalysisState) CacheUserMacro(symbol string, macro bool) {
	s.macroCache[symbol] = macro
}

func (s *AnalysisState) UserMacro(symbol string) (bool, bool) {
	macro, ok := s.macroCache[symbol]
	return macro, ok
}

func normalizeFileID(fileID string) (string, error) {
	if filepath.IsAbs(fileID) {
		return "", fmt.Errorf("%w: %s", ErrInvalidFileID, fileID)
	}
	cleaned := filepath.ToSlash(filepath.Clean(filepath.FromSlash(fileID)))
	if cleaned == "." || cleaned == ".." || strings.HasPrefix(cleaned, "../") {
		return "", fmt.Errorf("%w: %s", ErrInvalidFileID, fileID)
	}
	return cleaned, nil
}

func clonePlans(plans []FilePlan) []FilePlan {
	cloned := make([]FilePlan, len(plans))
	for index, plan := range plans {
		cloned[index] = FilePlan{
			fileID:   plan.fileID,
			metadata: NewMetadata(plan.metadata.Tokens()),
			classes:  cloneClasses(plan.classes),
		}
	}
	return cloned
}

func cloneClasses(classes []ClassPlan) []ClassPlan {
	cloned := make([]ClassPlan, len(classes))
	for index, class := range classes {
		cloned[index] = cloneClass(class)
	}
	return cloned
}

func cloneClass(class ClassPlan) ClassPlan {
	class.Decorators = append([]DecoratorPlan(nil), class.Decorators...)
	return class
}
