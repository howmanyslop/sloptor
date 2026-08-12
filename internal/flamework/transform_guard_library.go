package flamework

import (
	"os"
	"path/filepath"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
)

const flameworkDiagnosticCode = " @flamework/core"

func flameworkGuardLibrary(state *TransformState, file *ast.SourceFile) string {
	if state.guardLibrary != "" {
		return state.guardLibrary
	}
	coreRoot, coreOK := resolveNodeModuleRoot(filepath.Dir(file.FileName()), "@flamework/core")
	if !coreOK {
		state.AddDiagnostic(flameworkWarningAtEOF(file, "Flamework core was not found, guard generation may not work."))
		return "@rbxts/t"
	}
	fileGuard, fileGuardOK := resolveNodeModuleRoot(filepath.Dir(file.FileName()), "@rbxts/t")
	coreGuard, coreGuardOK := resolveNodeModuleRoot(coreRoot, "@rbxts/t")
	if fileGuardOK && coreGuardOK && filepath.Clean(fileGuard) == filepath.Clean(coreGuard) {
		state.guardLibrary = "@rbxts/t"
		return state.guardLibrary
	}
	expectedCoreGuardRoot := filepath.Join(coreRoot, "node_modules", "@rbxts", "t")
	if !coreGuardOK || !pathWithin(expectedCoreGuardRoot, coreGuard) {
		state.AddDiagnostic(flameworkWarningAtEOF(file, "Valid `@rbxts/t` was not found, guard generation may not work."))
		return "@rbxts/t"
	}
	state.guardLibrary = flameworkPreludeModule
	return state.guardLibrary
}

func flameworkWarningAtEOF(file *ast.SourceFile, message string) *ast.Diagnostic {
	end := file.End()
	return ast.NewDiagnosticWithStringCode(file, core.NewTextRange(end, end), flameworkDiagnosticCode, diagnostics.CategoryWarning, message)
}

func resolveNodeModuleRoot(start, module string) (string, bool) {
	for current := filepath.Clean(start); ; current = filepath.Dir(current) {
		candidate := filepath.Join(current, "node_modules", filepath.FromSlash(module))
		if info, err := os.Stat(filepath.Join(candidate, "package.json")); err == nil && info.Mode().IsRegular() {
			resolved, resolveErr := filepath.EvalSymlinks(candidate)
			if resolveErr == nil {
				candidate = resolved
			}
			return candidate, true
		}
		parent := filepath.Dir(current)
		if parent == current {
			return "", false
		}
	}
}
