package flamework

import (
	"fmt"
	"strings"

	"rotor/tsgo/ast"
)

const (
	// flameworkModulePrefix matches every published Flamework package.
	flameworkModulePrefix = "@flamework/"
	// flameworkMarkerPrefix is shared by every marker property the transforms
	// key on: _flamework_macro_*, _flamework_intrinsic, _flamework_Decorator,
	// and _flamework_key_obfuscation.
	flameworkMarkerPrefix = "_flamework"
	// flameworkMetadataTag marks macro and intrinsic declarations that carry no
	// marker property, such as `/** @metadata macro {@link config intrinsic } */`.
	flameworkMetadataTag = "@metadata"
)

// Distinctive Flamework API / macro names. Generic names like id() stay
// out; those still resolve when the file declares or imports surface.
var flameworkMacroCalleeNames = map[string]bool{
	"Flamework":      true,
	"Modding":        true,
	"Reflect":        true,
	"Networking":     true,
	"createGuard":    true,
	"createEvent":    true,
	"createServer":   true,
	"createClient":   true,
	"inspect":        true,
	"declarationUid": true,
}

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
	// Most project sources have no Flamework classes, macros, or APIs. Walking
	// every call expression through the type checker on those files dominated
	// clean full-build time; skip the expression visitor when no surface exists.
	needsExpressions := planned || sourceNeedsFlameworkExpressionTransform(state, sourceFile)
	if !planned && !needsExpressions {
		return sourceFile, nil
	}
	expressionTransformed := sourceFile
	if needsExpressions {
		expressionTransformed, err = transformFlameworkExpressionsInSourceFileWithRuntime(state, sourceFile, state.MacroRuntime())
		if err != nil {
			return nil, fmt.Errorf("transform Flamework expressions: %w", err)
		}
	}
	ast.SetParentInChildren(expressionTransformed.AsNode())
	defer ast.SetParentInChildren(sourceFile.AsNode())
	if !planned {
		if expressionTransformed == sourceFile {
			return sourceFile, nil
		}
		transformed := deduplicateSourceImports(state.factory, sourceFile, expressionTransformed)
		if transformed == sourceFile {
			return sourceFile, nil
		}
		return state.factory.DeepCloneReparse(transformed.AsNode()).AsSourceFile(), nil
	}
	classTransformed := false
	var classErr error
	var visitor *ast.NodeVisitor
	visitor = ast.NewNodeVisitor(func(node *ast.Node) *ast.Node {
		visited := visitor.VisitEachChild(node)
		if classErr != nil || !ast.IsClassDeclaration(visited) || visited.Name() == nil {
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
	transformed := deduplicateSourceImports(state.factory, sourceFile, visited)
	if transformed == sourceFile {
		return sourceFile, nil
	}
	return state.factory.DeepCloneReparse(transformed.AsNode()).AsSourceFile(), nil
}

// sourceNeedsFlameworkExpressionTransform is a cheap prefilter for the full
// expression visitor. Upstream visits every file, so a false negative silently
// drops macros, key obfuscation, and the diagnostics they raise; prefer
// over-inclusion.
func sourceNeedsFlameworkExpressionTransform(state *TransformState, sourceFile *ast.SourceFile) bool {
	if sourceFile == nil {
		return false
	}
	if sourceDeclaresFlameworkExpressionSurface(state, sourceFile) {
		return true
	}
	if state != nil && state.flameworkExpressionSurfaces != nil {
		if state.flameworkExpressionSurfaces[sourceFile.FileName()] {
			return true
		}
	}
	return false
}

// callMayBeFlameworkMacro is a cheap filter before GetResolvedSignature.
// False negatives drop macros and their diagnostics; prefer over-inclusion.
func callMayBeFlameworkMacro(state *TransformState, node *ast.Node) bool {
	if node == nil {
		return false
	}
	if len(node.TypeArguments()) > 0 {
		return true
	}
	if calleeMayBeFlameworkMacro(node.Expression()) {
		return true
	}
	sourceFile := ast.GetSourceFileOfNode(node)
	if sourceFile == nil {
		return false
	}
	if sourceDeclaresFlameworkExpressionSurface(state, sourceFile) {
		return true
	}
	if state != nil && state.flameworkExpressionSurfaces != nil {
		if state.flameworkExpressionSurfaces[sourceFile.FileName()] {
			return true
		}
	}
	return false
}

// sourceMayNeedFlameworkAccessRewrite is the access/assignment counterpart of
// callMayBeFlameworkMacro. Ordinary `.foo` / `obj[key]` must not pay
// GetTypeAtLocation just because the file was admitted for a macro call.
func sourceMayNeedFlameworkAccessRewrite(state *TransformState, sourceFile *ast.SourceFile) bool {
	if sourceFile == nil {
		return false
	}
	if sourceDeclaresFlameworkExpressionSurface(state, sourceFile) {
		return true
	}
	if state != nil && state.flameworkExpressionSurfaces != nil {
		if state.flameworkExpressionSurfaces[sourceFile.FileName()] {
			return true
		}
	}
	return false
}

func calleeMayBeFlameworkMacro(expression *ast.Node) bool {
	current := ast.SkipParentheses(expression)
	for current != nil {
		switch {
		case ast.IsIdentifier(current):
			return flameworkMacroCalleeNames[current.Text()]
		case ast.IsPropertyAccessExpression(current):
			if name := current.Name(); name != nil && flameworkMacroCalleeNames[name.Text()] {
				return true
			}
			current = ast.SkipParentheses(current.Expression())
		case ast.IsElementAccessExpression(current):
			// Dynamic callees (obj[key]()) can still resolve to a Flamework
			// intrinsic; signature resolution is the only safe check.
			return true
		default:
			return false
		}
	}
	return false
}

// sourceDeclaresFlameworkSurface reports whether the file itself declares
// Flamework surface: an `@flamework/*` import, one of the `_flamework_*` marker
// properties (macro, intrinsic, key obfuscation, decorator), or a `@metadata`
// JSDoc tag marking a macro or intrinsic declaration.
func sourceDeclaresFlameworkSurface(state *TransformState, sourceFile *ast.SourceFile) bool {
	if sourceFile == nil {
		return false
	}
	if cached, found := state.flameworkSurface(sourceFile); found {
		return cached
	}
	declares := false
	for _, imp := range sourceFile.Imports() {
		if imp == nil {
			continue
		}
		if strings.HasPrefix(imp.Text(), flameworkModulePrefix) {
			declares = true
			break
		}
	}
	if !declares {
		text := sourceFile.Text()
		declares = strings.Contains(text, flameworkMarkerPrefix) || strings.Contains(text, flameworkMetadataTag)
	}
	state.setFlameworkSurface(sourceFile, declares)
	return declares
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

// sourceDeclaresFlameworkExpressionSurface reports whether the file itself
// triggers expression transforms by direct declaration or textual surface.
// This factors the non-import checks that were previously inlined in the three
// prefilters so the reachability index can seed from a single predicate.
func sourceDeclaresFlameworkExpressionSurface(state *TransformState, sourceFile *ast.SourceFile) bool {
	if sourceFile == nil {
		return false
	}
	if sourceDeclaresFlameworkSurface(state, sourceFile) {
		return true
	}
	text := sourceFile.Text()
	if text == "" {
		return false
	}
	if strings.Contains(text, "Flamework") ||
		strings.Contains(text, "Modding") ||
		strings.Contains(text, "Reflect.") ||
		strings.Contains(text, ".attributes") {
		return true
	}
	for name := range flameworkMacroCalleeNames {
		if strings.Contains(text, name) {
			return true
		}
	}
	return false
}

// initializeFlameworkExpressionSurfaces populates the cached reachability
// index once per state using SourceFile.Imports() (includes type-only) and
// resolved modules. Seeds from direct expression surfaces, walks reverse
// "imported-by" edges with FIFO+marked to cover transitive + diamonds/cycles.
// Non-external (global/ambient) surface seed marks the entire program.
func initializeFlameworkExpressionSurfaces(state *TransformState) {
	if state == nil || state.program == nil || state.flameworkExpressionSurfaces != nil {
		return
	}
	state.flameworkExpressionSurfaces = make(map[string]bool)

	reverse := make(map[string][]string)
	all := state.program.GetSourceFiles()
	fileByName := make(map[string]*ast.SourceFile, len(all))
	for _, f := range all {
		if f == nil {
			continue
		}
		fn := f.FileName()
		fileByName[fn] = f
		for _, spec := range f.Imports() {
			if spec == nil {
				continue
			}
			res := state.program.GetResolvedModuleFromModuleSpecifier(f, spec)
			if !res.IsResolved() {
				continue
			}
			imp := res.ResolvedFileName
			if imp == "" || imp == fn {
				continue
			}
			reverse[imp] = append(reverse[imp], fn)
		}
	}

	// seed
	queue := make([]string, 0, len(all))
	marked := make(map[string]bool)
	hasGlobal := false
	for _, seed := range all {
		if seed == nil {
			continue
		}
		sfn := seed.FileName()
		if sourceDeclaresFlameworkExpressionSurface(state, seed) {
			if !marked[sfn] {
				marked[sfn] = true
				state.flameworkExpressionSurfaces[sfn] = true
				queue = append(queue, sfn)
			}
			if !ast.IsExternalModule(seed) {
				if !strings.HasPrefix(sfn, "bundled:") && !strings.Contains(sfn, "/libs/") {
					hasGlobal = true
				}
			}
		}
	}

	if hasGlobal {
		for fn := range fileByName {
			state.flameworkExpressionSurfaces[fn] = true
		}
		return
	}

	// BFS over reverse importers
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		for _, imp := range reverse[cur] {
			if !marked[imp] {
				marked[imp] = true
				state.flameworkExpressionSurfaces[imp] = true
				queue = append(queue, imp)
			}
		}
	}
}
