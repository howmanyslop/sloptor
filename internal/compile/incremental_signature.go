package compile

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"path/filepath"
	"strings"

	"rotor/internal/rojo"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
)

// declarationSignatureSelection is the state one selection pass needs.
type declarationSignatureSelection struct {
	// projectDir is what output paths are keyed relative to, matching
	// outputWriter.outputKey so previousOutputs can be looked up directly.
	projectDir string
	// declarationPath maps a source file to the absolute path its declaration
	// output is written to.
	declarationPath func(string) string
	// previousOutputs is the last build's output-path -> content hash map,
	// already validated against what is on disk.
	previousOutputs map[string]string
	// emit yields the declaration text a set of source files produces, as
	// output-path/text pairs. It must be the same text the build will write:
	// the comparison below is against the hash the previous build recorded for
	// the file it wrote.
	emit func(files []*ast.SourceFile) ([]sidecarOutputFile, error)
}

// selectByDeclarationSignature picks the files an incremental build has to
// recompile, stopping propagation at a dependency whose declaration output did
// not move.
//
// Today's rule (selectIncrementalSourceFiles) selects every transitive importer
// of every changed file, so a comment edit in a widely imported module
// recompiles the module's whole downstream cone. Nothing an importer can
// observe changes unless the dependency's declaration output changes, so
// importers are only pulled in when the freshly emitted `.d.ts` differs from
// the one the previous build recorded.
//
// This is TypeScript's own rule, not rbxtsc's: `tsc --incremental` and the
// BuilderProgram behind it key a file's downstream invalidation on its emitted
// declaration ("shape signature"), which is what this reproduces. rbxtsc has no
// equivalent and always rebuilds the whole importer closure.
//
// The seed set is the caller's: directly changed files, plus whatever else the
// caller already knows it must rebuild (missing outputs, importers of deleted
// files). Files whose declaration text cannot be produced or cannot be compared
// against a recorded hash propagate unconditionally, so a missing or unreadable
// previous output degrades to today's rule rather than to a stale build.
//
// Returns the selected files in sourceFiles order, together with every
// declaration emitted along the way so the caller does not have to emit them a
// second time.
func selectByDeclarationSignature(
	sourceFiles []*ast.SourceFile,
	seeds map[string]struct{},
	current, previous *incrementalManifest,
	selection declarationSignatureSelection,
) ([]*ast.SourceFile, []sidecarOutputFile, error) {
	if selection.emit == nil || selection.declarationPath == nil {
		return nil, nil, errors.New("compile: declaration signature selection needs an emitter and a path mapping")
	}

	byPath := make(map[string]*ast.SourceFile, len(sourceFiles))
	for _, sourceFile := range sourceFiles {
		byPath[normalizeSourceFilePath(sourceFile.FileName())] = sourceFile
	}
	reverse := make(map[string][]string)
	accumulateReverseDeps(reverse, current)
	accumulateReverseDeps(reverse, previous)

	selected := make(map[string]struct{}, len(seeds))
	pending := make([]string, 0, len(seeds))
	for path := range seeds {
		if _, ok := byPath[path]; !ok {
			// A deleted file emits nothing to compare, so its importers are
			// the seeds instead.
			for _, importer := range reverse[path] {
				if _, ok := byPath[importer]; ok {
					pending = append(pending, importer)
				}
			}
			continue
		}
		pending = append(pending, path)
	}

	var declarations []sidecarOutputFile
	for len(pending) > 0 {
		wave := make([]*ast.SourceFile, 0, len(pending))
		for _, path := range pending {
			if _, ok := selected[path]; ok {
				continue
			}
			sourceFile, ok := byPath[path]
			if !ok {
				continue
			}
			selected[path] = struct{}{}
			wave = append(wave, sourceFile)
		}
		pending = pending[:0]
		if len(wave) == 0 {
			break
		}

		emitted, err := selection.emit(wave)
		if err != nil {
			return nil, nil, err
		}
		declarations = append(declarations, emitted...)
		texts := make(map[string]string, len(emitted))
		for _, file := range emitted {
			texts[selection.outputKey(file.FileName)] = file.Text
		}

		for _, sourceFile := range wave {
			if selection.signatureHeld(sourceFile, texts) {
				continue
			}
			pending = append(pending, reverse[normalizeSourceFilePath(sourceFile.FileName())]...)
		}
	}

	result := make([]*ast.SourceFile, 0, len(selected))
	for _, sourceFile := range sourceFiles {
		if _, ok := selected[normalizeSourceFilePath(sourceFile.FileName())]; ok {
			result = append(result, sourceFile)
		}
	}
	return result, declarations, nil
}

