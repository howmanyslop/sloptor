package flamework

import (
	"context"
	"errors"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"rotor/internal/rojo"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tspath"
)

// StaticAddPathsMatches is the compile-time module set of each plain
// addPaths call. It is an incremental input only; it is not written to
// flamework.build or globs.json, which describe runtime addPathsGlob calls.
func StaticAddPathsMatches(program *compiler.Program, project *Project) (map[string][]string, error) {
	if program == nil || project == nil || project.RojoResolver() == nil {
		return nil, nil
	}
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	state := &TransformState{program: program, checker: checker, project: project}
	matches := make(map[string][]string)
	for _, file := range program.GetSourceFiles() {
		if file.IsDeclarationFile || program.IsSourceFileFromExternalLibrary(file) {
			continue
		}
		var visitErr error
		var visit func(*ast.Node) bool
		visit = func(node *ast.Node) bool {
			if visitErr != nil {
				return true
			}
			if ast.IsCallExpression(node) && len(node.Arguments()) > 0 && isAddPathsCallee(node.Expression()) && callMayBeFlameworkMacro(state, node) {
				signature := checker.GetResolvedSignature(node)
				if signature != nil {
					target := flameworkRewriteSymbol(readMacroMetadata(state, signature.Declaration()))
					argumentType := checker.GetTypeAtLocation(node.Arguments()[0])
					if target != nil && target.Parent.Name == "Flamework" && target.Name == "_addPaths" && argumentType != nil && argumentType.IsStringLiteral() {
						pattern, ok := addPathsPattern(state, stringLiteralValue(argumentType))
						if ok {
							clean, err := cleanGlobPattern(pattern)
							if err != nil {
								visitErr = err
								return true
							}
							modules, sources, err := addPathsModules(state, file, clean)
							if err != nil {
								visitErr = err
								return true
							}
							unimportable, err := hasUnimportableModule(state, clean, sources)
							if err != nil {
								visitErr = err
								return true
							}
							if unimportable {
								modules = append(modules, "<runtime-only-module>")
							}
							matches[clean] = append(matches[clean], modules...)
						}
					}
				}
			}
			return node.ForEachChild(visit)
		}
		visit(file.AsNode())
		if visitErr != nil {
			return nil, visitErr
		}
	}
	return matches, nil
}

// expandAddPaths replaces each standalone Flamework.addPaths or addPathsGlob
// call with side-effect imports at the call site. Bundlers can follow those
// static imports while runtime Instance requires are opaque to them. Calls it
// cannot expand keep the runtime rewrite and its diagnostics.
func expandAddPaths(state *TransformState, sourceFile *ast.SourceFile) (*ast.SourceFile, error) {
	if state.project.RojoResolver() == nil {
		return sourceFile, nil
	}
	sourceFile, err := expandTopLevelAddPaths(state, sourceFile)
	if err != nil {
		return nil, err
	}
	var transformErr error
	var visitor *ast.NodeVisitor
	visitor = ast.NewNodeVisitor(func(node *ast.Node) *ast.Node {
		if transformErr != nil {
			return node
		}
		if ast.IsExpressionStatement(node) {
			imports, ok, err := expandAddPathsStatement(state, sourceFile, node)
			if err != nil {
				transformErr = err
				return node
			}
			if ok {
				statements := make([]*ast.Node, len(imports))
				for index, declaration := range imports {
					statements[index] = staticImportCall(state.factory, declaration.AsImportDeclaration().ModuleSpecifier)
				}
				if node.Parent != nil && !ast.IsBlock(node.Parent) && !ast.IsSourceFile(node.Parent) {
					return state.factory.NewBlock(state.factory.NewNodeList(statements), true)
				}
				return state.factory.NewSyntaxList(statements)
			}
		}
		return visitor.VisitEachChild(node)
	}, state.factory, ast.NodeVisitorHooks{})
	transformed := visitor.VisitSourceFile(sourceFile)
	return transformed, transformErr
}

// A nested TypeScript import declaration is syntactically invalid. Use an
// import() expression with a private string suffix; the Luau transformer
// recognizes it and emits an immediate TS.import at this point. The binary
// expression keeps the marker valid under the project's CommonJS module mode.
func staticImportCall(factory *ast.NodeFactory, moduleSpecifier *ast.Node) *ast.Node {
	markedSpecifier := factory.NewBinaryExpression(nil, moduleSpecifier, nil, factory.NewToken(ast.KindPlusToken),
		factory.NewStringLiteral(transformer.FlameworkStaticImportMarker, ast.TokenFlagsNone))
	call := factory.NewCallExpression(factory.NewKeywordExpression(ast.KindImportKeyword), nil, nil,
		factory.NewNodeList([]*ast.Node{markedSpecifier}), ast.NodeFlagsNone)
	return factory.NewExpressionStatement(call)
}

