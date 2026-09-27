package flamework

import (
	"path/filepath"
	"strings"

	"rotor/internal/rojo"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func buildPathGlobIntrinsic(state *TransformState, trace *ast.Node, pathType *checker.Type) (*ast.Node, error) {
	if !pathType.IsStringLiteral() {
		return nil, invalidMacro(trace, "Path is invalid, expected string literal and got: %s", state.checker.TypeToString(pathType))
	}
	glob := stringLiteralValue(pathType)
	file := ast.GetSourceFileOfNode(trace)
	absoluteGlob, ok := projectGlob(state, file, glob)
	if !ok {
		return nil, invalidMacro(trace, "Could not resolve path glob %q", glob)
	}
	fileID, err := projectRelativePath(state.project.RootDirectory(), file.FileName())
	if err != nil {
		return nil, err
	}
	state.project.AddGlob(absoluteGlob, fileID)
	value, err := hashText(state, absoluteGlob, "addPaths", false)
	if err != nil {
		return nil, err
	}
	return state.factory.NewStringLiteral(value, ast.TokenFlagsNone), nil
}

// projectGlob makes a file-relative ("."-prefixed) glob project-relative.
func projectGlob(state *TransformState, file *ast.SourceFile, glob string) (string, bool) {
	if !strings.HasPrefix(glob, ".") {
		return glob, true
	}
	absolute := filepath.Join(filepath.Dir(file.FileName()), filepath.FromSlash(glob))
	relative, err := filepath.Rel(state.project.RootDirectory(), absolute)
	if err != nil {
		return "", false
	}
	return filepath.ToSlash(relative), true
}

func buildPathIntrinsic(state *TransformState, trace *ast.Node, pathType *checker.Type) (*ast.Node, error) {
	if !pathType.IsStringLiteral() {
		return nil, invalidMacro(trace, "Path is invalid, expected string literal and got: %s", state.checker.TypeToString(pathType))
	}
	rbxPath, ok := pathIntrinsicRbxPath(state, projectPathInput(state, stringLiteralValue(pathType)))
	if !ok {
		return nil, invalidMacro(trace, "Could not find Rojo data for '%s'", stringLiteralValue(pathType))
	}
	parts := make([]*ast.Node, len(rbxPath))
	for index, part := range rbxPath {
		parts[index] = state.factory.NewStringLiteral(part, ast.TokenFlagsNone)
	}
	pathExpression := state.factory.NewArrayLiteralExpression(state.factory.NewNodeList(parts), true)
	return state.factory.NewArrayLiteralExpression(state.factory.NewNodeList([]*ast.Node{pathExpression}), true), nil
}

// projectPathInput resolves a path intrinsic input against the project root.
func projectPathInput(state *TransformState, input string) string {
	if filepath.IsAbs(input) {
		return input
	}
	return filepath.Join(state.project.RootDirectory(), filepath.FromSlash(input))
}

func pathIntrinsicRbxPath(state *TransformState, input string) (rojo.RbxPath, bool) {
	return state.project.RojoResolver().GetRbxPathFromFilePath(state.project.PathTranslator().GetOutputPath(input))
}
