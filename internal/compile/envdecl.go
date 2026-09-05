package compile

import (
	"os"
	"path/filepath"
	"strings"

	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/wrapvfs"
)

// Synthetic ambient declaration for the rotor $env compile-time environment
// macro (a SUPERSET extension; see internal/transformer/envmacro.go). rotor
// controls program creation, so instead of asking users to install types (or
// auto-writing a rotor-env.d.ts into their project), an in-memory
// declaration file is added to every program's root files: the type checker
// then accepts `$env("NAME")`, `$env("NAME", "fallback")`, `$env.NAME`, and
// `$env["NAME"]` without any project changes.
//
// Parity safety:
//   - The file is a declaration file, so it is excluded from
//     projectSourceFiles (never compiled/emitted) AND from tsgo's
//     CommonSourceDirectory computation (tsgo/compiler/program.go:1536-1552
//     filters `!file.IsDeclarationFile`), so the PathTranslator's rootDir —
//     and therefore every output path and require chain — is unchanged.
//   - It is appended AFTER the config-derived root files, so existing
//     program file order is preserved.
//   - moduleDetection:"force" only forces non-declaration files into module
//     scope, so the declaration stays global.
//   - Code that never mentions `$env` resolves no differently; code that
//     mentions `$env` did not compile under rbxtsc (undeclared identifier),
//     so inlining it is a strict superset.
//
// Known limitation (documented in docs.md): a project that itself declares
// a GLOBAL `$env` (e.g. very old rbxts-transform-env typings) now gets a
// duplicate-declaration error — remove the plugin's global typings when
// using rotor's built-in macro. The current rbxts-transform-env exports
// `$env` as a MODULE export (`import { $env } from "rbxts-transform-env"`),
// which shadows the global per-file, does NOT conflict, and keeps compiling
// through the transformer-plugin sidecar (its imported symbol differs from
// the global one, so rotor's macro never intercepts it).
const envDeclFileName = "__rotor_env.d.ts"

// envDeclBody is the one true `$env` ambient declaration. The synthetic
// in-memory file (envDeclText) and the generated on-disk editor companion
// (EnvDeclFileText) must carry EXACTLY this declaration so the two are
// interchangeable — only their leading comments differ.
const envDeclBody = `declare const $env: {
	readonly [envName: string]: string | undefined;
} & ((name: string, fallback: string) => string) &
	((name: string) => string | undefined);
`

const envDeclText = `// rotor compiler extension (synthetic, in-memory): the $env compile-time
// environment macro. This file is injected by the rotor compiler and does
// not exist on disk.
` + envDeclBody

// EnvDeclFileName is the legacy per-macro editor companion for $env, superseded
// by the consolidated rotor.d.ts (rotortypes.go). The name is still recognised
// so a project that carries it keeps type-checking $env (the coexistence guard
// suppresses the synthetic declaration) and `rotor clean --types` removes it.
const EnvDeclFileName = "rotor-env.d.ts"

// atomicWriteFile writes data via a temp file in the target directory plus a
// rename so concurrent readers (editors, tsserver) never see partial content.
func atomicWriteFile(path string, data []byte) error {
	tmp, err := os.CreateTemp(filepath.Dir(path), filepath.Base(path)+".tmp-*")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		os.Remove(tmpName)
		return err
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmpName)
		return err
	}
	if err := os.Rename(tmpName, path); err != nil {
		os.Remove(tmpName)
		return err
	}
	return nil
}

// projectDeclaresEnvOnDisk reports whether the parsed program's root files
// already include an on-disk rotor-env.d.ts that declares $env. When they do,
// the synthetic in-memory declaration must NOT also be appended — two global
// `declare const $env` declarations would be a duplicate-identifier error
// (TS2451). The content check is tolerant (any `declare const $env`) so a
// hand-rolled or older-format declaration also suppresses the synthetic one
// instead of colliding with it.
func projectDeclaresEnvOnDisk(fs vfs.FS, fileNames []string) bool {
	for _, name := range fileNames {
		base := name
		if i := strings.LastIndexByte(name, '/'); i >= 0 {
			base = name[i+1:]
		}
		if !strings.EqualFold(base, EnvDeclFileName) && !strings.EqualFold(base, RotorTypesFileName) {
			continue
		}
		if text, ok := fs.ReadFile(name); ok && strings.Contains(text, "declare const $env") {
			return true
		}
	}
	return false
}

// envDeclPath places the synthetic declaration next to the tsconfig (slash
// separated, like all vfs paths).
func envDeclPath(configPath string) string {
	return tspath.GetDirectoryPath(configPath) + "/" + envDeclFileName
}

// injectEnvDeclFS wraps fs so the synthetic declaration file exists at
// declPath. Stat-level calls go through FileExists/ReadFile in tsgo's
// compiler host, so those two interceptions suffice (same shape as the
// source overlay FS in newOverlayFS).
func injectEnvDeclFS(inner vfs.FS, declPath string) vfs.FS {
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
				return envDeclText, true
			}
			return inner.ReadFile(path)
		},
	})
}
