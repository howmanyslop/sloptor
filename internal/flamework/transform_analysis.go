package flamework

import (
	"fmt"
	"path/filepath"
	"strings"

	"rotor/tsgo/ast"
)

type classTransformAnalysis struct {
	name                    string
	internalID              string
	identifier              string
	metadata                Metadata
	decorators              []classDecoratorAnalysis
	parameterIDs            []string
	implementedIDs          []string
	containsLegacyDecorator bool
}

type classDecoratorAnalysis struct {
	name          string
	internalID    string
	expression    *ast.Node
	arguments     []*ast.Node
	prerequisites []*ast.Node
	imports       []MacroImport
}

type identifierTransformInput struct {
	internalID string
	name       string
	node       *ast.Node
}

type FlameworkClassAnalysis struct {
	files       []FileAnalysis
	classes     []DiscoveredFlameworkClass
	diagnostics []*ast.Diagnostic
}

type DiscoveredFlameworkClass struct {
	InternalID         string
	DecoratorIDs       []string
	ConstructorTypeIDs []string
}

func (a FlameworkClassAnalysis) Files() []FileAnalysis {
	files := make([]FileAnalysis, len(a.files))
	for index, file := range a.files {
		files[index] = FileAnalysis{FileID: file.FileID, Metadata: NewMetadata(file.Metadata.Tokens()), Classes: cloneClasses(file.Classes)}
	}
	return files
}

func (a FlameworkClassAnalysis) Classes() []DiscoveredFlameworkClass {
	classes := make([]DiscoveredFlameworkClass, len(a.classes))
	for index, class := range a.classes {
		classes[index] = DiscoveredFlameworkClass{
			InternalID: class.InternalID, DecoratorIDs: append([]string(nil), class.DecoratorIDs...),
			ConstructorTypeIDs: append([]string(nil), class.ConstructorTypeIDs...),
		}
	}
	return classes
}

func (a FlameworkClassAnalysis) Diagnostics() []*ast.Diagnostic {
	return append([]*ast.Diagnostic(nil), a.diagnostics...)
}

func AnalyzeFlameworkClasses(input TransformInput) (FlameworkClassAnalysis, error) {
	return analyzeFlameworkClasses(input, true)
}

func analyzeFlameworkClasses(input TransformInput, includeNativeSafetyDiagnostics bool) (FlameworkClassAnalysis, error) {
	state, err := newTransformState(input, nil)
	if err != nil {
		return FlameworkClassAnalysis{}, err
	}
	result := FlameworkClassAnalysis{}
	dependencies := make([]dependencyClass, 0)
	for _, file := range input.Files {
		classes, discoverErr := discoverSourceClasses(state, file)
		if discoverErr != nil {
			return FlameworkClassAnalysis{}, fmt.Errorf("discover Flamework classes in %q: %w", file.FileName(), discoverErr)
		}
		if len(classes) == 0 {
			continue
		}
		fileID, relativeErr := filepath.Rel(input.Project.RootDirectory(), file.FileName())
		if relativeErr != nil {
			return FlameworkClassAnalysis{}, fmt.Errorf("resolve Flamework source path: %w", relativeErr)
		}
		analysis := FileAnalysis{FileID: filepath.ToSlash(fileID), Classes: make([]ClassPlan, len(classes))}
		for index, class := range classes {
			analysis.Classes[index] = class.plan
			result.classes = append(result.classes, class.class)
			if includeNativeSafetyDiagnostics {
				result.diagnostics = append(result.diagnostics, constraintDiagnostics(state, class)...)
			}
			dependencies = append(dependencies, dependencyClass{internalID: class.class.InternalID, dependencies: class.dependencies})
		}
		result.files = append(result.files, analysis)
	}
	if includeNativeSafetyDiagnostics {
		if err := validateDependencyCycles(dependencies); err != nil {
			return FlameworkClassAnalysis{}, err
		}
	}
	result.diagnostics = orderClassDiagnostics(result.diagnostics)
	return result, nil
}

func analyzeFlameworkClass(state *TransformState, plan FilePlan, node *ast.Node) (classTransformAnalysis, error) {
	if !ast.IsClassDeclaration(node) || node.Name() == nil {
		return classTransformAnalysis{}, fmt.Errorf("%w: expected named class declaration", ErrInvalidClassDeclaration)
	}
	name := node.Name().Text()
	classPlan, found := plannedClassByName(plan, name)
	if !found {
		return classTransformAnalysis{}, fmt.Errorf("%w: %s", ErrUnplannedClass, name)
	}
	identifier, err := identifierForTransform(state, identifierTransformInput{classPlan.InternalID, name, node})
	if err != nil {
		return classTransformAnalysis{}, err
	}
	analysis := classTransformAnalysis{
		name: name, internalID: classPlan.InternalID, identifier: identifier,
		containsLegacyDecorator: classPlan.containsLegacyDecorator,
	}
	analysis.decorators, analysis.metadata, err = analyzeClassDecorators(state, classPlan, node)
	if err != nil {
		return classTransformAnalysis{}, err
	}
	analysis.parameterIDs, err = analyzeConstructorParameters(state, plan, node)
	if err != nil {
		return classTransformAnalysis{}, err
	}
	analysis.implementedIDs, err = analyzeImplementedTypes(state, plan, node)
	if err != nil {
		return classTransformAnalysis{}, err
	}
	return analysis, nil
}

func plannedClassByName(plan FilePlan, name string) (ClassPlan, bool) {
	for _, class := range plan.Classes() {
		if classNameFromInternalID(class.InternalID) == name {
			return class, true
		}
	}
	return ClassPlan{}, false
}

// identifierForTransform mirrors the reference getNodeUid: a UID belongs to the
// file that declares the symbol, never to the file currently being emitted.
// The internalID on the input is only a fallback for nodes whose symbol cannot
// be resolved to a declaration.
func identifierForTransform(state *TransformState, input identifierTransformInput) (string, error) {
	if declaration := declarationFromNode(state, input.node); declaration != nil && declarationFullName(declaration) != "" {
		return declarationUID(state, declaration)
	}
	identifier, err := state.project.Identifier(DeclarationIdentity{
		InternalID:      input.internalID,
		DeclarationName: input.name,
		LuaFileName:     luaFileName(ast.GetSourceFileOfNode(input.node).FileName()),
	})
	if err != nil {
		return "", fmt.Errorf("generate Flamework identifier %q: %w", input.internalID, err)
	}
	return identifier, nil
}

func localInternalID(classInternalID, declarationName string) string {
	at := strings.LastIndexByte(classInternalID, '@')
	if at < 0 {
		return classInternalID
	}
	return classInternalID[:at+1] + declarationName
}
