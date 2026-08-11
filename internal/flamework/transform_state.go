package flamework

import (
	"errors"
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/compiler"
	"rotor/tsgo/printer"
)

var (
	ErrInvalidTransformInput = errors.New("invalid Flamework transform input")
	ErrAmbiguousFilePlan     = errors.New("ambiguous Flamework file plan")
)

// TransformInput is the concrete native compiler boundary for Flamework.
type TransformInput struct {
	Program      *compiler.Program
	Checker      *checker.Checker
	Files        []*ast.SourceFile
	Project      *Project
	Analyses     []FileAnalysis
	MacroRuntime *MacroRuntime
}

// SourceMetadata preserves the original source identity alongside its AST replacement.
type SourceMetadata struct {
	fileName    string
	original    *ast.SourceFile
	transformed *ast.SourceFile
	trace       SourceTrace
	emitContext *printer.EmitContext
}

func (m SourceMetadata) FileName() string             { return m.fileName }
func (m SourceMetadata) Original() *ast.SourceFile    { return m.original }
func (m SourceMetadata) Transformed() *ast.SourceFile { return m.transformed }
func (m SourceMetadata) Changed() bool                { return m.original != m.transformed }
func (m SourceMetadata) Trace() SourceTrace           { return m.trace }
func (m SourceMetadata) EmitContext() *printer.EmitContext {
	return m.emitContext
}

// SourceTrace carries the original source identity required to compose later trace maps.
type SourceTrace struct {
	fileName string
	text     string
}

func (t SourceTrace) OriginalFileName() string { return t.fileName }
func (t SourceTrace) OriginalText() string     { return t.text }

// TransformResult contains compiler-ready ASTs and stable diagnostic/source metadata.
type TransformResult struct {
	Files       []*ast.SourceFile
	Plans       []FilePlan
	Diagnostics []*ast.Diagnostic
	Sources     []SourceMetadata
}

type TransformState struct {
	program        *compiler.Program
	checker        *checker.Checker
	project        *Project
	factory        *ast.NodeFactory
	emitContext    *printer.EmitContext
	macroRuntime   MacroRuntime
	plans          []FilePlan
	diagnostics    []*ast.Diagnostic
	generatedNames map[string]map[string]int
	guardLibrary   string
}

func newTransformState(input TransformInput, plans []FilePlan) (*TransformState, error) {
	if input.Program == nil {
		return nil, fmt.Errorf("%w: program is nil", ErrInvalidTransformInput)
	}
	if input.Checker == nil {
		return nil, fmt.Errorf("%w: checker is nil", ErrInvalidTransformInput)
	}
	if input.Project == nil {
		return nil, fmt.Errorf("%w: project is nil", ErrInvalidTransformInput)
	}
	for index, file := range input.Files {
		if file == nil {
			return nil, fmt.Errorf("%w: source file %d is nil", ErrInvalidTransformInput, index)
		}
	}
	macroRuntime := defaultFlameworkMacroRuntime()
	if input.MacroRuntime != nil {
		if input.MacroRuntime.UUID != nil {
			macroRuntime.UUID = input.MacroRuntime.UUID
		}
		if input.MacroRuntime.RandomIndex != nil {
			macroRuntime.RandomIndex = input.MacroRuntime.RandomIndex
		}
		if input.MacroRuntime.BuildGuard != nil {
			macroRuntime.BuildGuard = input.MacroRuntime.BuildGuard
		}
	}
	emitContext := printer.NewEmitContext()
	return &TransformState{
		program:        input.Program,
		checker:        input.Checker,
		project:        input.Project,
		factory:        emitContext.Factory.AsNodeFactory(),
		emitContext:    emitContext,
		macroRuntime:   macroRuntime,
		plans:          clonePlans(plans),
		generatedNames: make(map[string]map[string]int),
	}, nil
}

func (s *TransformState) nextGeneratedName(fileName, preferred string) string {
	if s.generatedNames == nil {
		s.generatedNames = make(map[string]map[string]int)
	}
	names := s.generatedNames[fileName]
	if names == nil {
		names = make(map[string]int)
		s.generatedNames[fileName] = names
	}
	suffix := names[preferred]
	names[preferred] = suffix + 1
	if suffix == 0 {
		return preferred
	}
	return fmt.Sprintf("%s_%d", preferred, suffix)
}

func (s *TransformState) Program() *compiler.Program { return s.program }
func (s *TransformState) Checker() *checker.Checker  { return s.checker }
func (s *TransformState) Project() *Project          { return s.project }
func (s *TransformState) Factory() *ast.NodeFactory  { return s.factory }
func (s *TransformState) EmitContext() *printer.EmitContext {
	return s.emitContext
}
func (s *TransformState) MacroRuntime() MacroRuntime { return s.macroRuntime }
func (s *TransformState) Plans() []FilePlan          { return clonePlans(s.plans) }

func (s *TransformState) AddDiagnostic(diagnostic *ast.Diagnostic) {
	if diagnostic != nil {
		s.diagnostics = append(s.diagnostics, diagnostic)
	}
}

func (s *TransformState) planForFile(sourceFile *ast.SourceFile) (FilePlan, bool, error) {
	fileName := filepath.ToSlash(filepath.Clean(sourceFile.FileName()))
	var matched FilePlan
	matchCount := 0
	for _, plan := range s.plans {
		fileID := filepath.ToSlash(filepath.Clean(filepath.FromSlash(plan.FileID())))
		if fileName == fileID || strings.HasSuffix(fileName, "/"+fileID) {
			matched = plan
			matchCount++
		}
	}
	if matchCount > 1 {
		return FilePlan{}, false, fmt.Errorf("%w: %s", ErrAmbiguousFilePlan, sourceFile.FileName())
	}
	return matched, matchCount == 1, nil
}

func (s *TransformState) orderedDiagnostics() []*ast.Diagnostic {
	diagnostics := append([]*ast.Diagnostic(nil), s.diagnostics...)
	sort.SliceStable(diagnostics, func(left, right int) bool {
		leftFile := diagnosticFileName(diagnostics[left])
		rightFile := diagnosticFileName(diagnostics[right])
		if leftFile != rightFile {
			return leftFile < rightFile
		}
		if diagnostics[left].Pos() != diagnostics[right].Pos() {
			return diagnostics[left].Pos() < diagnostics[right].Pos()
		}
		if diagnostics[left].Code() != diagnostics[right].Code() {
			return diagnostics[left].Code() < diagnostics[right].Code()
		}
		return diagnostics[left].String() < diagnostics[right].String()
	})
	return diagnostics
}
