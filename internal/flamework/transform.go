package flamework

import (
	"fmt"
	"strings"

	"rotor/tsgo/ast"
)

// Transform performs serial project analysis before visiting source files in caller order.
func Transform(input TransformInput) (TransformResult, error) {
	state, err := newTransformState(input, nil)
	if err != nil {
		return TransformResult{}, err
	}
	discovered, err := analyzeFlameworkClasses(input, false)
	if err != nil {
		return TransformResult{}, fmt.Errorf("discover Flamework sources: %w", err)
	}
	analyses, err := mergeTransformAnalyses(input.Analyses, discovered.Files())
	if err != nil {
		return TransformResult{}, err
	}
	plans, err := input.Project.Analyze(analyses)
	if err != nil {
		return TransformResult{}, fmt.Errorf("analyze Flamework sources: %w", err)
	}
	if err := preloadTransformIdentifiers(input.Project, plans); err != nil {
		return TransformResult{}, err
	}
	state.plans = clonePlans(plans)
	for _, diagnostic := range discovered.Diagnostics() {
		state.AddDiagnostic(diagnostic)
	}

	files := make([]*ast.SourceFile, len(input.Files))
	sources := make([]SourceMetadata, len(input.Files))
	for index, sourceFile := range input.Files {
		transformed, transformErr := transformSourceFile(state, sourceFile)
		if transformErr != nil {
			return TransformResult{}, fmt.Errorf("transform Flamework source %q: %w", sourceFile.FileName(), transformErr)
		}
		files[index] = transformed
		sources[index] = SourceMetadata{
			fileName:    sourceFile.FileName(),
			original:    sourceFile,
			transformed: transformed,
			emitContext: state.EmitContext(),
			trace: SourceTrace{
				fileName: sourceFile.FileName(),
				text:     sourceFile.Text(),
			},
		}
	}

	return TransformResult{
		Files:       files,
		Plans:       clonePlans(plans),
		Diagnostics: state.orderedDiagnostics(),
		Sources:     sources,
	}, nil
}

func preloadTransformIdentifiers(project *Project, plans []FilePlan) error {
	for _, plan := range plans {
		for _, class := range plan.Classes() {
			if err := preloadClassIdentifier(project, class, plan.FileID()); err != nil {
				return err
			}
		}
	}
	return nil
}

func preloadClassIdentifier(project *Project, class ClassPlan, fileName string) error {
	name := classNameFromInternalID(class.InternalID)
	if name == "" {
		return nil
	}
	if _, err := project.Identifier(DeclarationIdentity{InternalID: class.InternalID, DeclarationName: name, LuaFileName: luaFileName(fileName)}); err != nil {
		return fmt.Errorf("preload Flamework identifier %q: %w", class.InternalID, err)
	}
	for _, decorator := range class.Decorators {
		if !strings.HasPrefix(decorator.InternalID, project.PackageName()+":") {
			continue
		}
		if _, err := project.Identifier(DeclarationIdentity{InternalID: decorator.InternalID, DeclarationName: decorator.Name, LuaFileName: luaFileName(fileName)}); err != nil {
			return fmt.Errorf("preload Flamework decorator identifier %q: %w", decorator.InternalID, err)
		}
	}
	return nil
}

func transformSourceFile(state *TransformState, sourceFile *ast.SourceFile) (*ast.SourceFile, error) {
	plan, planned, err := state.planForFile(sourceFile)
	if err != nil {
		return nil, err
	}
	expressionTransformed, err := transformFlameworkExpressionsInSourceFileWithRuntime(state, sourceFile, state.MacroRuntime())
	if err != nil {
		return nil, fmt.Errorf("transform Flamework expressions: %w", err)
	}
	ast.SetParentInChildren(expressionTransformed.AsNode())
	classTransformed := false
	var classErr error
	var visitor *ast.NodeVisitor
	visitor = ast.NewNodeVisitor(func(node *ast.Node) *ast.Node {
		visited := visitor.VisitEachChild(node)
		if classErr != nil || !planned || !ast.IsClassDeclaration(visited) || visited.Name() == nil {
			return visited
		}
		classPlan, found, resolveErr := plannedClassForNode(state, plan, visited)
		if resolveErr != nil {
			classErr = resolveErr
			return visited
		}
		if !found {
			return visited
		}
		classOnlyPlan := FilePlan{fileID: plan.fileID, metadata: plan.metadata, classes: []ClassPlan{classPlan}}
		transformed, transformErr := transformFlameworkClass(state, classOnlyPlan, visited)
		if transformErr != nil {
			classErr = transformErr
			return visited
		}
		classTransformed = true
		return transformed
	}, state.factory, ast.NodeVisitorHooks{})
	visited := visitor.VisitSourceFile(expressionTransformed)
	if classErr != nil {
		return nil, fmt.Errorf("transform Flamework class: %w", classErr)
	}
	if classTransformed {
		visited = prependFlameworkReflectImport(state.factory, visited)
	}
	return deduplicateSourceImports(state.factory, sourceFile, visited), nil
}

