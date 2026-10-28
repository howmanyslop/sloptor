package compile

import (
	"errors"
	"maps"
	"path/filepath"
	"reflect"
	"slices"
	"strings"

	"rotor/internal/fsutil"
	"rotor/internal/rojo"
	"rotor/tsgo/outputpaths"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

type solutionWriteMetadata struct {
	writeRoots           map[string][]string
	waitOnlyDependencies map[string][]string
	restoredMetadataErrs map[string]string
	validationErr        error
	referenceOutputs     map[string][]string
}

// projectReferenceInfo is one project's share of the solution metadata. It
// depends only on the parsed config (options and root file names), never on
// file contents, so a text edit leaves it unchanged.
type projectReferenceInfo struct {
	config     *projectConfig
	err        string
	writeRoots []string
	// paths maps source and declaration files to Luau paths for runtime lookup.
	// saltPaths preserves authored spelling without runtime lookup aliases.
	// emits reports whether the project produces outputs.
	paths     map[string]string
	saltPaths map[string]string
	outputs   []string
	emits     bool
}

// crossProjectMetadata caches projectReferenceInfo per project, so a watch
// cycle re-parses only the invalidated projects' configs.
type crossProjectMetadata struct {
	infos map[string]*projectReferenceInfo
	// references caches the inputs of the Rojo mount checks.
	references *referenceRojoCache
	// configParses and refreshes count work for tests: configs parsed and
	// solution metadata derivations.
	configParses int
	refreshes    int
}

func populateCrossProjectMetadata(graph *SolutionGraph) solutionWriteMetadata {
	metadata, _ := newCrossProjectMetadata(graph)
	return metadata
}

func newCrossProjectMetadata(graph *SolutionGraph) (solutionWriteMetadata, *crossProjectMetadata) {
	cache := &crossProjectMetadata{references: newReferenceRojoCache(nil)}
	return cache.rebuild(graph), cache
}

// rebuild drops every cached project info and derives the metadata of graph
// from fresh config parses.
func (m *crossProjectMetadata) rebuild(graph *SolutionGraph) solutionWriteMetadata {
	m.infos = make(map[string]*projectReferenceInfo, len(graph.Projects))
	m.parse(graph.Projects)
	return m.derive(graph, graph.Projects)
}

// parse re-reads the configs of projects and reports whether the metadata of
// any of them changed.
func (m *crossProjectMetadata) parse(projects []SolutionProject) bool {
	changed := false
	for _, project := range projects {
		info := newProjectReferenceInfo(project)
		m.configParses++
		if !info.equal(m.infos[project.ConfigPath]) {
			changed = true
		}
		m.infos[project.ConfigPath] = info
	}
	return changed
}

func newProjectReferenceInfo(project SolutionProject) *projectReferenceInfo {
	info := &projectReferenceInfo{writeRoots: []string{}}
	config, diagnostics, err := parseProjectConfig(project.ConfigPath)
	if err != nil {
		info.err = err.Error()
		if len(diagnostics) > 0 {
			info.err = strings.Join(diagnostics, "; ")
		}
		return info
	}
	info.config = config
	options := config.Options()
	if options.OutDir != "" {
		pathTranslator := createPathTranslator(config, !project.Options.LuaExtension)
		info.writeRoots = appendCanonicalWriteRoot(info.writeRoots, pathTranslator.OutDir)
		if options.GetEmitDeclarations() && options.DeclarationDir != "" {
			info.writeRoots = appendCanonicalWriteRoot(info.writeRoots, filepath.FromSlash(options.DeclarationDir))
		}
	}
	if project.Options.EmitIncludeFiles {
		if includePath, includeErr := resolveIncludePath(filepath.Dir(project.ConfigPath), project.Options.IncludePath); includeErr == nil {
			info.writeRoots = appendCanonicalWriteRoot(info.writeRoots, includePath)
		}
	}
	if len(projectRootDirs(config)) == 0 || options.OutDir == "" {
		return info
	}
	translator := createPathTranslator(config, !project.Options.LuaExtension)
	info.paths = map[string]string{}
	info.saltPaths = map[string]string{}
	info.outputs = []string{}
	info.emits = true
	for _, fileName := range config.FileNames() {
		if tspath.IsDeclarationFileName(fileName) && !isPathDescendantOf(filepath.FromSlash(fileName), translator.RootDir) {
			continue
		}
		importPath := translator.GetImportPath(fileName, false)
		info.outputs = append(info.outputs, importPath)
		canonical := rojo.CanonicalFileName(fileName, osvfs.FS().UseCaseSensitiveFileNames())
		info.paths[canonical] = importPath
		info.saltPaths[normalizeSourceFilePath(fileName)] = importPath
		if !tspath.IsDeclarationFileName(fileName) && !strings.HasSuffix(fileName, ".json") {
			declarationPath := outputpaths.GetOutputDeclarationFileNameWorker(fileName, options, config)
			declarationCanonical := rojo.CanonicalFileName(declarationPath, osvfs.FS().UseCaseSensitiveFileNames())
			info.paths[declarationCanonical] = importPath
			info.saltPaths[normalizeSourceFilePath(declarationPath)] = importPath
		}
	}
	addProjectRealPathAliases(info.paths, filepath.Dir(project.ConfigPath), osvfs.FS().UseCaseSensitiveFileNames())
	return info
}

// equal compares all that the solution metadata reads: the derived paths,
// outputs, and write roots, and the options the validations check.
func (info *projectReferenceInfo) equal(other *projectReferenceInfo) bool {
	if other == nil || info.err != other.err || info.emits != other.emits ||
		!slices.Equal(info.writeRoots, other.writeRoots) || !slices.Equal(info.outputs, other.outputs) ||
		!maps.Equal(info.paths, other.paths) || !maps.Equal(info.saltPaths, other.saltPaths) || (info.config == nil) != (other.config == nil) {
		return false
	}
	return info.config == nil || reflect.DeepEqual(info.config.Options(), other.config.Options())
}

// derive builds the solution metadata from the cached project infos and
// stores each project's reference paths in graph. The Rojo mount check covers
// the direct references of consumers only.
func (m *crossProjectMetadata) derive(graph *SolutionGraph, consumers []SolutionProject) solutionWriteMetadata {
	m.refreshes++
	configs := make(map[string]projectOutputHost, len(graph.Projects))
	metadata := solutionWriteMetadata{
		writeRoots:           make(map[string][]string, len(graph.Projects)),
		waitOnlyDependencies: map[string][]string{},
		restoredMetadataErrs: make(map[string]string),
		referenceOutputs:     make(map[string][]string),
	}
	for _, project := range graph.Projects {
		info := m.infos[project.ConfigPath]
		if info == nil {
			continue
		}
		metadata.writeRoots[project.ConfigPath] = info.writeRoots
		if info.err != "" {
			metadata.restoredMetadataErrs[project.ConfigPath] = info.err
			continue
		}
		configs[project.ConfigPath] = info.config
	}
	projects := make(map[string]SolutionProject, len(graph.Projects))
	for _, project := range graph.Projects {
		projects[project.ConfigPath] = project
	}
	for index := range graph.Projects {
		project := &graph.Projects[index]
		paths := map[string]string{}
		saltPaths := map[string]string{}
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
			if info := m.infos[configPath]; info != nil {
				maps.Copy(paths, info.paths)
				maps.Copy(saltPaths, info.saltPaths)
				outputs = append(outputs, info.outputs...)
			}
		}
		for _, reference := range project.References {
			visit(reference)
		}
		project.Options.projectReferencePaths = paths
		project.Options.projectReferenceSaltPaths = saltPaths
		if own := m.infos[project.ConfigPath]; own != nil && own.emits {
			outputs = append(outputs, own.outputs...)
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
	metadata.validationErr = validateSolutionReferences(graph, configs)
	if metadata.validationErr == nil {
		metadata.validationErr = validateProjectsReferenceRojoPaths(graph, consumers, metadata.referenceOutputs, m.references)
	}
	if metadata.validationErr == nil {
		metadata.validationErr = validateProjectOutputs(graph, configs)
	}
	return metadata
}

// projectRestoredDeclarations lists the declaration outputs a satisfied
// project must have restored. It builds the program, because the emitted set
// includes sources that only imports reach.
func projectRestoredDeclarations(project SolutionProject) ([]string, error) {
	_, program, diagnostics, err := newProjectProgram(filepath.Dir(project.ConfigPath), project.ConfigPath)
	if err != nil {
		if len(diagnostics) > 0 {
			return nil, errors.New(strings.Join(diagnostics, "; "))
		}
		return nil, err
	}
	options := program.Options()
	if options.OutDir == "" || !options.GetEmitDeclarations() {
		return nil, nil
	}
	declarations := []string{}
	for _, sourceFile := range projectSourceFiles(program) {
		declarations = append(declarations, filepath.FromSlash(outputpaths.GetOutputDeclarationFileNameWorker(sourceFile.FileName(), options, program)))
	}
	return declarations, nil
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

// addProjectRealPathAliases also keys paths under dir by dir's real path, since
// import lookups may see a file through its resolved path (for example an
// 8.3 short temp path or a junction). One Realpath per project, not per file.
func addProjectRealPathAliases(paths map[string]string, dir string, useCaseSensitiveFileNames bool) {
	spelled := rojo.CanonicalFileName(dir, useCaseSensitiveFileNames)
	real := rojo.CanonicalFileName(osvfs.FS().Realpath(filepath.ToSlash(dir)), useCaseSensitiveFileNames)
	if real == spelled {
		return
	}
	prefix := spelled + string(filepath.Separator)
	aliases := map[string]string{}
	for key, value := range paths {
		if rest, ok := strings.CutPrefix(key, prefix); ok {
			aliases[filepath.Join(real, rest)] = value
		}
	}
	maps.Copy(paths, aliases)
}

func canonicalWriteRoot(path string) string {
	path = filepath.Clean(filepath.FromSlash(path))
	if absolute, err := filepath.Abs(path); err == nil {
		path = filepath.Clean(absolute)
	}
	missing := []string{}
	for {
		if resolved, err := fsutil.EvalSymlinks(path); err == nil {
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

func projectRootDirs(program projectOutputHost) []string {
	options := program.Options()
	if options.RootDir != "" {
		return []string{options.RootDir}
	}
	return options.RootDirs
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
