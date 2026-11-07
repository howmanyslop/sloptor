# rotor_cli

The `sloptor` binary. The command tree is declared once in `src/cli.rs` with
[usage-rs](https://usage.jdx.dev/rust/)'s derive API; parsing, help, errors, shell completions,
the docs pages (`gen-docs`) and the [usage spec](https://usage.jdx.dev/spec/) all come from it.

| Module | Role |
| --- | --- |
| `cli.rs` | the command tree: every command, flag, default, example and help text |
| `argv.rs` | the one rewrite before parsing: a leading `--build` or `-b` selects `build` |
| `app.rs` | the exit-code owner: completion and spec requests, parsing, help, usage errors, dispatch |
| `term.rs` | the one writer edge and the colour policy |
| `gen_docs.rs` | the CLI reference as Starlight MDX, read from the spec |
| `build.rs`, `check.rs`, `clean.rs`, `diagnostics.rs`, `lsp.rs`, `api_server.rs` | the commands |
| `options.rs` | the merge of the defaults, tsconfig's `rbxts` key and the command line |

## Flag names

Each flag is kebab-case. rbxtsc's camelCase spelling stays as a hidden alias, so roblox-ts build
scripts keep working; help and completions show only the kebab name.

| Flag | Hidden alias | Command |
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

A leading `--build` or `-b` selects `build` (`sloptor --build tsconfig.json`), the shape Nx and
other TypeScript build integrations use.

### Booleans

A boolean takes `--flag`, `--flag=true`, `--flag=false` and `--no-flag`, and the last one given
wins. `--flag value` never consumes the value: `value` is the positional. Other spellings
(`--flag=1`, `--flag=TRUE`) are usage errors.

The booleans that layer over tsconfig's `rbxts` key (`--watch`, `--use-polling`, `--verbose`,
`--no-include`, `--luau`, `--luau-types`, `--optimized-loops`, `--optimized-array-appends`,
`--allow-comment-directives`, `--log-truthy-changes`, `--write-only-changed`,
`--write-transformed-files`) and `--single-threaded` need a third state, "not given", so the
config's value stands. usage's native negation only fits a plain `bool`, so each of these is an
`Option<bool>` (`--flag[=BOOL]` in help) with a hidden `--no-flag` partner, the two overriding
each other; `build::collect` folds the pair back into one `Option<bool>`. `--no-include` has no
partner. Every other boolean is a plain `bool`. Those worth turning off (`--json`, `--bell`, `--clear`,
`check --watch`, `--types`, `--dry-run`, `--rbxts`) have a native `--no-flag`, shown in help as
`--flag / --no-flag`; `--emit-declaration-only` and `lsp --stdio` take `=false` only.

### Values

`--build`, `--rojo`, `--project`, `--include-path` and `--max-errors` take an optional value: a
bare `--build` builds the project's references, a bare `--rojo` discovers the Rojo project, a
bare `--max-errors` means 0 (render every frame), and a bare `--project` or `--include-path` is
the empty path. `--builders` and `--checkers` take a positive integer and `--max-errors` a
non-negative one (`cli::Positive`, `cli::NonNegative`): `invalid value '0' for '--checkers':
must be a positive integer`. Every flag may repeat, and the last value wins.

## Exit codes

| Code | When |
| --- | --- |
| 0 | success, `-h`, `--help`, `help`, `-v`, `-V`, `--version`, `version` |
| 1 | an operational failure: compile and type errors, config errors, a missing project |
| 2 | a usage error: an unknown command or flag, a bad or missing value, a broken implication (`--use-polling requires --watch`), no command at all |
| 130 | a watch session ended by Ctrl+C (or SIGTERM, SIGHUP) |

`diagnostics` exits 0 whenever it produced a census. A bare `sloptor` prints the root help on
stderr and exits 2. The usage errors a command finds after parsing (`app::usage`) render like
usage's own: the message, the command's `Usage:` line, and `For more information, try
'--help'.`

## Colour

`--color auto|always|never` is global and read before usage parses (`term::color_choice`), so
usage errors follow it too. Under `auto`, `NO_COLOR` turns colour off, then `CLICOLOR_FORCE` or
`FORCE_COLOR` (other than `0`) turns it on, then `CLICOLOR=0` turns it off; otherwise a stream
is coloured when it is a terminal and `TERM` is not `dumb`. stdout and stderr are decided
separately. Help is rendered in cargo's colours, which sloptor's own output uses too. `--json`
output and completion scripts never contain escapes.

## Help and errors

Every help page opens with a banner (`sloptor vX  ·  <about>` at the root, `sloptor <command>  ·
<about>` below it), then usage's sections in the compact layout (`help_template` in `cli.rs`):
the usage line, the commands, the arguments, the flags under their headings, and the global
`-v`, `--color` and `-h`. `-h` stops there, apart from the root's pointer to `--help`; `--help`
adds the command's `after_long_help` (examples, and at the root the environment and exit codes),
written as authored so its columns survive. `app::help_page` assembles the page.
`sloptor help <command>` is the same as `sloptor <command> --help`.

Usage lines, `tip:` suggestions and error text are usage's. A misspelt flag's tip names every
close spelling the parser accepts, the camelCase aliases included.

## Completions

`sloptor completion <shell>` (alias `completions`) is usage's built-in command, for bash, zsh,
fish, elvish, nu and powershell. The script holds no command tree: on every Tab it calls
`sloptor __complete_word__`, which `app.rs` answers before parsing, so a script never goes stale
when sloptor is upgraded. `--install` writes the script where the shell looks for it. Path
arguments carry `ValueHint::AnyPath`, `FilePath` or `DirPath`, so shells complete files and
directories. Hidden flags and the camelCase aliases are not offered.

## The spec and gen-docs

`sloptor __usage_spec__` prints the CLI as a usage KDL spec, for `usage` tooling (`usage lint`,
generated man pages and Markdown). `cli::tests::the_spec_round_trips_through_usage` keeps it
parseable by `usage-lib`.

The hidden `sloptor gen-docs <dir>` writes the Starlight pages under
`documentation/src/content/docs/commands/` (`mise run docs:generate`; the pages are generated,
not committed). Each page has the usage line, the arguments, the flags under their help headings
with the camelCase aliases in their own column, the global flags, and the command's examples.

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

## Behaviour notes

- **`diagnostics` reads stdin.** Unless stdin is a terminal, the overlay request is read to EOF
  and its first JSON value decoded, so a script with no request must pass `< /dev/null`. An
  internal compiler error's `stack` is `""`.
- **`--use-polling` with `-b`** polls through `notify`'s `PollWatcher` (100 ms).
- **Ctrl+C in a watch session** exits 130 after clearing a progress line that may be on screen.
  Nothing else needs restoring: a session never changes the terminal's modes.
- **The `Object` collision check.** rotor's tsgo removed the CommonJS top-level `Object`
  collision check (`12f44893`). The tsrs mirror patch `0010-rotor-fork-object-collision.patch`
  removes it too, so neither `build` nor `check` reports TS2441 for a top-level `Object`
  binding.

## Tests

`src/*` unit tests cover the build flag semantics (parsed through the real command tree), the
spec round trip, argv normalization, the option merge, the JSON encoder, the frames, the
formatters, gen-docs, the runtime policy, the watch loops (over a fake watcher), the build API
and the language server. `rotor_lsp/tests/server.rs` covers the server end to end.
`tests/cli.rs` runs the binary end to end: exit codes, help, usage errors and their tips,
colour, completion scripts and requests, the spec, `--json`, `check`, `clean`, `schema`,
`diagnostics` and `gen-docs`.

### Fork parity and the runtime suites

`tests/forkparity.rs` ports `internal/forkparity` and the Lune half of `internal/conformance`. The
tests that run sloptor over the corpus or build places are ignored by default.
`mise run rust:forkparity` builds the release binary, names it in `ROTOR_BIN`, and runs them all;
it takes about 20 seconds after the build, once the first run has installed the pinned roblox-ts into the user cache. Without `ROTOR_BIN` the suite runs the test profile's
`sloptor`. The runtime suites need Node and npm, and Rojo and Lune from `mise.toml`. Without Rojo
or Lune a runtime test prints `SKIPPED <test>: <why>` and passes (`ROTOR_ROJO_PATH` and
`ROTOR_LUNE_PATH` name the tools when they are not on `PATH`); `full_fork_parity` fails instead,
because a skipped run verifies nothing.

| Go test | Rust test |
| --- | --- |
| `forkparity` `TestFullForkParity`, `TestProjectOracleGoldens` | `matrix::full_fork_parity`: the 10 transformer and 10 project fixtures, with zero drift. An `upstream-corrected` row's byte differences are accepted only after the runtime test it names passes |
| `forkparity` `TestArchiveIdentity`, `TestTransformerFixtureProvenance`, `TestProjectFixtureManifest` | `matrix::archive_digest_and_provenance_match` |
| `forkparity` `TestDivergenceLedgerCoversMatrixCases`, `TestDivergenceLedgerRejectsUnknownMatrixCase` | `matrix::ledger_covers_every_matrix_case`, `matrix::ledger_rejects_an_unknown_case_and_a_go_behavioral_test`. A corrected row must name a test of `runtime` |
| `forkparity` `TestBehavioralVerificationRejectsUnexecutedTests` | none: `full_fork_parity` runs the named test in-process and fails when it cannot run, so there is no skipped run to reject |
| `forkparity` `TestProjectFixtures*`, `TestPerformanceOutputFixture` | `matrix::project_fixtures_cover_the_fork_scenarios`, `matrix::performance_output_fixture_is_intact` |
| `conformance` `TestBehavioralSuite` | `runtime::behavioral_suite` |
| `conformance` `TestPinnedCompatibility/*` | `runtime::pinned_compatibility_{loops,calling_conventions,iterator_rest,switch}` |
| `conformance` `TestPinnedPackageInterop`, `TestPinnedCompatibilityDiagnostics` | `runtime::pinned_package_interop`, `runtime::pinned_compatibility_diagnostics` |
| `conformance` `TestRandomnessAcceptance` | `runtime::randomness_acceptance` (skips without `ROTOR_RANDOMNESS_PATH`) |
| `conformance` `TestDiagnosticsCorpus`, `TestNoRobloxSymbolInstanceofFixture`, `TestRojoTopologyDiagnosticFixtures` | `runtime::diagnostics_corpus` (not ignored; it compiles in-process) |
| `conformance` `TestFixtureManifestCoverage` | `rotor_compile`'s `tests/conformance.rs` `fixture_manifest_covers_every_golden` |

Not ported: the tests that run the fork itself (`TestRunnerCapturesArtifacts`,
`TestTransformerOracleGoldens`), because `roblox-ts.zip` ships without the fork's `node_modules`
and Go skipped them too. The comparator and normalizer behind them (`TestNormalizerBoundaries`,
`TestComparator*`) went with them. Also not ported: the zip extraction guards
(`TestArchiveRejects*`), because sloptor's suite checks the archive's digest and never extracts
it, and the report digests (`TestMatrixArtifact*`), because the Rust suite reports drift
directly and writes no report file.
