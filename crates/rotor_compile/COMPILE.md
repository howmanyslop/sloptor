# rotor_compile: the port of `internal/compile`

Stage 5a ports the single-project pipeline: `CompileFile`, `CompileProject` and
`BuildProjectWithOptions` with declarations, the incremental manifest, the copy-files gate and
output writing. Stage 5b ports the solution builder, the transformer-plugin sidecar, the census
and watch mode. Stage 6 ports native Flamework and Jecs (`internal/flamework`, `internal/jecs`)
and the glue that calls them.

A file split between stages keeps one Rust module per Go file. The 5a part is ported, and each
5b or Stage 6 entry point keeps Go's signature and returns an explicit "not yet ported" error
(`sidecar::not_yet_ported`). The stubs return errors rather than calling `todo!`,
`unimplemented!` or `panic!`. With Flamework and transformer plugins off, every stubbed path
takes the branch Go takes.

Rows whose Rust module reads "(none yet)" have no Rust file until their stage ports them.

## File map (61 Go files)

| Go file | Stage | Rust module | Notes |
| --- | --- | --- | --- |
| `afterdeclarations.go` | 5a | `afterdeclarations` | |
| `census.go` | 5b | `census` | Stub `CensusCollector`; the project pipeline's census branches are kept |
| `census_solution.go` | 5b | (none yet) | |
| `compile.go` | 5a | `compile` | |
| `compiler_options.go` | 5a | `compiler_options` | The loader in `rotor_tsrs::project` applies the same overrides |
| `copyfilesgate.go` | 5a | `copyfilesgate` | |
| `crossproject.go` | 5a (part) | `crossproject` | `addProjectRealPathAliases` only; the solution's cross-project map is 5b |
| `declaration_prefetch.go` | 5a | `declaration_prefetch` | |
| `declmap.go` | 5a | `declmap` | |
| `declpaths.go` | 5a | `declpaths` | |
| `envdecl.go` | 5a | `envdecl` | The synthetic file is injected by `rotor_tsrs::project` |
| `filemacro.go` | 5a | `filemacro` | `$file` returns raw bytes |
| `flamework_config.go` | 5a | `flamework_config` | Moved to 5a: the pipeline needs its Flamework-off detection |
| `flamework_pipeline.go` | 5a (part) | `flamework_pipeline` | Non-Flamework route; Flamework on is "not yet ported" |
| `flamework_program_update.go` | 6 | (none yet) | |
| `incremental_flamework.go` | 6 | `incremental_flamework` | Stub: no Flamework salt input |
| `incremental_manifest.go` | 5a | `incremental_manifest` | |
| `incremental_paths.go` | 5a | `incremental_paths` | |
| `incremental_plugin_salt.go` | 5a | `incremental_plugin_salt` | |
| `incremental_salt.go` | 5a | `incremental_salt` | |
| `incremental_signature.go` | 5a | `incremental_signature` | |
| `jecs_native.go`, `jecs_rebase.go` | 6 | (none yet) | |
| `luau_types.go` | 5a | `luau_types` | |
| `macro_inputs.go` | 5a | `macro_inputs` | |
| `macrodecl.go` | 5a | `macrodecl` | |
| `native_types.go` | 5a | `native_types` | |
| `output.go` | 5a | `output` | |
| `outputwriter.go` | 5a | `outputwriter` | `os.Root` replaced by canonical-path confinement |
| `outputwriter_manifest.go` | 5a | `outputwriter_manifest` | |
| `overlayfs.go` | 5a | `overlayfs` | Overlays go through `rotor_tsrs::project::update_with_overlays` |
| `package_layout.go` | 5a | `package_layout` | |
| `package_layout_watch.go` | 5b | (none yet) | Watch mode |
| `packageexports.go` | 5a | `packageexports` | |
| `packageexports_parse.go` | 5a | `packageexports_parse` | Condition order comes from the `gojson` ordered parse |
| `parallelwrite.go` | 5a | `parallelwrite` | |
| `project.go` | 5a | `project` | Program construction is in `rotor_tsrs::project` |
| `project_config.go` | 5a | `project_config` | |
| `project_references.go` | 5b | (none yet) | Single-project reference path maps are in `project` |
| `rbxts_options.go` | 5a | `rbxts_options` | |
| `rbxts_schema.go` | 5a | `rbxts_schema` | |
| `reference_validation.go` | 5b | (none yet) | |
| `rotortypes.go` | 5a | `rotortypes` | |
| `sanitize.go` | 5a | `sanitize` | Re-exports `rotor_tsrs::tsconfig` |
| `sidecar.go` | 5b (part) | `sidecar` | 5a has the helpers the pipeline calls (plugin detection, overlay paths, `realpath`); plugins on is "not yet ported" |
| `sidecar_install.go`, `sidecar_parallel.go`, `sidecar_roots.go`, `sidecar_shards.go` | 5b | (none yet) | |
| `solution.go` | 5b (part) | `solution` | `canonicalExplicitPath` and `pathHasExplicitSymlink` only |
| `solution_cache.go`, `solution_drain.go`, `solution_satisfied.go`, `solution_scheduler.go`, `solution_watch.go` | 5b | (none yet) | |
| `source_trace.go` | 5a | `source_trace` | |
| `source_trace_sourcemap.go` | 5a | `source_trace_sourcemap` | |
| `timings.go` | 5a | `timings` | Go runtime metadata fields are left out |
| `transformer_host.go`, `transformer_host_sidecar.go` | 5b | (none yet) | |
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

