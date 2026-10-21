package compile

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	stdjson "encoding/json"
	"maps"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"sort"

	"rotor/internal/fsutil"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tspath"
)

// incrementalManifest is what one build leaves behind for the next one to
// select against.
//
// Files carries each source file's content hash and the project files it
// resolved a reference to, which is the import graph selection walks.
// Keys are absolute compiler filenames in memory and project-relative URIs
// on disk, independent of the manifest's folder or volume.
//
// Outputs carries the content hash of every file this build wrote, keyed by
// project-relative output path. It is what lets the writer skip a byte-identical
// rewrite, and it doubles as the declaration SIGNATURE store: an incremental
// build re-emits the `.d.ts` of a file it is considering, compares it against
// the hash recorded here, and stops propagating to that file's importers when
// the two agree (selectByDeclarationSignature).
//
// That is deliberately TypeScript's rule and not rbxtsc's: `tsc --incremental`
// and the BuilderProgram behind it key a file's downstream invalidation on its
// emitted declaration ("shape signature"), while rbxtsc always rebuilds the
// whole importer closure. Rotor therefore selects strictly fewer files than
// rbxtsc for the same edit and writes the same bytes.
//
// Salt invalidates the whole manifest at once. It covers the compiler options,
// the Rojo/include topology, the native Flamework inputs, and each effective
// transformer plugin's identity (incremental_salt.go), because none of those
// show up as a source-file change.
type incrementalManifest struct {
	Version int                             `json:"version"`
	Salt    string                          `json:"salt"`
	Files   map[string]incrementalFileState `json:"files"`
	Outputs map[string]string               `json:"outputs"`
}

type incrementalFileState struct {
	Hash string   `json:"hash"`
	Refs []string `json:"refs,omitempty"`
}

func readIncrementalManifest(path string, program *compiler.Program) (*incrementalManifest, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var manifest incrementalManifest
	if err := stdjson.Unmarshal(data, &manifest); err != nil {
		return nil, nil
	}
	if manifest.Version != 3 {
		return nil, nil
	}
	if manifest.Files == nil {
		manifest.Files = map[string]incrementalFileState{}
	}
	if manifest.Outputs == nil {
		manifest.Outputs = map[string]string{}
	}
	// Inspection callers can read the persisted graph without a compiler.
	if program == nil {
		return &manifest, nil
	}
	// Reuse the compiler's filenames, including its preserveSymlinks choice,
	// without realpath/stat calls for every file on a warm build.
	sourcePaths := map[string]string{}
	projectDir := program.GetCurrentDirectory()
	encodePath := incrementalSourcePathEncoder(program)
	for _, file := range program.SourceFiles() {
		stored := resolveIncrementalSourcePath(projectDir, encodePath(file.FileName()))
		sourcePaths[normalizeSourceFilePath(stored)] = normalizeSourceFilePath(file.FileName())
	}
	manifest.Files = mapIncrementalFiles(manifest.Files, func(name string) string {
		absolute := resolveIncrementalSourcePath(projectDir, name)
		if source, ok := sourcePaths[normalizeSourceFilePath(absolute)]; ok {
			return source
		}
		if !program.Options().PreserveSymlinks.IsTrue() {
			if real, err := fsutil.EvalSymlinks(absolute); err == nil {
				absolute = real
			}
		}
		return normalizeSourceFilePath(absolute)
	})
	return &manifest, nil
}

func writeIncrementalManifest(path string, manifest *incrementalManifest, program *compiler.Program) error {
	stored := *manifest
	stored.Files = mapIncrementalFiles(manifest.Files, incrementalSourcePathEncoder(program))
	data, err := stdjson.MarshalIndent(&stored, "", "\t")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	temp, err := os.CreateTemp(filepath.Dir(path), ".tsbuildinfo-*.tmp")
	if err != nil {
		return err
	}
	tempPath := temp.Name()
	defer func() { _ = os.Remove(tempPath) }()
	if _, err := temp.Write(data); err != nil {
		_ = temp.Close()
		return err
	}
	if err := temp.Chmod(0o644); err != nil {
		_ = temp.Close()
		return err
	}
	if err := temp.Close(); err != nil {
		return err
	}
	return os.Rename(tempPath, path)
}

