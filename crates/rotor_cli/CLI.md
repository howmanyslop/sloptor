# rotor_cli

The `rotor-rs` binary: the Rust port of `cmd/rotor`, on clap. The command tree, flags, exit
codes and every `--json` byte match Go; help, errors, completions and human output are
redesigned. Stage 7a Phase 1 wires `build`, `check`, `clean`, `schema`, `completion` and
`version`; the other commands parse their flags, then fail with a "not ported" error until
Stage 7b.

## Go file to Rust module

| Go (`cmd/rotor`) | Rust (`src/`) | Status |
| --- | --- | --- |
| `main.go` | `main.rs` | ported |
| `cli.go` (Cobra tree, help, error policy) | `cli.rs` (clap derive tree), `app.rs` (exit codes, dispatch, help and error rendering) | ported; help redesigned |
| `cli.go` (`normalizeCompilerInvocation`, `normalizeLegacyArgs`) | `argv.rs` | ported; clap reads the optional values itself |
| `options.go` | `options.rs` | ported |
| `build.go` | `build.rs`, `json.rs` | ported, except watch |
| `check.go` | `check.rs`, plus `rotor_tsrs::project::load_check_program` | ported, except watch |
| `clean.go` | `clean.rs` | ported |
| `schema.go` | `app.rs` | ported |
| `completion.go` | `app.rs` (`clap_complete`) | redesigned |
| `ui.go` | `ui.rs` | Go's content for now; Phase 2 replaces it |
| — | `term.rs` | the one writer edge and the colour policy |
| `profiles.go` | `build.rs` | flags kept; they report a clear error |
| `runtime_policy*.go` | — | Phase 2 |
| `diagnostics.go`, `lsp.go`, `api_server.go`, `gen_docs.go` | `cli.rs` (flags only) | Stage 7b |
| `watch.go`, `watch_json.go`, `solution_watch*.go` | — | Stage 7b |

## Flag names

Each flag's primary name is kebab-case. Go's camelCase name stays as a hidden alias, so scripts
keep working. Short flags and names that were already kebab-case or one word are unchanged.

| Kebab name (primary) | Go name (hidden alias) | Command |
| --- | --- | --- |
| `--include-path`, `-i` | `--includePath` | build |
| `--no-include` | `--noInclude` | build |
| `--write-only-changed` | `--writeOnlyChanged` | build |
| `--luau-types` | `--luauTypes` | build |
| `--emit-declaration-only` | `--emitDeclarationOnly` | build |
| `--allow-comment-directives` | `--allowCommentDirectives` | build |
| `--log-truthy-changes` | `--logTruthyChanges` | build |
| `--write-transformed-files` | `--writeTransformedFiles` | build |
| `--optimized-loops` | `--optimizedLoops` | build |
| `--optimized-array-appends` | `--optimizedArrayAppends` | build |
| `--use-polling` | `--usePolling` | build |
| `--single-threaded` | `--singleThreaded` | build |

These are unchanged: `-p/--project`, `-b/--build`, `--builders`, `--checkers`, `-w/--watch`,
`--verbose`, `--type`, `--rojo`, `--luau`, `--json`, `--bell`, `--clear`, `--max-errors`,
`--timings`, `--cpuprofile`, `--trace-out`, `--blockprofile`, `--mutexprofile`, `--heapprofile`,
`-v/--version`, `--types`, `-n/--dry-run`, `--rbxts` and `--stdio`.

A boolean takes pflag's forms: `--flag`, `--flag=<bool>` (any `strconv.ParseBool` spelling) and
`--no-flag[=<bool>]`, under either name (`--no-optimizedLoops` works). `--flag value` never
consumes the value. Every flag may repeat, and the last value wins. `--build`, `--rojo` and
`--max-errors` take an optional value, as in Go: a bare `--build` builds the project's
references, a bare `--rojo` discovers the Rojo project, and a bare `--max-errors` means 0. A bare
`--project` or `--include-path` is the empty path, as in Go.

## Exit codes