func mergeTransformAnalyses(explicit, discovered []FileAnalysis) ([]FileAnalysis, error) {
	merged := make([]FileAnalysis, 0, len(explicit)+len(discovered))
	indices := make(map[string]int, len(explicit)+len(discovered))
	characterized := append([]FileAnalysis(nil), explicit...)
	for fileIndex := range characterized {
		characterized[fileIndex].Classes = cloneClasses(characterized[fileIndex].Classes)
		for classIndex := range characterized[fileIndex].Classes {
			characterized[fileIndex].Classes[classIndex].containsLegacyDecorator = true
		}
	}
	for _, analysis := range append(characterized, discovered...) {
		fileID, err := normalizeFileID(analysis.FileID)
		if err != nil {
			return nil, fmt.Errorf("merge Flamework analysis: %w", err)
		}
		index, found := indices[fileID]
		if !found {
			indices[fileID] = len(merged)
			merged = append(merged, FileAnalysis{FileID: fileID, Metadata: NewMetadata(analysis.Metadata.Tokens()), Classes: cloneClasses(analysis.Classes)})
			continue
		}
		current := &merged[index]
		current.Metadata = NewMetadata(append(current.Metadata.Tokens(), analysis.Metadata.Tokens()...))
		for _, class := range analysis.Classes {
			classIndex := -1
			for candidate := range current.Classes {
				if current.Classes[candidate].InternalID == class.InternalID {
					classIndex = candidate
					break
				}
			}
			if classIndex < 0 {
				current.Classes = append(current.Classes, cloneClass(class))
				continue
			}
			current.Classes[classIndex] = mergeTransformClassPlan(current.Classes[classIndex], class)
		}
	}
	return merged, nil
}

func mergeTransformClassPlan(characterized, discovered ClassPlan) ClassPlan {
	result := cloneClass(discovered)
	result.containsLegacyDecorator = characterized.containsLegacyDecorator || discovered.containsLegacyDecorator
	if result.FilePath == "" {
		result.FilePath = characterized.FilePath
	}
	seen := make(map[string]struct{}, len(result.Decorators))
	for _, decorator := range result.Decorators {
		seen[decorator.InternalID] = struct{}{}
	}
	for _, decorator := range characterized.Decorators {
		if _, found := seen[decorator.InternalID]; found {
			continue
		}
		result.Decorators = append(result.Decorators, decorator)
	}
	return result
}

func plannedClassForNode(state *TransformState, plan FilePlan, node *ast.Node) (ClassPlan, bool, error) {
	internalID, err := declarationInternalID(state, node)
	if err != nil {
		return ClassPlan{}, false, fmt.Errorf("resolve planned class %q: %w", node.Name().Text(), err)
	}
	for _, class := range plan.Classes() {
		if class.InternalID == internalID {
			return class, true, nil
		}
	}
	return ClassPlan{}, false, nil
}

func classNameFromInternalID(internalID string) string {
	separator := strings.LastIndexByte(internalID, '@')
	if separator < 0 || separator == len(internalID)-1 {
		return ""
	}
	name := internalID[separator+1:]
	if nested := strings.LastIndexByte(name, '.'); nested >= 0 {
		name = name[nested+1:]
	}
	return name
}

func luaFileName(fileName string) string {
	fileName = strings.TrimSuffix(fileName, ".ts")
	fileName = strings.TrimSuffix(fileName, ".d")
	if separator := strings.LastIndexAny(fileName, `/\\`); separator >= 0 {
		fileName = fileName[separator+1:]
	}
	if fileName == "index" {
		return "init"
	}
	return fileName
}
