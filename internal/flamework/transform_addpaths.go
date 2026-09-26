package flamework

import (
	"errors"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"rotor/internal/rojo"
	"rotor/tsgo/ast"
	"rotor/tsgo/tspath"
)

// expandTopLevelAddPaths replaces each top-level Flamework.addPaths or
// addPathsGlob statement with one side-effect import per matching module, in
// path order. Bundlers cannot follow Flamework's runtime requires of
// Instances, but they can follow static imports. Calls it cannot expand keep
// the runtime rewrite and its diagnostics.
func expandTopLevelAddPaths(state *TransformState, sourceFile *ast.SourceFile) (*ast.SourceFile, error) {
	if state.project.RojoResolver() == nil {
		return sourceFile, nil
	}
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
// Lua module or a tsconfig-excluded file. A static import cannot load it, so
// the call keeps the runtime rewrite. Only the pattern's literal directory
// prefix is walked.
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

var unimportableModuleExtensions = map[string]bool{".lua": true, ".luau": true, ".ts": true, ".tsx": true}

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
