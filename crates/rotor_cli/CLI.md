# rotor_cli

The `sloptor` binary: the Rust port of `cmd/rotor`, on clap. The command tree, flags, exit
codes and every `--json` byte match Go; help, errors, completions and human output are
redesigned. Stage 7a wires `build`, `check`, `clean`, `diagnostics`, `schema`, `completion`,
`gen-docs` and `version`, and the runtime policy. Stage 7b adds `build --watch`,
`check --watch`, `lsp` (crate `rotor_lsp`) and the hidden `api-server`.

## Go file to Rust module

| Go (`cmd/rotor`) | Rust (`src/`) | Status |
| --- | --- | --- |
| `main.go` | `main.rs` | ported |
| `cli.go` (Cobra tree, help, error policy) | `cli.rs` (clap derive tree), `app.rs` (exit codes, dispatch, help and error rendering) | ported; help redesigned |
| `cli.go` (`normalizeCompilerInvocation`, `normalizeLegacyArgs`) | `argv.rs` | ported; clap reads the optional values itself |
| `options.go` | `options.rs` | ported |
| `build.go` | `build.rs`, `json.rs` | ported |
| `check.go` | `check.rs`, plus `rotor_tsrs::project::load_check_program` | ported |
| `clean.go` | `clean.rs` | ported; output redesigned |
| `diagnostics.go` | `diagnostics.rs` | ported; JSON byte-identical, text output redesigned |
| `gen_docs.go` | `gen_docs.rs` | ported; pages come from the clap tree |
| `schema.go` | `app.rs` | ported |
| `completion.go` | `app.rs` (`clap_complete`) | redesigned |
| `ui.go` | `ui.rs` (verbs, header, formatters), `frames.rs` (diagnostic frames), `live.rs` (progress line, project lines), `report.rs` (result block, `--verbose` report) | redesigned |
| — | `term.rs` | the one writer edge and the colour policy |
| — | `rotor_compile::progress` | the atomics the live view samples |
| `profiles.go` | `build.rs` | flags kept; they report a clear error |
| `runtime_policy*.go`, `internal/admission` | `runtime.rs` | ported where Rust has an equivalent |
| `watch.go` | `watch.rs` | ported; output redesigned |
| `watch_json.go` | `watch_json.rs` | ported; NDJSON byte-identical |
| `solution_watch.go` | `solution_watch.rs` | ported |
| `solution_watch_coverage.go` | `solution_watch_coverage.rs` | ported |
| `lsp.go` | `lsp.rs` | ported |
| `api_server.go` | `api_server.rs` | ported |
| `internal/buildapi` | `buildapi.rs` | ported; responses byte-identical |
| `internal/lsp` | crate `rotor_lsp` | ported; messages byte-identical |
| `tsgo/fswatch` (the surface the solution watch uses) | `fswatch.rs`, on `notify` | ported |
| — | `memory.rs` | gives a pass's freed heap back after each watch rebuild and census |

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
| 1 | any failure: compile and type errors, config errors, usage errors (clap's included) |
| 130 | a watch session ended by Ctrl+C (or SIGTERM, SIGHUP) |

clap's own usage errors exit 2 by default; `app.rs` maps them to 1, as Go does.

## Colour

`--color auto|always|never` is global and read before clap parses, so usage errors follow it
too. Under `auto`, `NO_COLOR` turns colour off, then `CLICOLOR_FORCE` or `FORCE_COLOR` (other
than `0`) turns it on, then `CLICOLOR=0` turns it off; otherwise a stream is coloured when it is
a terminal and `TERM` is not `dumb`. stdout and stderr are decided separately. `--json` output
and completion scripts never contain escapes.

## Human output

Every command opens with a header (`sloptor vX  ·  <command>  ·  <project>`), then writes
cargo-style lines with the verb right-aligned in a 12-column gutter (`Compiled`, `Built`,
`Fresh`, `Failed`, `Checked`, `Removed`, `Censused`, `Finished`).

- **Progress line.** `rotor_compile::progress::BuildProgress` holds atomics that the workers bump
  per file and per project, plus one lock taken per finished project. `live.rs` samples it from
  its own thread at most 15 times a second and draws one line on stderr: the phase, a bar,
  files (or projects for `-b`), elapsed time and the projects building. It draws only when
  stderr is a coloured terminal and the build has run for 150 ms. A `logservice` hook clears
  the line before any log line, so `--verbose` lines never share it.
- **Solution builds** (`-b`) print a line per project in finish order (`Built`, `Fresh`,
  `Failed`, `Blocked`), on a pipe too, then a summary of projects, files compiled, written and
  unchanged, wall time and throughput.
- **Diagnostics** render as rustc-style frames: `error[TS2322]`, the path and line:column, the
  snippet with its span underlined, tsc's related information as a `= note:` with its own
  snippet, and `= help:` for the `Suggestion:` line of rotor's own errors. Then the counts.
- **`--verbose`** keeps Go's log lines, then adds the per-phase chart, the five slowest files by
  transform time and, with `-b`, the five slowest projects.
- **Plain mode** (`--color never`, `NO_COLOR`, a pipe) prints the same text with no escapes and
  no progress line. `--json` output does not change.

The pieces (`ui::header`, `ui::verb_line`, `frames::render_all`, `live::project_line`,
`report::verbose`) are independent; the watch loop reuses them.

## Watch mode, the language server and the build API

- **Watchers.** `build --watch` and `check --watch` on one project poll, as Go does: a stamp
  per file (size, time and, for small content-sensitive files, a SHA-256), a 100 ms to 1 s
  interval scaled to the walk's cost, and a 50 ms quiet window capped at 500 ms. `build -b
  --watch` uses file-system events through `notify` on every platform (FSEvents on macOS,
  inotify on Linux, `ReadDirectoryChangesW` on Windows), behind `fswatch.rs`, which ports
  `tsgo/fswatch`'s event coalescing and debounce (the first event after 500 ms of quiet runs at
  once, others wait for 50 ms of quiet). tsrs mirrors no watcher.
- **Human output.** The initial build prints as `sloptor build` does. Each rebuild opens with a
  timestamped divider naming up to three changed files, then the frames, then
  `Rebuilt N files in X · N written · M unchanged · rebuild #k` or
  `Failed rebuild #k in X · counts · files`, then `Watching N files · last K ▁▅█ · Ctrl+C to
  stop` (the sparkline only in colour) or `Waiting N errors · watching N files`. `--clear` (on by
  default, as in Go) clears the screen before a rebuild on a terminal; `--bell` rings when a
  rebuild flips between passing and failing. On a pipe the text is plain.
- **`--json`** writes Go's NDJSON events (`buildStart`, `buildEnd`, `watching`), byte-identical
  except the `at` and `durationMs` values.
- **Memory.** A watch session, the language server and the build API mark the process
  long-lived (`rotor_tsrs::project::set_long_lived`): each build, check or census runs in a
  `ProgramScope` that frees its programs, their checker regions, the file loader's regions and
  the processed-file data, and the Flamework preparse and declaration prefetch threads allocate
  in regions the scope frees. Its threads are kept for the session and reused
  (`rotor_tsrs::threads`, patch 0012 for tsrs's checker threads) instead of started and ended
  per pass. After each pass `memory.rs` returns mimalloc's freed pages (`mi_collect`) on every
  thread, idle ones included, and tsrs's released slabs. ps RSS includes mimalloc's reclaimable
  pages and keeps rising; the footprint is the figure that plateaus. See
  `rotor_compile/COMPILE.md` for the numbers.
- **`lsp`** runs its censuses one at a time on one long-lived large-stack thread, each in its
  own program scope. A census that panics answers with that panic as its error, as Go's
  `census` recovers it, and the thread serves the next one. A known request whose params Go's
  `lsproto` would fail to decode is answered `InvalidParams` (-32602); the check is the params'
  top level only (`rotor_lsp/src/params.rs`, generated from `lsp_generated.go`).
- **`api-server`** decodes requests as a stream (no size limit) and keeps a transformer
  callback's result bytes exact (serde_json `RawValue`), so the transport metrics match Go's.
  A build that panics ends the process with status 2, as Go's unrecovered panic does, so the
  client sees EOF instead of a build that never answers.

## Runtime policy

`runtime.rs` ports `runtime_policy*.go` and `internal/admission` for `build`, `check` and
`diagnostics`.

- **Admission** (`ROTOR_MAX_BUILDS`): the same slot count rule (`0` turns it off) and the same
  slot directory and flock files as Go, so Go and Rust builds share the machine's slots. A build
  that waits more than a second says so on stderr.
- **Workers.** Rust has no `GOMAXPROCS`, so the share sizes the tsrs worker pool through
  `RAYON_NUM_THREADS`: `max(2, cpus / share)`, as Go's `GOMAXPROCS`.
  A caller's `RAYON_NUM_THREADS` wins, then `GOMAXPROCS`. The share reaches the Node sidecar's
  shard count, as in Go.
- **`GOGC` and `GOMEMLIMIT`** have no equivalent without a garbage collector, so they are not
  applied. The memory budget (cgroup limits on Linux, `hw.memsize` on macOS) still feeds the
  slot count.
- **Slot counts can differ from Go.** Go reads total memory through its OS stats, which report
  less than `hw.memsize` on macOS, so a 24 GiB Mac gets 3 slots from Go and 4 from Rust.
- **No mid-build refresh.** Go re-reads the active slot count while it runs; Rust sizes the pool
  once at admission, because rayon's global pool cannot be resized.

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
- **`diagnostics` stack traces.** stdin works as in Go: unless it is a terminal, the overlay
  request is read to EOF and its first JSON value decoded, so a script with no request must pass
  `< /dev/null`. In the JSON output an internal compiler error's `stack` is `""`, since a Rust
  panic carries no Go stack.
- **`gen-docs`** writes the same Starlight MDX layout as Go, built from the clap tree: options
  are grouped by help heading and the camelCase aliases get their own column.
- **Ctrl+C in a watch session** exits 130, the code a shell reports for Go's default signal
  exit, after clearing a progress line that may be on screen. Nothing else needs restoring: a
  session never changes the terminal's modes.
- **`--use-polling` with `-b`** polls through `notify`'s `PollWatcher` (100 ms). Go accepts the
  flag (it only checks that `--watch` is given) but always uses OS events.
- **Watch human output** is redesigned (above). A solution rebuild prints the same compact lines
  as a single-project one, and `unchanged` is left out when it is 0.
- **The divider's clock** is local time from `jiff`.
- **The language server's typed decoding.** Go decodes each request's params into typed structs
  through go-json-experiment; the Rust server decodes the fields it reads with serde_json. A
  request whose params do not decode gets the same error codes for `initialize` and `shutdown`,
  but a malformed params object of another method can differ in the error's message text.
- **The `Object` collision check.** rotor's tsgo removed the CommonJS top-level `Object`
  collision check (`12f44893`). The tsrs mirror patch
  `0010-rotor-fork-object-collision.patch` removes it too, so neither `build` nor `check`
  reports TS2441 for a top-level `Object` binding.

## Tests

`src/*` unit tests cover argv normalization, the option merge, the JSON encoder and the build
flag semantics, the frames, the formatters, gen-docs, the runtime policy, the watch loops
(`watch_test.go`, `watch_flamework_test.go`, `watch_json_test.go`, `solution_watch_test.go`,
`solution_watch_coverage_test.go`, `package_watch_test.go`, over a fake watcher where Go uses
one), the build API (`server_test.go`, `timing_schema_contract_test.go`) and `lsp_test.go`.
`rotor_lsp/tests/server.rs` ports `internal/lsp`'s server tests. `tests/cli.rs`
runs the binary end to end, including `diagnostics`, `gen-docs` and the `Object` binding under
`build` and `check`. `tests/go_parity.rs` compares
`--json` (`build`, `check` and `diagnostics`, with stdin overlays), `schema` and `version` output byte for byte, and exit codes, with a Go build. It is
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
| `runtime_policy*_test.go` | rewritten as `runtime.rs` unit tests; the `GOGC`/`GOMEMLIMIT` cases have no equivalent |
| `diagnostics*_test.go`, `task7_diag_preconditions_test.go`, `gen_docs_test.go` | rewritten as `diagnostics_*` and `gen_docs_*` tests in `tests/cli.rs`, and the diagnostics cases in `tests/go_parity.rs` |
| `TestStderrWriter` | Go's `stderrWriter` type has no counterpart; the solution watch writes its errors to `std::io::stderr` |
| `TestCmdBuildJSONDiagnosticPathIsProjectRelative` | covered by `build_json_reports_success_and_diagnostics` in `tests/cli.rs` |
| `parallel_transformer_test.go`, `concurrency_system_test.go`, `build_solution_reuse_test.go` | compile-level behaviour that needs the Node sidecar or fixtures; not ported in Stage 7a |
