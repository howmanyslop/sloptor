package compile

import (
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
	// emit yields the declaration text a set of source files produces. It must
	// be the same text the build will write: the comparison below is against
	// the hash the previous build recorded for the file it wrote.
	emit func(files []*ast.SourceFile) ([]declarationEmitFile, error)
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
) ([]*ast.SourceFile, []declarationEmitFile, error) {
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

	var declarations []declarationEmitFile
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
// caller should keep the plain reverse-closure selection.
//
// It needs a previous build to compare against (same salt, recorded output
// hashes) and a project that emits declarations at all — with declarations off
// there is no signature to compare.
func narrowSelectionByDeclarationSignature(
	dir string,
	program *compiler.Program,
	pathTranslator *rojo.PathTranslator,
	sourceFiles []*ast.SourceFile,
	extraSeeds map[string]struct{},
	current, previous *incrementalManifest,
	previousOutputs map[string]string,
	timings *BuildTimings,
) ([]*ast.SourceFile, []declarationEmitFile, bool, error) {
	if previous == nil || current == nil || previous.Salt != current.Salt {
		return nil, nil, false, nil
	}
	if len(previousOutputs) == 0 || !program.Options().GetEmitDeclarations() {
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
		emit: func(files []*ast.SourceFile) ([]declarationEmitFile, error) {
			// A wave's emit is a real tsgo declaration emit, so it is charged
			// to declarationEmit like the write path's own emit and taken back
			// out of incrementalSelection, which otherwise stops being a total
			// of its own bookkeeping.
			stop := logStage(program.Options().ConfigFilePath, declarationEmitStage)
			declarations, err := declarationTextsForSelection(program, files)
			timings.moveStageDuration(incrementalSelectionStage, declarationEmitStage, stop())
			return declarations, err
		},
	})
	if err != nil {
		return nil, nil, false, err
	}
	return selected, declarations, true, nil
}

// declarationTextsForSelection is the selection pass's half of the declaration
// emit seam: it drops the files this project emits no `.d.ts` for and hands the
// rest to emitDeclarationTexts, the same function the write path uses off the
// same original program. The bytes it returns are therefore the bytes the build
// writes, which is what lets the rule compare them against the hash the
// previous build recorded and then hand them straight to the writer.
func declarationTextsForSelection(program *compiler.Program, files []*ast.SourceFile) ([]declarationEmitFile, error) {
	emittable := make([]*ast.SourceFile, 0, len(files))
	for _, sourceFile := range files {
		if emitsDeclarationOutput(sourceFile) {
			emittable = append(emittable, sourceFile)
		}
	}
	return emitDeclarationTexts(program, emittable)
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