// Only the persisted graph uses relative paths; selection and declaration
// signatures continue to use the compiler's absolute source filenames.
func mapIncrementalFiles(files map[string]incrementalFileState, convert func(string) string) map[string]incrementalFileState {
	result := make(map[string]incrementalFileState, len(files))
	for name, state := range files {
		state.Refs = slices.Clone(state.Refs)
		for i, ref := range state.Refs {
			state.Refs[i] = convert(ref)
		}
		sort.Strings(state.Refs)
		result[convert(name)] = state
	}
	return result
}

func sameIncrementalManifest(a, b *incrementalManifest) bool {
	return reflect.DeepEqual(a, b)
}

// Declaration and JSON inputs can change an importer's output without being
// compiled themselves. Declarations include external and referenced projects.
func resolvedNonCompiledInputs(program *compiler.Program) []*ast.SourceFile {
	var files []*ast.SourceFile
	for _, sourceFile := range program.SourceFiles() {
		if sourceFile.IsDeclarationFile ||
			(tspath.HasJSONFileExtension(sourceFile.FileName()) && !program.IsSourceFromProjectReference(sourceFile.Path())) {
			files = append(files, sourceFile)
		}
	}
	return files
}

func buildIncrementalManifest(program *compiler.Program, sourceFiles []*ast.SourceFile, salt string, previous *incrementalManifest) (*incrementalManifest, error) {
	inputFiles := slices.Concat(sourceFiles, resolvedNonCompiledInputs(program))
	manifest := &incrementalManifest{
		Version: 3,
		Salt:    salt,
		Files:   make(map[string]incrementalFileState, len(inputFiles)),
		Outputs: map[string]string{},
	}
	currentHashes := make(map[string]string, len(inputFiles))
	for _, sourceFile := range inputFiles {
		path := normalizeSourceFilePath(sourceFile.FileName())
		sum := sha256.Sum256([]byte(sourceFile.Text()))
		currentHashes[path] = hex.EncodeToString(sum[:])
	}
	if previous != nil && previous.Salt == salt && len(previous.Files) == len(currentHashes) {
		unchanged := true
		for path, hash := range currentHashes {
			if prev, ok := previous.Files[path]; !ok || prev.Hash != hash {
				unchanged = false
				break
			}
		}
		if unchanged {
			// No source changed: reuse the previous reference graph verbatim,
			// so the manifest stays byte-identical and no checker work runs.
			manifest.Files = maps.Clone(previous.Files)
			return manifest, nil
		}
	}
	sourceSet := make(map[string]struct{}, len(currentHashes))
	for path := range currentHashes {
		sourceSet[path] = struct{}{}
	}
	globalDeclarations := globalDeclarationPaths(inputFiles)
	for _, sourceFile := range inputFiles {
		path := normalizeSourceFilePath(sourceFile.FileName())
		var refs []string
		if !tspath.HasJSONFileExtension(sourceFile.FileName()) {
			refs = referencedProjectFiles(program, sourceFile, sourceSet)
			if !sourceFile.IsDeclarationFile {
				refs = append(refs, globalDeclarations...)
				sort.Strings(refs)
				refs = slices.Compact(refs)
			}
		}
		manifest.Files[path] = incrementalFileState{Hash: currentHashes[path], Refs: refs}
	}
	return manifest, nil
}

func globalDeclarationPaths(files []*ast.SourceFile) []string {
	var paths []string
	for _, file := range files {
		if !file.IsDeclarationFile {
			continue
		}
		global := !ast.IsExternalModule(file)
		for _, augmentation := range file.ModuleAugmentations {
			if augmentation != nil && !ast.IsStringLiteral(augmentation) {
				global = true
				break
			}
		}
		if global {
			paths = append(paths, normalizeSourceFilePath(file.FileName()))
		}
	}
	return paths
}

func pruneMissingOutputs(index *outputPresenceIndex, outputs map[string]string) {
	for path := range outputs {
		if !index.hasRegular(path) {
			delete(outputs, path)
		}
	}
}