// signatureHeld reports whether this file's declaration output is byte-for-byte
// what the previous build wrote. Anything it cannot establish — no declaration
// emitted, no recorded hash — is reported as a change, because the alternative
// is leaving an importer stale.
func (selection declarationSignatureSelection) signatureHeld(sourceFile *ast.SourceFile, texts map[string]string) bool {
	key := selection.outputKey(selection.declarationPath(sourceFile.FileName()))
	if key == "" {
		return false
	}
	previous, recorded := selection.previousOutputs[key]
	if !recorded {
		return false
	}
	text, emitted := texts[key]
	if !emitted {
		return false
	}
	sum := sha256.Sum256([]byte(text))
	return hex.EncodeToString(sum[:]) == previous
}

// outputKey mirrors outputWriter.outputKey so a declaration path lines up with
// the manifest entry the previous build recorded for it.
func (selection declarationSignatureSelection) outputKey(path string) string {
	if selection.projectDir == "" || path == "" {
		return ""
	}
	relative, err := filepath.Rel(selection.projectDir, filepath.Clean(filepath.FromSlash(path)))
	if err != nil {
		return filepath.ToSlash(filepath.Clean(filepath.FromSlash(path)))
	}
	return filepath.ToSlash(relative)
}

// changedSourcePaths is the set of files whose own text moved since the
// previous build, including the ones that went away.
func changedSourcePaths(current, previous *incrementalManifest) map[string]struct{} {
	changed := make(map[string]struct{})
	if current == nil || previous == nil {
		return changed
	}
	for path, state := range current.Files {
		if prev, ok := previous.Files[path]; !ok || prev.Hash != state.Hash {
			changed[path] = struct{}{}
		}
	}
	for path := range previous.Files {
		if _, ok := current.Files[path]; !ok {
			changed[path] = struct{}{}
		}
	}
	return changed
}

// narrowSelectionByDeclarationSignature applies the declaration-signature rule
// when the build is in a position to trust it, and reports false when the
// caller should keep today's reverse-closure selection.
//
// It needs a previous build to compare against (same salt, recorded output
// hashes) and a project whose declaration text this pass can reproduce exactly
// (selectionDeclarationsMatchTheWritePath).
func narrowSelectionByDeclarationSignature(
	dir string,
	program *compiler.Program,
	pathTranslator *rojo.PathTranslator,
	sourceFiles []*ast.SourceFile,
	extraSeeds map[string]struct{},
	current, previous *incrementalManifest,
	previousOutputs map[string]string,
	pipeline *flameworkPipeline,
	timings *BuildTimings,
) ([]*ast.SourceFile, []sidecarOutputFile, bool, error) {
	if previous == nil || current == nil || previous.Salt != current.Salt {
		return nil, nil, false, nil
	}
	if len(previousOutputs) == 0 || !selectionDeclarationsMatchTheWritePath(program, pipeline) {
		return nil, nil, false, nil
	}

	seeds := changedSourcePaths(current, previous)
	for path := range extraSeeds {
		seeds[path] = struct{}{}
	}
	if len(seeds) == 0 {
		return nil, nil, true, nil
	}

	selected, declarations, err := selectByDeclarationSignature(sourceFiles, seeds, current, previous, declarationSignatureSelection{
		projectDir:      filepath.Clean(filepath.FromSlash(dir)),
		declarationPath: pathTranslator.GetOutputDeclarationPath,
		previousOutputs: previousOutputs,
		emit: func(files []*ast.SourceFile) ([]sidecarOutputFile, error) {
			// This is a real declaration emit, not the bookkeeping the rest of
			// incremental selection is, so it reports on its own stage rather
			// than inflating that one.
			stop := logStage(program.Options().ConfigFilePath, declarationSignatureEmitStage)
			declarations, err := declarationTextsForSelection(program, files)
			timings.moveStageDuration(incrementalSelectionStage, declarationSignatureEmitStage, stop())
			return declarations, err
		},
	})
	if err != nil {
		return nil, nil, false, err
	}
	return selected, declarations, true, nil
}

