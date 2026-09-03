package compile

import (
	"path/filepath"

	"rotor/tsgo/ast"
	"rotor/tsgo/tspath"
)

// narrowedSidecarRoots is the LanguageService root set an incremental round
// trip asks the worker for: the files being compiled, plus every file that can
// change the checker's answers about them without being imported.
//
// The worker's program is built from scratch inside the rotor process and
// thrown away when it exits, so parsing and binding the 2,800 files of a
// project to transform three of them buys nothing. TypeScript pulls each
// root's import closure into the program on its own, so every type a
// transformer asks the checker about a compiled file is still reachable
// through the compiled file's own imports.
//
// What is NOT reachable that way is anything that reaches a file without being
// imported, and dropping one of those would change the checker's answers
// rather than only make the program smaller. Three shapes qualify, and
// contributesAmbientDeclarations keeps all of them.
//
// HAZARD: this is sound for a transformer that reaches the program through the
// files it is handed, which is what the transformer API is shaped for. A
// transformer that enumerates program.getSourceFiles() to build program-wide
// state sees fewer files than it would in a full build.
// rbxts-transformer-jecs, rbxts-transformer-jest, the React-compiler applier
// and native Flamework are all per-file. See docs.md, "External transformer
// plugins".
//
// A full build compiles every file, so the narrowed set would name the whole
// project anyway; returning nil leaves the request unnarrowed and lets the
// worker keep whatever root set it already has.
func narrowedSidecarRoots(compileFiles, projectFiles []*ast.SourceFile) []string {
	if len(compileFiles) == 0 || len(compileFiles) >= len(projectFiles) {
		return nil
	}
	roots := make([]string, 0, len(compileFiles))
	seen := make(map[string]struct{}, len(compileFiles))
	add := func(name string) {
		path := filepath.FromSlash(name)
		if _, ok := seen[path]; ok {
			return
		}
		seen[path] = struct{}{}
		roots = append(roots, path)
	}
	for _, sourceFile := range compileFiles {
		add(sourceFile.FileName())
	}
	for _, sourceFile := range projectFiles {
		if contributesAmbientDeclarations(sourceFile) {
			add(sourceFile.FileName())
		}
	}
	return roots
}

// contributesAmbientDeclarations reports whether a project file can reach
// another file's meaning without that file importing it, which is what makes
// it unsafe to drop from a narrowed root set:
//
//   - a declaration file (`.d.ts`, `.d.mts`, `.d.cts`), which exists to declare
//     things nothing has to import;
//   - a source file that is not an external module — a global script, whose
//     every top-level name is a global;
//   - a module that opens `declare global { ... }`, which the parser records as
//     an augmentation named `global`.
//
// The first two are cheap. The third needs the parsed file, which the caller
// already has: these are the program's own source files, not paths.
func contributesAmbientDeclarations(sourceFile *ast.SourceFile) bool {
	if tspath.IsDeclarationFileName(sourceFile.FileName()) {
		return true
	}
	if !ast.IsExternalModule(sourceFile) {
		return true
	}
	for _, augmentation := range sourceFile.ModuleAugmentations {
		// A string-literal name augments a named module, which only a file
		// that imports that module can see. An identifier name is `global`.
		if augmentation != nil && !ast.IsStringLiteral(augmentation) {
			return true
		}
	}
	return false
}
