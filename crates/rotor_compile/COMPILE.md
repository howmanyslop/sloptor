# rotor_compile: the port of `internal/compile`

Stage 5a ports the single-project pipeline: `CompileFile`, `CompileProject` and
`BuildProjectWithOptions` with declarations, the incremental manifest, the copy-files gate and
output writing. Stage 5b ports the solution builder, the transformer-plugin sidecar, the census
and watch mode. Stage 6 ports native Flamework and Jecs (`internal/flamework`, `internal/jecs`)
and the glue that calls them.

A file split between stages keeps one Rust module per Go file. Stage 6 ports `internal/jecs`
into `rotor_jecs` (`forof`, `query`, `transform`) and the compile-side glue listed below.

## File map (61 Go files)

| Go file | Stage | Rust module | Notes |
| --- | --- | --- | --- |
| `afterdeclarations.go` | 5a | `afterdeclarations` | |
| `census.go` | 5b | `census` | |
| `census_solution.go` | 5b | `census_solution` | |
| `compile.go` | 5a | `compile` | |
| `compiler_options.go` | 5a | `compiler_options` | The loader in `rotor_tsrs::project` applies the same overrides |
| `copyfilesgate.go` | 5a | `copyfilesgate` | |
| `crossproject.go` | 5a, 5b | `crossproject` | |
| `declaration_prefetch.go` | 5a | `declaration_prefetch` | |
| `declmap.go` | 5a | `declmap` | |
| `declpaths.go` | 5a | `declpaths` | |
| `envdecl.go` | 5a | `envdecl` | The synthetic file is injected by `rotor_tsrs::project` |
| `filemacro.go` | 5a | `filemacro` | `$file` returns raw bytes |
| `flamework_config.go` | 5a | `flamework_config` | Moved to 5a: the pipeline needs its Flamework-off detection |
| `flamework_pipeline.go` | 5a, 6 | `flamework_pipeline` | |
| `flamework_program_update.go` | 6 | `flamework_program_update` | Multi-file updates use tsrs patch 0007 (`Program::update_program_files_with_checkers`) |
| `incremental_flamework.go` | 6 | `incremental_flamework` | |
| `incremental_manifest.go` | 5a | `incremental_manifest` | |
| `incremental_paths.go` | 5a | `incremental_paths` | |
| `incremental_plugin_salt.go` | 5a | `incremental_plugin_salt` | |
| `incremental_salt.go` | 5a | `incremental_salt` | |
| `incremental_signature.go` | 5a | `incremental_signature` | |
| `jecs_native.go`, `jecs_rebase.go` | 6 | `jecs_native`, `jecs_rebase` | |
| `luau_types.go` | 5a | `luau_types` | |
| `macro_inputs.go` | 5a | `macro_inputs` | |
| `macrodecl.go` | 5a | `macrodecl` | |
| `native_types.go` | 5a | `native_types` | |
| `output.go` | 5a | `output` | |
| `outputwriter.go` | 5a | `outputwriter` | `os.Root` replaced by canonical-path confinement |
| `outputwriter_manifest.go` | 5a | `outputwriter_manifest` | |
| `overlayfs.go` | 5a | `overlayfs` | Overlays go through `rotor_tsrs::project::update_with_overlays` |
| `package_layout.go` | 5a | `package_layout` | |
| `package_layout_watch.go` | 5b | `package_layout_watch` | |
| `packageexports.go` | 5a | `packageexports` | |
| `packageexports_parse.go` | 5a | `packageexports_parse` | Condition order comes from the `gojson` ordered parse |
| `parallelwrite.go` | 5a | `parallelwrite` | |
| `project.go` | 5a | `project` | Program construction is in `rotor_tsrs::project` |
| `project_config.go` | 5a | `project_config` | |
| `project_references.go` | 5b | `project_references` | Single-project reference path maps are in `project` |
| `rbxts_options.go` | 5a | `rbxts_options` | |
| `rbxts_schema.go` | 5a | `rbxts_schema` | |
| `reference_validation.go` | 5b | `reference_validation` | |
| `rotortypes.go` | 5a | `rotortypes` | |
| `sanitize.go` | 5a | `sanitize` | Re-exports `rotor_tsrs::tsconfig` |
| `sidecar.go` | 5b | `sidecar` | The embedded worker is `include_bytes!` of `tools/sidecar` (`sidecar_install`) |
| `sidecar_install.go`, `sidecar_parallel.go`, `sidecar_roots.go`, `sidecar_shards.go` | 5b | `sidecar_install`, `sidecar_parallel`, `sidecar_roots`, `sidecar_shards` | |
| `solution.go` | 5b | `solution` | |
| `solution_cache.go`, `solution_drain.go`, `solution_satisfied.go`, `solution_scheduler.go`, `solution_watch.go` | 5b | `solution_cache`, `solution_drain`, `solution_satisfied`, `solution_scheduler`, `solution_watch` | `solution_watch` is the coordinator's half; the watch loop is Stage 7 |
| `source_trace.go` | 5a | `source_trace` | |
| `source_trace_sourcemap.go` | 5a | `source_trace_sourcemap` | |
| `timings.go` | 5a | `timings` | Go runtime metadata fields are left out |
| `transformer_host.go`, `transformer_host_sidecar.go` | 5b | `transformer_host`, `transformer_host_sidecar` | |
| `transformer_order.go` | 5a | `transformer_order` | |