## Go tests

The Go tests are ported in `src/tests/` and in `src/source_trace.rs`'s test module. Each Go test
file maps to one Rust module, and the expected strings are copied verbatim.
`logservice_test.go` is ported into `rotor_support::logservice`. `tests/conformance.rs` is the
conformance golden gate, and `tests/filemacro_bytes.rs` covers `$file` on Windows-1252 bytes.

Ported: `array_append_optimization`, `baseurl`, `comment_directives`, `compile`,
`copyfilesgate`, `declmap`, `diagnostic_string_code`, `env_macro`, `exportfrom`,
`getmoduletree_folder`, `imports`, `incremental`, `jsx_build`, `labels`, `luau_types`,
`luau_types_emit`, `macro`, `macro_inputs`, `native_types`, `output`, `output_guard`,
`outputwriter`, `overlayfs` (through the overlay route rotor_compile takes), `parallelwrite`,
`pnpm`, `project`, `rbxts_options`, `rotortypes`, `skip_identical_writes`, `source_trace`,
`sourcemap_acceptance`, `sourcemap_columns`, `sourcemap_fallback`, `sourcemap_lines`,
`sourcemap_paths`, `unreachable_function`, `validateoptions`, `vector_math`.

`src/tests/needs_later_stages.rs` holds the Go tests whose subject is 5b or Stage 6 as ignored tests:
`task7_*`, `sourcemap_plugin_acceptance`, `sourcemap_performance_acceptance` and
`react_syntax_origin`. Other ignored tests name their reason in `#[ignore]`:

- Tests that swap Go's `transformer.TransformStatement` variable.
- Tests that set process environment variables. That is `unsafe` in Rust 2024, which the
  workspace denies.
- Tests that inject `outputWriterOperations` or count calls on `newOverlayFS`.
- Tests that need string-code diagnostics, which tsrs does not have.
- Halves of tests that build a solution (`BuildSolutionWithOptions`, 5b).

Not ported yet, though their subject is 5a: `audit`, `declaration_emit`, `declpaths`,
`incremental_plugin_salt`, `incremental_signature`, `incremental_signature_build`,
`package_layout`, `packageexports`, `packageexports_compile`, `project_context_selection` and
`timings`. The 5b and Stage 6 files (`census*`, `flamework_*`, `jecs_*`, `performance_parity*`,
`project_reference_import`, `sidecar*`, `solution*`, `transformer*`) move with their stage.
