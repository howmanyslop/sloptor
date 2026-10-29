package compile

import (
	"rotor/internal/config"
)

// effectiveNativeTypes resolves the rotor `nativeTypes` option for one
// project. The option has two declaration surfaces and a fixed precedence:
//
//  1. tsconfig.json's `rbxts.nativeTypes`, including through `extends`,
//  2. rotor.toml's top-level `nativeTypes`,
//  3. fallback, the caller's ProjectOptions.NativeTypes (false unless set).
//
// A surface that does not declare the key at all is skipped, so an explicit
// `false` in tsconfig correctly wins over `true` in rotor.toml. `dir` is the
// project directory rotor.toml is read from; `tsConfigPath` is the active
// tsconfig (an unreadable or missing one declares nothing).
//
// Unreadable configuration never fails the compile here: a malformed
// tsconfig.json has already been reported by the program build, and rotor.toml
// problems are only fatal where another consumer (Flamework) needs them. Both
// degrade to "undeclared", which keeps the pre-option output.
func effectiveNativeTypes(dir, tsConfigPath string, fallback bool) bool {
	if declared, err := ReadRbxtsOptions(tsConfigPath); err == nil && declared != nil && declared.NativeTypes != nil {
		return *declared.NativeTypes
	}
	if cfg, err := config.Load(dir); err == nil && cfg.NativeTypes != nil {
		return *cfg.NativeTypes
	}
	return fallback
}