Outside the Go file map:

- `gojson`: Go-compatible JSON helpers. The parse keeps key order and duplicates, strings are
  escaped the way `encoding/json` escapes them (HTML escaping included), and indentation
  matches `MarshalIndent`.
- `internal/logservice` is ported as `rotor_support::logservice`.
- `rotor_tsrs::project::load_project_in` takes the project directory separately from the
  tsconfig's directory, as `newProjectProgramFromFSWithOptions` does under `--project`.

## Deviations from Go

- The incremental salt is a hash over the same inputs as Go's (`incremental_salt.rs`), with the
  path options made project-relative as `portableIncrementalOptions` does. The compiler options
  are encoded by tsrs's Go-JSON writer, so the salt is not byte-identical to Go's, and a Go-built
  manifest never matches a rotor-rs build.
- The copy-files manifest (`copyfilesgate.rs`, `compiler_version`) and the Rojo resolver cache
  key (`output.rs`, `load_rojo_caches_pre_build`) record `rotor-rs <version>`, where Go records
  tsgo's `core.Version()`. The TypeScript versions differ, so rotor-rs keeps its caches apart
  from Go's on purpose. The first rotor-rs build after a Go build in the same checkout therefore
  copies non-compiled files, resolves Rojo data and compiles every file in full.
- `OutputWriter` confines writes without `os.Root`. Each output's parent directory is resolved
  once and cached (`dir_roots`, Go's `dirRoot`), and only a final symlink is resolved per write.
- tsrs diagnostics have no string codes, so every diagnostic code is `TS####`.
- Stage 5b:
  - The solution's shared parse cache goes through a `SourceFileCache` hook on a caching
    `CompilerHost` in `rotor_tsrs` (`load_project_in_cached`); Go's shared `cachedvfs` stat
    cache is not ported.
  - Cancellation is a flag (`project::Cancellation`) checked where Go checks `ctx.Err()`; there
    is no deadline context. Tests build Go's countdown context with `Cancellation::countdown`.
  - The API transformer callback rides on `ProjectOptions::transformer_callback` instead of the
    context (`transformer_host::with_transformer_callback`). It gets
    `Cancellation::with_timeout`, Go's `context.WithTimeout` (`Cancellation::err` gives
    `context deadline exceeded`).
  - The CLI has no admission or runtime policy yet (Stage 7), so the build share stays 1.
  - Go's owned-stderr spawn mode is not distinguished (`sidecar::spawn_sidecar_session_mode`);
    a worker's stderr is always a pipe whose tail outlives `close`.
  - The embedded worker's file list is written out (`sidecar_install::EMBEDDED_SIDECAR`) where
    Go embeds `lib/*.js`; a test fails when `tools/sidecar` gains or changes a file.
  - When `update_with_overlays` fails, the error is returned; Go's
    `newProjectProgramWithOverlay` fallback is not ported.
  - tsrs leaves out an empty `sourceRoot` in declaration maps where tsgo writes
    `"sourceRoot":""`; `declmap::restore_empty_source_root` puts it back.
  - The persistence queue lives on the coordinator (`SolutionCoordinator::persist_queue`),
    used when the drainer opts in (`SolutionProjectDrainer::queues_persistence`), where Go's
    build drainer owns it. A reload empties it with the build drainer it replaces, as Go's fresh
    drainer starts empty. Persistence steps are `FnMut`, so a failed step is retried.
  - A build frees its programs when it ends (`output::build_project` opens a
    `rotor_tsrs::project::ProgramScope`, and tracked programs get tsrs's checker regions), as
    Go drops them. Parsed files stay in tsrs's thread arena, so the solution parse cache can
    share them; Go's collector frees those too. A census, `compile_file` and `compile_project`
    keep their programs.
  - The sidecar shard count's CPU count is `std::thread::available_parallelism`, which honours
    cgroup CPU quotas where Go's `runtime.NumCPU` does not. Only cgroup-limited Linux can see a
    smaller default pool than Go.
  - Tests override the environment the sidecar reads through `sidecar::set_test_env` (Go's
    `t.Setenv`); the process environment is never touched.
- Stage 6:
  - `rotor_jecs::transform` (`package_symbol`, transform.rs:491): the fallback walk over
    resolved modules visits them sorted by path, then by name and mode, where Go ranges over a
    map.
  - `sidecar::merge_sidecar_overlays` (sidecar.rs:1652) indexes the sidecar's overlays by
    normalized path; Go rescans them per file (`overlayByPath`). Same result, linear time.
  - `rotor_flamework` caches each directory's `package.json` and `flamework.build` facts for
    one transform (`package_declaration_facts`, transform_symbols.rs:167) and indexes plans by
    path suffix (`plan_index`, transform_state.rs:174); Go rereads and rescans per lookup.
  - `parallel_overlay_aliases` (flamework_program_update.rs:455) and `overlay_aliases` take
    overlays in path order; Go ranges over its map, so a colliding alias's text is
    nondeterministic there.
  - `SourceTraceMap::new` decodes Go's `rawSourceMap` with `gojson` and reproduces
    `encoding/json`'s errors for that struct (`decode_raw_source_map`, source_trace.rs:288).
  - A malformed `rotor.toml` reports `rotor_config`'s TOML error text, not BurntSushi's
    (tests/task7_acceptance.rs:178).
  - The salt keeps Stage 5a's deviation: Flamework inputs feed the same hash, so a Go-built
    manifest still never matches.
  - Test hooks: `PROGRAM_UPDATES` and `LAST_PREPARSE` (flamework_program_update.rs:322) stand
    in for Go's `CreateCheckerPool` counting hook and its `program.Host().(nativeOverlayHost)`
    assertion; `solution_cache::wrap` is Go's `cache.wrap` over any host.

## Go tests

The Go tests are ported in `src/tests/` and in `src/source_trace.rs`'s test module. Each Go test
file maps to one Rust module, and the expected strings are copied verbatim.
`logservice_test.go` is ported into `rotor_support::logservice`. `tests/conformance.rs` is the
conformance golden gate, and `tests/filemacro_bytes.rs` covers `$file` on Windows-1252 bytes.

Ported: `array_append_optimization`, `audit`, `baseurl`, `comment_directives`, `compile`,
`copyfilesgate`, `declaration_emit`, `declmap`, `declpaths`, `diagnostic_string_code`,
`env_macro`, `exportfrom`, `getmoduletree_folder`, `imports`, `incremental`,
`incremental_plugin_salt`, `incremental_signature`, `incremental_signature_build`, `jsx_build`,
`labels`, `luau_types`, `luau_types_emit`, `macro`, `macro_inputs`, `native_types`, `output`,
`output_guard`, `outputwriter`, `overlayfs` (through the overlay route rotor_compile takes),
`package_layout`, `packageexports`, `packageexports_compile`, `parallelwrite`, `pnpm`, `project`,
`project_context_selection`, `rbxts_options`, `rotortypes`, `skip_identical_writes`,
`source_trace`, `sourcemap_acceptance`, `sourcemap_columns`, `sourcemap_fallback`,
`sourcemap_lines`, `sourcemap_paths`, `timings`, `unreachable_function`, `validateoptions`,
`vector_math`.

Stage 6 ports `flamework_*`, `incremental_flamework*`, `jecs_native`, `solution_flamework_*`,
`task7_*`, and the Flamework halves of `sidecar`, `sidecar_acceptance` (source maps and React
syntax origin) and `transformers_integration`. The native fixture tests copy
`testdata/transformers/native/project` into a temporary directory, where Go builds it in place,
so concurrent test processes do not race on its `node_modules` link. Ignored tests name their
reason in `#[ignore]`:

- Tests that swap Go's `transformer.TransformStatement` variable.
- Tests that set process environment variables outside the sidecar's settings. That is
  `unsafe` in Rust 2024, which the workspace denies.
- Tests that inject `outputWriterOperations` or count calls on `newOverlayFS`.
- Tests that need string-code diagnostics, which tsrs does not have.
- Tests that swap Go's `declarationTextsForSelection` variable, and the guard against a
  selection with a nil emitter, which a Rust `DeclarationSignatureSelection` cannot express.

Stage 5b ports `census`, `census_solution`, `project_reference_import`, `sidecar*`,
`solution*` (all but the benchmark-only `solution_watch_bench`), `transformer_host` and
`transformers_integration`, and turns on the 5a tests that were waiting for it. Scripted
sidecar sessions (`sidecar_parallel`) are small Node workers instead of Go's in-memory pipes.

`performance_parity` (it runs the CLI binary) waits for Stage 7. `incremental_portability` is
not ported yet.
