package flamework

import (
	"path/filepath"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func buildPathGlobIntrinsic(state *TransformState, trace *ast.Node, pathType *checker.Type) (*ast.Node, error) {
	if !pathType.IsStringLiteral() {
		return nil, invalidMacro(trace, "Path is invalid, expected string literal and got: %s", state.checker.TypeToString(pathType))
	}
	glob := stringLiteralValue(pathType)
	absoluteGlob := glob
	if strings.HasPrefix(glob, ".") {
		file := ast.GetSourceFileOfNode(trace)
		absolute := filepath.Join(filepath.Dir(file.FileName()), filepath.FromSlash(glob))
		relative, err := filepath.Rel(state.project.RootDirectory(), absolute)
		if err != nil {
			return nil, invalidMacro(trace, "Could not resolve path glob %q", glob)
		}
		absoluteGlob = filepath.ToSlash(relative)
	}
	fileID, err := projectRelativePath(state.project.RootDirectory(), ast.GetSourceFileOfNode(trace).FileName())
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

func buildPathIntrinsic(state *TransformState, trace *ast.Node, pathType *checker.Type) (*ast.Node, error) {
	if !pathType.IsStringLiteral() {
		return nil, invalidMacro(trace, "Path is invalid, expected string literal and got: %s", state.checker.TypeToString(pathType))
	}
	input := stringLiteralValue(pathType)
	if !filepath.IsAbs(input) {
		input = filepath.Join(state.project.RootDirectory(), filepath.FromSlash(input))
	}
	output := state.project.PathTranslator().GetOutputPath(input)
	rbxPath, ok := state.project.RojoResolver().GetRbxPathFromFilePath(output)
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
