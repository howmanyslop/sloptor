package flamework

import (
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"rotor/internal/rojo"
)

var ErrPathEscape = errors.New("flamework: path escapes project root")

// TranslateOutputPath applies the project's concrete roblox-ts path translator
// and returns a slash-separated path relative to root.
func TranslateOutputPath(root string, translator *rojo.PathTranslator, inputPath string) (string, error) {
	input, err := projectPath(root, inputPath)
	if err != nil {
		return "", err
	}

	output := translator.GetOutputPath(input)
	return projectRelativePath(root, output)
}

func projectPath(root, filePath string) (string, error) {
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return "", fmt.Errorf("flamework: resolve project root: %w", err)
	}
	absPath := filePath
	if !filepath.IsAbs(absPath) {
		absPath = filepath.Join(absRoot, filepath.FromSlash(absPath))
	}
	absPath, err = filepath.Abs(absPath)
	if err != nil {
		return "", fmt.Errorf("flamework: resolve path %q: %w", filePath, err)
	}
	if _, err := projectRelativePath(absRoot, absPath); err != nil {
		return "", err
	}
	return absPath, nil
}

func projectRelativePath(root, filePath string) (string, error) {
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return "", fmt.Errorf("flamework: resolve project root: %w", err)
	}
	absPath, err := filepath.Abs(filePath)
	if err != nil {
		return "", fmt.Errorf("flamework: resolve path %q: %w", filePath, err)
	}
	rel, err := filepath.Rel(absRoot, absPath)
	if err != nil {
		return "", fmt.Errorf("flamework: make %q project-relative: %w", filePath, err)
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) || filepath.IsAbs(rel) {
		return "", fmt.Errorf("%w: %q", ErrPathEscape, filePath)
	}
	return filepath.ToSlash(rel), nil
}
