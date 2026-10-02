package compile

import (
	"fmt"
	"path/filepath"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
)

func (d *solutionBuildDrainer) validateSatisfiedProject(project SolutionProject, cache *solutionCompileCache) *solutionSelectionError {
	if detail := d.restoredMetadataErrs[project.ConfigPath]; detail != "" {
		message := fmt.Sprintf("satisfied project %s has invalid restored metadata: %s", project.ConfigPath, detail)
		return &solutionSelectionError{configPath: project.ConfigPath, code: "SOLUTION_SATISFIED_OUTPUT_INVALID", message: message}
	}
	for _, declarationPath := range d.restoredDeclarations[project.ConfigPath] {
		declarationText, ok := cache.fs.ReadFile(declarationPath)
		if !ok {
			message := fmt.Sprintf("satisfied project %s is missing restored declaration output %s; restore the output or select the project for work", project.ConfigPath, filepath.Clean(filepath.FromSlash(declarationPath)))
			return &solutionSelectionError{configPath: project.ConfigPath, code: "SOLUTION_SATISFIED_OUTPUT_MISSING", message: message}
		}
		parseOptions := declarationSourceFileParseOptions(declarationPath)
		parsed := cache.getSourceFile(parseOptions, func(options ast.SourceFileParseOptions) *ast.SourceFile {
			return parser.ParseSourceFile(options, declarationText, core.ScriptKindTS)
		})
		if parsed == nil || len(parsed.Diagnostics()) > 0 {
			detail := "the declaration could not be parsed"
			if parsed != nil && len(parsed.Diagnostics()) > 0 {
				detail = parsed.Diagnostics()[0].String()
			}
			message := fmt.Sprintf("satisfied project %s has invalid restored declaration output %s: %s; restore the output or select the project for work", project.ConfigPath, filepath.Clean(filepath.FromSlash(declarationPath)), detail)
			return &solutionSelectionError{configPath: project.ConfigPath, code: "SOLUTION_SATISFIED_OUTPUT_INVALID", message: message}
		}
	}
	return nil
}