func selectIncrementalSourceFiles(sourceFiles []*ast.SourceFile, current, previous *incrementalManifest) []*ast.SourceFile {
	if previous == nil || previous.Salt != current.Salt {
		return sourceFiles
	}

	changed := make(map[string]struct{})
	for path, state := range current.Files {
		prev, ok := previous.Files[path]
		if !ok || prev.Hash != state.Hash {
			changed[path] = struct{}{}
		}
	}
	for path := range previous.Files {
		if _, ok := current.Files[path]; !ok {
			changed[path] = struct{}{}
		}
	}
	if len(changed) == 0 {
		return nil
	}

	reverse := make(map[string][]string)
	accumulateReverseDeps(reverse, current)
	accumulateReverseDeps(reverse, previous)

	selected := make(map[string]struct{})
	queue := make([]string, 0, len(changed))
	for path := range changed {
		queue = append(queue, path)
		if _, ok := current.Files[path]; ok {
			selected[path] = struct{}{}
		}
	}
	for len(queue) > 0 {
		path := queue[0]
		queue = queue[1:]
		for _, importer := range reverse[path] {
			if _, seen := selected[importer]; seen {
				continue
			}
			selected[importer] = struct{}{}
			queue = append(queue, importer)
		}
	}

	result := make([]*ast.SourceFile, 0, len(selected))
	for _, sourceFile := range sourceFiles {
		if _, ok := selected[normalizeSourceFilePath(sourceFile.FileName())]; ok {
			result = append(result, sourceFile)
		}
	}
	return result
}

func accumulateReverseDeps(reverse map[string][]string, manifest *incrementalManifest) {
	if manifest == nil {
		return
	}
	for importer, state := range manifest.Files {
		for _, dep := range state.Refs {
			reverse[dep] = append(reverse[dep], importer)
		}
	}
}

func referencedProjectFiles(program *compiler.Program, file *ast.SourceFile, sourceSet map[string]struct{}) []string {
	referenced := make(map[string]struct{})
	add := func(path string) {
		path = normalizeSourceFilePath(path)
		if path == normalizeSourceFilePath(file.FileName()) {
			return
		}
		if _, ok := sourceSet[path]; ok {
			referenced[path] = struct{}{}
		}
	}

	checker, done := program.GetTypeCheckerForFileExclusive(context.Background(), file)
	defer done()

	addSymbolDecls := func(symbol *ast.Symbol) {
		if symbol == nil {
			return
		}
		for _, declaration := range symbol.Declarations {
			if sourceFile := ast.GetSourceFileOfNode(declaration); sourceFile != nil {
				add(sourceFile.FileName())
			}
		}
	}

	for _, importName := range file.Imports() {
		addSymbolDecls(checker.GetSymbolAtLocation(importName))
	}

	sourceFileDirectory := filepath.Dir(filepath.FromSlash(file.FileName()))
	for _, referencedFile := range file.ReferencedFiles {
		add(resolveReferencedFile(program, referencedFile.FileName, sourceFileDirectory))
	}

	if typeRefsInFile, ok := program.GetResolvedTypeReferenceDirectives()[file.Path()]; ok {
		for _, typeRef := range typeRefsInFile {
			if typeRef.ResolvedFileName != "" {
				add(resolveReferencedFile(program, typeRef.ResolvedFileName, sourceFileDirectory))
			}
		}
	}

	for _, moduleName := range file.ModuleAugmentations {
		if ast.IsStringLiteral(moduleName) {
			addSymbolDecls(checker.GetSymbolAtLocation(moduleName))
		}
	}

	for _, ambientModule := range checker.GetAmbientModules() {
		addSymbolDecls(ambientModule)
	}

	refs := make([]string, 0, len(referenced))
	for path := range referenced {
		refs = append(refs, path)
	}
	sort.Strings(refs)
	return refs
}

func resolveReferencedFile(program *compiler.Program, fileName, sourceFileDirectory string) string {
	if redirect := program.GetParseFileRedirect(fileName); redirect != "" {
		return redirect
	}
	if filepath.IsAbs(filepath.FromSlash(fileName)) {
		return filepath.FromSlash(fileName)
	}
	return filepath.Join(sourceFileDirectory, filepath.FromSlash(fileName))
}

func normalizeSourceFilePath(path string) string {
	return filepath.Clean(filepath.FromSlash(path))
}
