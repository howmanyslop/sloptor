package compile

import (
	"path/filepath"
	"strings"

	"rotor/tsgo/ast"
)

// narrowedSidecarRoots is the LanguageService root set an incremental round
// trip asks the worker for: the files being compiled plus every declaration
// file in the project.
//
// The worker's program is built from scratch inside the rotor process and
// thrown away when it exits, so parsing and binding the 2,800 files of a
// project to transform three of them buys nothing. TypeScript pulls each
// root's import closure into the program on its own, so every type a
// transformer asks the checker about a compiled file is still reachable.
// Declaration files are added whole because an ambient one can contribute
// globals that nothing imports, and dropping those would change the checker's
// answers rather than only make the program smaller.
//
// HAZARD: this is sound for a transformer that reaches the program through the
// files it is handed, which is what the transformer API is shaped for. A
// transformer that enumerates program.getSourceFiles() to build program-wide
// state sees fewer files than it would in a full build. rbxts-transformer-jecs,
// rbxts-transformer-jest, the React-compiler applier and native Flamework are
// all per-file. See docs.md, "External transformer plugins".
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
		if strings.HasSuffix(sourceFile.FileName(), ".d.ts") {
			add(sourceFile.FileName())
		}
	}
	return roots
}
