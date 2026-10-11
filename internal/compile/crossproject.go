package compile

import (
	"path/filepath"
	"strings"

	"rotor/internal/rojo"
	"rotor/tsgo/compiler"
	"rotor/tsgo/outputpaths"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

type solutionWriteMetadata struct {
	writeRoots           map[string][]string
	waitOnlyDependencies map[string][]string
	restoredDeclarations map[string][]string
	restoredMetadataErrs map[string]string
	validationErr        error
	referenceOutputs     map[string][]string
}

func populateCrossProjectMetadata(graph *SolutionGraph) solutionWriteMetadata {
	pathsByProject := make(map[string]map[string]string, len(graph.Projects))
	outputsByProject := make(map[string][]string, len(graph.Projects))
	programs := make(map[string]*compiler.Program, len(graph.Projects))
	metadata := solutionWriteMetadata{
		writeRoots:           make(map[string][]string, len(graph.Projects)),
		waitOnlyDependencies: map[string][]string{},
		restoredDeclarations: make(map[string][]string, len(graph.Projects)),
		restoredMetadataErrs: make(map[string]string),
		referenceOutputs:     make(map[string][]string),
	}
	for _, project := range graph.Projects {
		metadata.writeRoots[project.ConfigPath] = []string{}
		_, program, diagnostics, err := newProjectProgram(filepath.Dir(project.ConfigPath), project.ConfigPath)
		if err != nil {
			metadata.restoredMetadataErrs[project.ConfigPath] = err.Error()
			if len(diagnostics) > 0 {
				metadata.restoredMetadataErrs[project.ConfigPath] = strings.Join(diagnostics, "; ")
			}
			continue
		}
		rootDirs := projectRootDirs(program)
		programs[project.ConfigPath] = program
		if program.Options().OutDir != "" {
			pathTranslator := createPathTranslator(program, !project.Options.LuaExtension)
			metadata.writeRoots[project.ConfigPath] = appendCanonicalWriteRoot(
				metadata.writeRoots[project.ConfigPath],
				pathTranslator.OutDir,
			)
			if program.Options().GetEmitDeclarations() {
				if declarationDir := program.Options().DeclarationDir; declarationDir != "" {
					metadata.writeRoots[project.ConfigPath] = appendCanonicalWriteRoot(metadata.writeRoots[project.ConfigPath], filepath.FromSlash(declarationDir))
				}
				for _, sourceFile := range projectSourceFiles(program) {
					metadata.restoredDeclarations[project.ConfigPath] = append(
						metadata.restoredDeclarations[project.ConfigPath],
						filepath.FromSlash(outputpaths.GetOutputDeclarationFileNameWorker(sourceFile.FileName(), program.Options(), program)),
					)
				}
			}
		}
		if project.Options.EmitIncludeFiles {
			if includePath, includeErr := resolveIncludePath(filepath.Dir(project.ConfigPath), project.Options.IncludePath); includeErr == nil {
				metadata.writeRoots[project.ConfigPath] = appendCanonicalWriteRoot(
					metadata.writeRoots[project.ConfigPath],
					includePath,
				)
			}
		}
		if len(rootDirs) == 0 || program.Options().OutDir == "" {
			continue
		}
		translator := createPathTranslator(program, !project.Options.LuaExtension)
		paths := map[string]string{}
		pathsByProject[project.ConfigPath] = paths
		outputsByProject[project.ConfigPath] = []string{}
		for _, fileName := range program.CommandLine().FileNames() {
			if tspath.IsDeclarationFileName(fileName) && !isPathDescendantOf(filepath.FromSlash(fileName), translator.RootDir) {
				continue
			}
			importPath := translator.GetImportPath(fileName, false)
			outputsByProject[project.ConfigPath] = append(outputsByProject[project.ConfigPath], importPath)
			canonical := rojo.CanonicalFileName(fileName, osvfs.FS().UseCaseSensitiveFileNames())
			paths[canonical] = importPath
			if !tspath.IsDeclarationFileName(fileName) && !strings.HasSuffix(fileName, ".json") {
				declarationPath := outputpaths.GetOutputDeclarationFileNameWorker(fileName, program.Options(), program)
				declarationCanonical := rojo.CanonicalFileName(declarationPath, osvfs.FS().UseCaseSensitiveFileNames())
				paths[declarationCanonical] = importPath
			}
		}
	}
	projects := make(map[string]SolutionProject, len(graph.Projects))
	for _, project := range graph.Projects {
		projects[project.ConfigPath] = project
	}
	for index := range graph.Projects {
		project := &graph.Projects[index]
		paths := map[string]string{}
		outputs := []string{}
		visited := map[string]bool{}
		var visit func(string)
		visit = func(configPath string) {
			if visited[configPath] {
				return
			}
			visited[configPath] = true
			for _, reference := range projects[configPath].References {
				visit(reference)
			}
			for key, output := range pathsByProject[configPath] {
				paths[key] = output
			}
			outputs = append(outputs, outputsByProject[configPath]...)
		}
		for _, reference := range project.References {
			visit(reference)
		}
		project.Options.projectReferencePaths = paths
		if ownOutputs, emits := outputsByProject[project.ConfigPath]; emits {
			outputs = append(outputs, ownOutputs...)
			includePath, err := resolveIncludePath(filepath.Dir(project.ConfigPath), project.Options.IncludePath)
			if err != nil {
				metadata.validationErr = err
				return metadata
			}
			outputs = append(outputs, filepath.Join(includePath, "RuntimeLib.lua"))
			metadata.referenceOutputs[project.ConfigPath] = outputs
		}
	}
	metadata.waitOnlyDependencies = deriveWaitOnlyDependencies(graph.Projects, metadata.writeRoots)
	metadata.validationErr = validateSolutionReferences(graph, programs)
	if metadata.validationErr == nil {
		metadata.validationErr = validateReferenceRojoPaths(graph, metadata.referenceOutputs)
	}
	if metadata.validationErr == nil {
		metadata.validationErr = validateProjectOutputs(graph, programs)
	}
	return metadata
}

func appendCanonicalWriteRoot(roots []string, path string) []string {
	canonical := canonicalWriteRoot(path)
	for _, root := range roots {
		if root == canonical {
			return roots
		}
	}
	return append(roots, canonical)
}

func canonicalWriteRoot(path string) string {
	path = filepath.Clean(filepath.FromSlash(path))
	if absolute, err := filepath.Abs(path); err == nil {
		path = filepath.Clean(absolute)
	}
	missing := []string{}
	for {
		if resolved, err := filepath.EvalSymlinks(path); err == nil {
			for index := len(missing) - 1; index >= 0; index-- {
				resolved = filepath.Join(resolved, missing[index])
			}
			return filepath.Clean(resolved)
		}
		parent := filepath.Dir(path)
		if parent == path {
			return path
		}
		missing = append(missing, filepath.Base(path))
		path = parent
	}
}

func deriveWaitOnlyDependencies(projects []SolutionProject, writeRoots map[string][]string) map[string][]string {
	dependencies := map[string][]string{}
	for laterIndex := 1; laterIndex < len(projects); laterIndex++ {
		later := projects[laterIndex]
		for earlierIndex := 0; earlierIndex < laterIndex; earlierIndex++ {
			earlier := projects[earlierIndex]
			if writeRootsOverlap(writeRoots[earlier.ConfigPath], writeRoots[later.ConfigPath]) {
				dependencies[later.ConfigPath] = append(dependencies[later.ConfigPath], earlier.ConfigPath)
			}
		}
	}
	return dependencies
}

func writeRootsOverlap(left, right []string) bool {
	for _, leftRoot := range left {
		for _, rightRoot := range right {
			if writeRootContains(leftRoot, rightRoot) || writeRootContains(rightRoot, leftRoot) {
				return true
			}
		}
	}
	return false
}

func writeRootContains(root, path string) bool {
	if !osvfs.FS().UseCaseSensitiveFileNames() {
		root = strings.ToLower(root)
		path = strings.ToLower(path)
	}
	relative, err := filepath.Rel(root, path)
	return err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator))
}

func projectRootDirs(program *compiler.Program) []string {
	options := program.Options()
	if options.RootDir != "" {
		return []string{options.RootDir}
	}
	return options.RootDirs
}

func isProjectTypeScriptFile(fileName string) bool {
	return strings.HasSuffix(fileName, ".ts") || strings.HasSuffix(fileName, ".tsx")
}

func pathWithinAnyRoot(fileName string, rootDirs []string) bool {
	for _, rootDir := range rootDirs {
		rel, err := filepath.Rel(filepath.FromSlash(rootDir), filepath.FromSlash(fileName))
		if err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			return true
		}
	}
	return false
}