func expandTopLevelAddPaths(state *TransformState, sourceFile *ast.SourceFile) (*ast.SourceFile, error) {
	statements := make([]*ast.Node, 0, len(sourceFile.Statements.Nodes))
	changed := false
	for _, statement := range sourceFile.Statements.Nodes {
		imports, ok, err := expandAddPathsStatement(state, sourceFile, statement)
		if err != nil {
			return nil, err
		}
		if !ok {
			statements = append(statements, statement)
			continue
		}
		changed = true
		statements = append(statements, imports...)
	}
	if !changed {
		return sourceFile, nil
	}
	return state.factory.UpdateSourceFile(sourceFile, state.factory.NewNodeList(statements), sourceFile.EndOfFileToken).AsSourceFile(), nil
}

func expandAddPathsStatement(state *TransformState, file *ast.SourceFile, statement *ast.Node) ([]*ast.Node, bool, error) {
	if !ast.IsExpressionStatement(statement) {
		return nil, false, nil
	}
	call := statement.Expression()
	if !ast.IsCallExpression(call) || len(call.Arguments()) == 0 || !isAddPathsCallee(call.Expression()) || !callMayBeFlameworkMacro(state, call) {
		return nil, false, nil
	}
	signature := state.checker.GetResolvedSignature(call)
	if signature == nil {
		return nil, false, nil
	}
	target := flameworkRewriteSymbol(readMacroMetadata(state, signature.Declaration()))
	if target == nil || target.Parent.Name != "Flamework" {
		return nil, false, nil
	}
	argumentType := state.checker.GetTypeAtLocation(call.Arguments()[0])
	if argumentType == nil || !argumentType.IsStringLiteral() {
		return nil, false, nil
	}
	value := stringLiteralValue(argumentType)
	var pattern string
	var ok bool
	switch target.Name {
	case "_addPaths":
		pattern, ok = addPathsPattern(state, value)
	case "_addPathsGlob":
		pattern, ok = projectGlob(state, file, value)
		if ok {
			// Registered as upstream does: keeps globs.json and the glob-driven
			// incremental salt, so a new match recompiles this file.
			fileID, err := projectRelativePath(state.project.RootDirectory(), file.FileName())
			if err != nil {
				return nil, false, err
			}
			state.project.AddGlob(pattern, fileID)
		}
	}
	if !ok {
		return nil, false, nil
	}
	clean, err := cleanGlobPattern(pattern)
	if err != nil {
		return nil, false, nil
	}
	modules, sources, err := addPathsModules(state, file, clean)
	if err != nil {
		return nil, false, err
	}
	if unimportable, err := hasUnimportableModule(state, clean, sources); err != nil || unimportable {
		return nil, false, err
	}
	imports := make([]*ast.Node, len(modules))
	for index, module := range modules {
		imports[index] = state.factory.NewImportDeclaration(nil, nil, state.factory.NewStringLiteral(module, ast.TokenFlagsNone), nil)
	}
	return imports, true, nil
}

// isAddPathsCallee is a cheap syntactic prefilter so ordinary top-level calls
// skip signature resolution.
func isAddPathsCallee(callee *ast.Node) bool {
	callee = ast.SkipParentheses(callee)
	if !ast.IsPropertyAccessExpression(callee) || callee.Name() == nil {
		return false
	}
	name := callee.Name().Text()
	return name == "addPaths" || name == "addPathsGlob"
}

// addPathsPattern matches the addPaths target and every descendant, like the
// runtime's GetDescendants walk. A target without Rojo data is left to the
// runtime rewrite, which reports it.
func addPathsPattern(state *TransformState, input string) (string, bool) {
	absolute := projectPathInput(state, input)
	if _, ok := pathIntrinsicRbxPath(state, absolute); !ok {
		return "", false
	}
	relative, err := projectRelativePath(state.project.RootDirectory(), absolute)
	if err != nil {
		return "", false
	}
	return path.Join(relative, "**"), true
}

