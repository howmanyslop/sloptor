package compile

import (
	"strings"

	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/wrapvfs"
)

// Synthetic ambient declaration for three compile-time introspection macros
// (see internal/transformer/{nameofmacro,keysmacro,filemacro}.go). It is
// injected like the $env declaration (envdecl.go), with the same coexistence
// guard so a project carrying the generated on-disk companion does not collide
// with the in-memory one.
//
// Parity safety: identical to envdecl.go's rationale — the file is a declaration
// file (excluded from compiled sources and CommonSourceDirectory), appended
// AFTER the config-derived root files, global under moduleDetection:"force",
// and code that never mentions these macros resolves no differently. They did
// not compile under rbxtsc (undeclared identifiers), so inlining them is a
// strict superset.
const macroDeclFileName = "__rotor_macros.d.ts"

// macroDeclBody is the one true declaration for the three macros. The synthetic
// in-memory file (macroDeclText) and the generated on-disk editor companion
// (MacroDeclFileText) must carry EXACTLY this declaration so the two are
// interchangeable — only their leading comments differ.
const macroDeclBody = `declare function $nameof(item: unknown): string;
declare function $keys<T>(): string[];
declare function $file(path: string): any;
`

const macroDeclText = `// rotor compiler extension (synthetic, in-memory): $nameof, $keys, and $file.
// This file is injected by the rotor
// compiler and does not exist on disk.
` + macroDeclBody

// MacroDeclFileName is the legacy editor companion for the $nameof / $keys /
// $file macros, superseded by the consolidated rotor.d.ts
// (rotortypes.go). The name is still recognised so a project that carries it
// keeps type-checking those macros (the coexistence guard suppresses the
// synthetic declaration) and `rotor clean --types` removes it.
const MacroDeclFileName = "rotor-macros.d.ts"

// macroDeclSubstrings are the cheap substring sentinels used to detect whether
// any project source references one of the three macros (the same kind of scan
// build/check use for $env). A hit on any one drives the on-disk
// rotor-macros.d.ts refresh.
var macroDeclSubstrings = []string{"$nameof", "$keys", "$file"}

// SourceUsesMacros reports whether text references any of the three macros.
func SourceUsesMacros(text string) bool {
	for _, sub := range macroDeclSubstrings {
		if strings.Contains(text, sub) {
			return true
		}
	}
	return false
}

// projectDeclaresMacrosOnDisk reports whether the parsed program's root files
// already include an on-disk rotor-macros.d.ts that declares these macros. When
// they do, the synthetic in-memory declaration must NOT also be appended — two
// global declarations of the same function would be a duplicate-identifier
// error. The content check is tolerant (any `declare function $nameof`).
func projectDeclaresMacrosOnDisk(fs vfs.FS, fileNames []string) bool {
	for _, name := range fileNames {
		base := name
		if i := strings.LastIndexByte(name, '/'); i >= 0 {
			base = name[i+1:]
		}
		if !strings.EqualFold(base, MacroDeclFileName) && !strings.EqualFold(base, RotorTypesFileName) {
			continue
		}
		if text, ok := fs.ReadFile(name); ok && strings.Contains(text, "declare function $nameof") {
			return true
		}
	}
	return false
}

// macroDeclPath places the synthetic declaration next to the tsconfig (slash
// separated, like all vfs paths).
func macroDeclPath(configPath string) string {
	return tspath.GetDirectoryPath(configPath) + "/" + macroDeclFileName
}

// injectMacroDeclFS wraps fs so the synthetic declaration file exists at
// declPath. Same two-interception shape as injectEnvDeclFS.
func injectMacroDeclFS(inner vfs.FS, declPath string) vfs.FS {
	matches := func(path string) bool {
		if inner.UseCaseSensitiveFileNames() {
			return path == declPath
		}
		return strings.EqualFold(path, declPath)
	}
	return wrapvfs.Wrap(inner, wrapvfs.Replacements{
		FileExists: func(path string) bool {
			return matches(path) || inner.FileExists(path)
		},
		ReadFile: func(path string) (string, bool) {
			if matches(path) {
				return macroDeclText, true
			}
			return inner.ReadFile(path)
		},
	})
}
