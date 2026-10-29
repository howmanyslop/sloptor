package compile

import (
	"rotor/internal/config"
)

// effectiveLuauTypes resolves the experimental rotor `luauTypes` option for
// one project. The option has three declaration surfaces and a fixed
// precedence:
//
//  1. the command line's `--luauTypes` (argv, the ProjectOptions.SolutionArgv
//     layer; nil or an unset field when the flag was not passed),
//  2. tsconfig.json's `rbxts.luauTypes`, including through `extends`,
//  3. rotor.toml's top-level `luauTypes`,
//  4. fallback, the caller's ProjectOptions.LuauTypes (false unless set).
//
// A surface that does not declare the key at all is skipped, so an explicit
// `false` on the command line or in tsconfig correctly wins over `true` in
// rotor.toml. `dir` is the project directory rotor.toml is read from;
// `tsConfigPath` is the active tsconfig (an unreadable or missing one declares
// nothing).
//
// Unreadable configuration never fails the compile here: a malformed
// tsconfig.json has already been reported by the program build, and rotor.toml
// problems are only fatal where another consumer (Flamework) needs them. Both
// degrade to "undeclared", which keeps the pre-option output.
func effectiveLuauTypes(dir, tsConfigPath string, argv *RbxtsOptions, fallback bool) bool {
	if argv != nil && argv.LuauTypes != nil {
		return *argv.LuauTypes
	}
	if declared, err := ReadRbxtsOptions(tsConfigPath); err == nil && declared != nil && declared.LuauTypes != nil {
		return *declared.LuauTypes
	}
	if cfg, err := config.Load(dir); err == nil && cfg.LuauTypes != nil {
		return *cfg.LuauTypes
	}
	return fallback
}