// addPathsModules returns import specifiers, relative to file, for every
// program source whose path or ancestor directory matches pattern and that
// Rojo places as a module (not a Script or LocalScript). It also returns the
// project-relative paths of every program source.
func addPathsModules(state *TransformState, file *ast.SourceFile, pattern string) ([]string, map[string]bool, error) {
	root := state.project.RootDirectory()
	resolver := state.project.RojoResolver()
	translator := state.project.PathTranslator()
	compare := tspath.ComparePathsOptions{UseCaseSensitiveFileNames: state.program.UseCaseSensitiveFileNames()}
	fromDirectory := tspath.GetDirectoryPath(file.FileName())
	type match struct{ relative, module string }
	matches := make([]match, 0)
	sources := make(map[string]bool)
	for _, source := range state.program.GetSourceFiles() {
		if source.IsDeclarationFile || state.program.IsSourceFileFromExternalLibrary(source) {
			continue
		}
		relative, err := projectRelativePath(root, source.FileName())
		if err != nil {
			continue
		}
		sources[relative] = true
		if source == file {
			continue
		}
		matched, err := matchGlobOrAncestor(pattern, relative)
		if err != nil {
			return nil, nil, err
		}
		if !matched {
			continue
		}
		output := translator.GetOutputPath(source.FileName())
		if _, ok := resolver.GetRbxPathFromFilePath(output); !ok {
			continue
		}
		if isScriptRbxType(resolver.GetRbxTypeFromFilePath(output)) {
			continue
		}
		matches = append(matches, match{relative: relative, module: moduleSpecifier(fromDirectory, source.FileName(), compare)})
	}
	sort.Slice(matches, func(i, j int) bool { return matches[i].relative < matches[j].relative })
	modules := make([]string, len(matches))
	for index, match := range matches {
		modules[index] = match.module
	}
	return modules, sources, nil
}

// hasUnimportableModule reports a file on disk that pattern matches and Rojo
// places as a module but that is not a program source, such as a hand-written
// Lua module. TypeScript files excluded by tsconfig emit no output and cannot
// be runtime matches. Only the pattern's literal directory prefix is walked.
func hasUnimportableModule(state *TransformState, pattern string, sources map[string]bool) (bool, error) {
	prefix := globLiteralPrefix(pattern)
	directory := filepath.Join(state.project.RootDirectory(), filepath.FromSlash(prefix))
	resolver := state.project.RojoResolver()
	found := false
	err := fs.WalkDir(os.DirFS(directory), ".", func(name string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil || found || !entry.Type().IsRegular() {
			return walkErr
		}
		relative := path.Join(prefix, name)
		if sources[relative] || !unimportableModuleExtensions[path.Ext(relative)] || strings.HasSuffix(relative, ".d.ts") {
			return nil
		}
		matched, err := matchGlobOrAncestor(pattern, relative)
		if err != nil || !matched {
			return err
		}
		output := state.project.PathTranslator().GetOutputPath(filepath.Join(directory, filepath.FromSlash(name)))
		if _, ok := resolver.GetRbxPathFromFilePath(output); ok && !isScriptRbxType(resolver.GetRbxTypeFromFilePath(output)) {
			found = true
		}
		return nil
	})
	if errors.Is(err, fs.ErrNotExist) {
		return false, nil
	}
	return found, err
}

var unimportableModuleExtensions = map[string]bool{".lua": true, ".luau": true}

// globLiteralPrefix returns the leading directory segments of pattern that
// contain no glob syntax.
func globLiteralPrefix(pattern string) string {
	segments := strings.Split(pattern, "/")
	for index, segment := range segments {
		if segment == "**" || strings.ContainsAny(segment, "*?[\\") || index == len(segments)-1 {
			return path.Join(segments[:index]...)
		}
	}
	return ""
}

func isScriptRbxType(rbxType rojo.RbxType) bool {
	return rbxType == rojo.RbxTypeScript || rbxType == rojo.RbxTypeLocalScript
}

func matchGlobOrAncestor(pattern, relative string) (bool, error) {
	for candidate := relative; candidate != "."; candidate = path.Dir(candidate) {
		if matched, err := matchGlob(pattern, candidate); err != nil || matched {
			return matched, err
		}
	}
	return false, nil
}

func moduleSpecifier(fromDirectory, to string, compare tspath.ComparePathsOptions) string {
	return tspath.EnsurePathIsNonModuleName(tspath.RemoveFileExtension(tspath.GetRelativePathFromDirectory(fromDirectory, to, compare)))
}