| Code | When |
| --- | --- |
| 0 | success, `--help`, `-h`, `--version`, `version` |
| 1 | any failure: compile and type errors, config errors, usage errors (clap's included), "not ported" |

clap's own usage errors exit 2 by default; `app.rs` maps them to 1, as Go does.

## Colour

`--color auto|always|never` is global and read before clap parses, so usage errors follow it
too. Under `auto`, `NO_COLOR` turns colour off, then `CLICOLOR_FORCE` or `FORCE_COLOR` (other
than `0`) turns it on, then `CLICOLOR=0` turns it off; otherwise a stream is coloured when it is
a terminal and `TERM` is not `dumb`. stdout and stderr are decided separately. `--json` output
and completion scripts never contain escapes.

## Completions

`sloptor completion <shell>` (alias `completions`) generates a static script with
`clap_complete` for bash, zsh, fish, elvish and powershell. Path arguments carry
`ValueHint::AnyPath`, `FilePath` or `DirPath`, so zsh and fish complete files and directories.
Booleans complete as plain flags, and the camelCase aliases are not offered.

Dynamic completion of `tsconfig*.json` paths through `clap_complete::CompleteEnv` is not used.
It sits behind clap_complete's `unstable-dynamic` feature, which also turns on
`clap/unstable-ext`. Neither is covered by semver, and both replace the static script with
`COMPLETE=<shell> sloptor`, a different install step from Go's.

## Intentional differences from Go

- **Help and usage errors** come from clap: grouped headings (Input, Output, Emit, Optimization,
  Watch, Diagnostics, Runtime, Advanced, General), `-h` short and `--help` long with examples,
  value names and possible values, and `tip:` suggestions. A misspelt or unaliased camelCase flag
  suggests the kebab name (`--maxErrors` gives `--max-errors`), never a hidden alias. A usage
  error prints clap's `Usage:` line and `For more information, try '--help'.` rather than Go's
  `Run 'sloptor build --help' for usage.`.
- **Error wording.** The implication errors name the kebab flags: `--use-polling requires
  --watch` and `--emit-declaration-only requires --build` replace yargs' `Implications failed:`
  text. Value errors are clap's (`invalid value '0' for '--checkers <N>': must be a positive
  integer`, `a value is required for '--checkers <N>'`). Unknown commands and flags are clap's
  `unrecognized subcommand` and `unexpected argument`.
- **`--type` without a value** is a usage error from clap, where Go reported the invalid empty
  choice. Both exit 1.
- **Profiles.** `--cpuprofile`, `--trace-out`, `--blockprofile`, `--mutexprofile` and
  `--heapprofile` keep their parsing, their `requires a path` and `cannot be used with --watch`
  checks, and the duplicate-path check (symlinks and hard links included). Go runtime profiles
  have no Rust equivalent, so a build given one then fails with an error naming a sampling
  profiler, before it builds. `--timings` is fully supported.
- **Completion scripts** are `clap_complete`'s static scripts, not Cobra's `__complete`
  protocol, and add elvish. `completions` is an alias of `completion`.
- **The root command** prints `error: a command is required` when given no command, where Go
  printed `no command given`. Both exit 1.
- **Not yet ported (Stage 7b):** `diagnostics`, `lsp`, `api-server`, `gen-docs`,
  `build --watch` and `check --watch`. They parse their flags, so a usage error is still a usage
  error, then exit 1 with `` `sloptor <command>` is not ported to the Rust CLI yet``.
- **Runtime policy** (`GOGC`, `GOMEMLIMIT`, `GOMAXPROCS`, `ROTOR_MAX_BUILDS` admission) is not
  applied yet. Phase 2 maps it to rayon and checker counts.
- **Known checker gap:** rotor's tsgo removed the `Object` top-level collision check for CommonJS
  (`12f44893`), but tsrs still reports it as TS2441. The test for it is ignored until the tsrs
  mirror carries the fix.

## Tests

`src/*` unit tests cover argv normalization, the option merge, the JSON encoder and the build
flag semantics. `tests/cli.rs` runs the binary end to end. `tests/go_parity.rs` compares
`--json`, `schema` and `version` output byte for byte, and exit codes, with a Go build. It is
ignored by default:

```sh
go build -trimpath -o /tmp/rotor-go ./cmd/rotor
ROTOR_GO_BIN=/tmp/rotor-go mise x -- cargo nextest run -p rotor_cli --run-ignored only -E 'binary(go_parity)'
```

These `cmd/rotor` tests were dropped or rewritten in Stage 7a:

| Go test | Why |
| --- | --- |
| `TestRootHelpRendersCargoStyleLayout`, `TestBuildHelpShowsOptionsOrder`, `TestBuildHelpShowsConcurrencyControls` | pinned Cobra help text; rewritten as `help_is_grouped_and_shows_kebab_names_only` |
| `TestInvalidCommandRendersCompactError`, `TestSubcommandUsageErrorNamesTheCommand` | pinned Go's error hint; rewritten against clap's `Usage:` line |
| `TestCompletionGeneratesNativeScripts`, `TestCompletionHelpShowsInstallExamples` | pinned Cobra's `__complete` protocol and help; rewritten for `clap_complete` |
| `TestNoColorHelpAndEvents` | rewritten as `colour_follows_the_flag_and_the_environment` |
| `TestBuildWritesCombinedProfiles`, `TestFailedBuildFinalizesCombinedProfiles`, `TestBuildProfilesStopReportsFinalizationErrors` | Go pprof; rewritten as `go_profiles_fail_clearly_before_building` |
| `TestBuildSuccessStructure`, `TestUIEvents*`, `TestRenderDiagFrames` | pinned Go's chrome, which Phase 2 replaces; code frames stay covered by `styled_build_failure_renders_code_frames_on_stderr` and `rotor_support::diagframe` |
| `TestJSONDiagnostics_prefersConcreteStringCodeAndMessage` | tsrs has no string-code diagnostics |
| `TestCheckProgramCheckerOverride`, `TestCmdCheckJSONKeepsCompilerWarningsOffStdout`, `TestBuildTimingsNormalOutputUnchanged`, `TestBuildSolutionPropagatesCheckers` | not ported yet: they need program or coordinator internals, or a compiler-warning source |
| `runtime_policy*_test.go` | with the runtime policy, in Phase 2 |
| `diagnostics*_test.go`, `task7_diag_preconditions_test.go`, `lsp_test.go`, `gen_docs_test.go`, `watch*_test.go`, `solution_watch*_test.go`, `package_watch_test.go` | Stage 7b |
| `parallel_transformer_test.go`, `concurrency_system_test.go`, `build_solution_reuse_test.go` | compile-level behaviour that needs the Node sidecar or fixtures; not ported in Stage 7a |
