// Command rotor is the rotor CLI.
//
// `sloptor check` is a fast native TypeScript project checker; `sloptor build`
// compiles to Luau with the full rbxtsc build flag surface (ProjectOptions
// merged from defaults < tsconfig `rbxts` key < argv). Build watch mode is
// available; incremental rebuild selection remains later Phase 4 work.
//
// Exit-code policy: 0 = success, 1 = ANY failure including usage errors —
// matching upstream rbxtsc, whose yargs `.fail` handler sets exit code 1
// (CLI/cli.ts L30-35). rotor previously exited 2 for usage errors; that
// divergence was removed in Phase 4 for drop-in parity.
package main

import (
	"os"

	"rotor/internal/compile"
	rotorversion "rotor/internal/version"
)

// version is sloptor's own release version, used for `--version` and the
// `sloptor build` emit header (`-- Compiled with sloptor v...`). The value is
// defined in code (internal/version) — no ldflags injection; kept as a var so
// tests can override it.
var version = rotorversion.Version

func main() {
	// The hidden daemon child enters through main as well; enabling this twice
	// across the parent and child processes is intentional and idempotent.
	compile.EnablePersistentSidecarDaemon()
	os.Exit(run(os.Args[1:]))
}