// selectionDeclarationsMatchTheWritePath reports whether the text
// declarationTextsForSelection produces is byte-for-byte the declaration text
// this build will write. The rule compares against the hash the previous build
// recorded for the file it wrote, and the caller hands the emitted text
// straight to the write path, so anything less is either a rule that never
// fires or a build that writes the wrong declarations.
//
// TEMPORARY. Three routes still produce declaration text that this pass cannot
// reproduce, and all three are being removed with the sidecar declaration
// stage:
//
//   - a project with external transformer plugins, whose declarations go
//     through the worker so an `afterDeclarations` transformer can see them;
//   - a project with `paths`/`baseUrl`, whose declarations go through the
//     worker for the module-specifier rewrite;
//   - a native Flamework project, whose declarations are emitted from the
//     overlaid program the transform produced rather than from this one.
//
// Once declaration emit is native (the paths rewrite included) and runs off
// the same program for every project, this collapses to
// `program.Options().GetEmitDeclarations()`.
func selectionDeclarationsMatchTheWritePath(program *compiler.Program, pipeline *flameworkPipeline) bool {
	if !program.Options().GetEmitDeclarations() {
		return false
	}
	if pipeline != nil {
		return false
	}
	return !projectUsesTransformerPlugins(program.CommandLine()) && !declarationUsesPathAliases(program)
}

// declarationTextsForSelection emits declaration output into memory with tsgo.
//
// SEAM. This is the one place the selection pass produces declaration text, and
// it deliberately mirrors emitDeclarations' native branch step for step so the
// two agree byte-for-byte. When the native declaration route lands (its
// `paths` rewrite included), this is where its emitter plugs in, and
// selectionDeclarationsMatchTheWritePath widens to every declaration-emitting
// project.
func declarationTextsForSelection(program *compiler.Program, files []*ast.SourceFile) ([]sidecarOutputFile, error) {
	if !program.Options().GetEmitDeclarations() {
		return nil, nil
	}
	ctx := context.Background()
	emitted := make([][]sidecarOutputFile, len(files))
	jobs := make([]func() error, 0, len(files))
	for index, sourceFile := range files {
		if sourceFile.IsDeclarationFile || !isCompilableFile(sourceFile.FileName()) {
			continue
		}
		jobs = append(jobs, func() error {
			var produced []sidecarOutputFile
			result := program.Emit(ctx, compiler.EmitOptions{
				TargetSourceFile: sourceFile,
				EmitOnly:         compiler.EmitOnlyDts,
				WriteFile: func(fileName string, text string, _ *compiler.WriteFileData) error {
					produced = append(produced, sidecarOutputFile{
						FileName: filepath.FromSlash(fileName),
						Text:     rewriteDeclarationTypeReferences(text),
					})
					return nil
				},
			})
			emitted[index] = produced
			if result != nil && len(result.Diagnostics) > 0 {
				return errors.New("compile: declaration emit diagnostics")
			}
			return nil
		})
	}
	if err := parallelize(writeWorkers(), jobs); err != nil {
		return nil, err
	}
	var all []sidecarOutputFile
	for _, produced := range emitted {
		all = append(all, produced...)
	}
	return all, nil
}

// recoveredInputPaths names the source files whose recorded outputs are no
// longer on disk, so a build that lost an output rebuilds the file that makes
// it whatever else selection decides.
func recoveredInputPaths(dir string, pathTranslator *rojo.PathTranslator, previousOutputs map[string]string, presence *outputPresenceIndex) map[string]struct{} {
	recovered := map[string]struct{}{}
	for outputPath := range previousOutputs {
		if presence.hasRegular(outputPath) {
			continue
		}
		absolutePath := filepath.Join(filepath.FromSlash(dir), filepath.FromSlash(outputPath))
		inputPath := strings.TrimSuffix(absolutePath, ".map")
		for _, candidate := range pathTranslator.GetInputPaths(inputPath) {
			recovered[normalizeSourceFilePath(candidate)] = struct{}{}
		}
	}
	return recovered
}
