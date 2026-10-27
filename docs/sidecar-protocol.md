# Transformer Sidecar Protocol

Rotor runs transformer plugins (`compilerOptions.plugins`) through a Node
worker that mirrors upstream roblox-ts plugin semantics. The worker's source
of truth is `tools/sidecar/`, and it is **embedded in the rotor binary**
(`tools/sidecar/embed.go` + `internal/compile/sidecar_install.go`): released
binaries extract it to `<user-cache>/rotor/sidecar-<content-hash>/` on first
plugin build, so no repo checkout is required. `ROTOR_SIDECAR_PATH` overrides
the worker location (repo development and tests point it at `tools/sidecar`).

Projects without plugins never spawn Node. **Declaration emit never goes
through the worker**: rotor emits `.d.ts` natively from tsgo, including the
`paths` rewrite and the `@rbxts/types` reference rescope (see "Declaration
emit is native" below).

## TypeScript resolution

The worker resolves the `typescript` package **from the project's
`node_modules` first**, falling back to the worker's own directory. This
matters for correctness, not just convenience: plugins `require("typescript")`
themselves, and factory nodes only compose with `transformNodes` when both
sides share one module instance — upstream roblox-ts guarantees this by
construction (its own pinned 5.5.3 is the hoisted copy plugins see).
roblox-ts projects pin `typescript@~5.5.3`; if a project has no typescript
install, the worker reports a `typescript-not-found` diagnostic.

## Setup (repo development)

```bash
cd tools/sidecar
bun install --no-save
```

The package pins `typescript@5.5.3` to match the upstream `roblox-ts` plugin
runtime; this copy is only the fallback for synthetic test fixtures that have
no `node_modules` of their own.

## Invocation

```bash
node tools/sidecar/main.js
```

The process reads newline-delimited JSON requests from `stdin` and writes one
newline-delimited JSON response per request to `stdout`.

Builds started through the JavaScript API use this stdio worker too, sharded
and with native jecs, exactly like CLI builds. A client created with
`createBuildSession({ transformerCallbacks: true })` instead asks for callbacks
when it initializes. The native API server then sends each `transform`
request back over the build connection to the client's in-process
`SidecarServer`, which serves them one at a time on its own thread. Both
transports meet at one native transformer-host seam, so plugin resolution,
ordering, options, overlay handling, and diagnostic trace maps are shared
rather than reimplemented.

**stdout is reserved for protocol responses.** `main.js` captures the real
stdout writer and reroutes every other stdout write (plugin `console.log`,
e.g. Flamework's logging) to stderr. Rotor streams the worker's stderr lines
to the compiler log as they arrive and keeps a tail for error reporting.

## Warm sessions

Rotor keeps **one worker per `(projectDir, tsConfigPath)` for the life of the
rotor process**, including across `rotor build -w` rebuilds — the JS program
stays warm, mirroring upstream's persistent `transformerWatcher`. The worker
exits when rotor's pipes close.

Edits are communicated via `changedFiles`: rotor stat-diffs the project's
`.ts`/`.tsx` files against the session's last-seen stamps and ships new text
for anything that changed, which bumps the worker's LanguageService script
versions (upstream `updateFile` semantics). A request on a fresh worker sends
no changed files — the worker reads from disk. If a worker dies mid-request,
rotor respawns it once and retries.

**Caller source overlays** (`ProjectOptions.Overlays`, which `rotor
diagnostics` fills from its stdin request) ride the same field, under
different rules. An overlay exists nowhere on disk, so no stat describes it:
each one ships on every round trip, fresh worker or not. An overlay the next
round trip drops is undone by resending the file's disk text, because the
worker's override map outlives the request that filled it.

Known limitation: a warm worker's *plugin-visible* view of an edited ambient
`.d.ts` can be stale until the watch session restarts (stamps cover the
`.ts`/`.tsx` compile surface). Rotor's own typecheck and emit always read
fresh state.

Inside the worker, one in-memory project session per
`(projectDir, tsConfigPath)` holds the overlay map and reuses the TypeScript
`LanguageService` program across requests. A stdio worker naturally owns one
such session. The API client maps canonical project-and-config keys to sessions
so concurrent solution projects cannot replace one another's TypeScript state;
disposing the API session disposes the complete map.

## Protocol v1

Each request must be a single JSON object with `protocol: 1`.

```json
{
  "protocol": 1,
  "tsConfigPath": "C:/abs/project/tsconfig.json",
  "projectDir": "C:/abs/project",
  "compileFileNames": [
    "C:/abs/project/src/example.ts"
  ],
  "rootFileNames": [
    "C:/abs/project/src/example.ts",
    "C:/abs/project/src/globals.d.ts"
  ],
  "changedFiles": [
    {
      "fileName": "C:/abs/project/src/example.ts",
      "text": "export const phase = \"memory\";\n"
    }
  ],
  "transformSources": true
}
```

`rootFileNames` is optional and narrows the worker's `LanguageService` root
set for one request. Rotor sends it on an incremental round trip, naming the
files being compiled plus every `.d.ts` in the project; TypeScript still pulls
each root's import closure into the program, so a transformer asking the
checker about a compiled file gets the same answers while the worker parses
and binds far fewer files. Omitting it means "every file the tsconfig names",
which is what a full build sends.

Within a session the root limit only ever widens, and the first request that
omits it drops the limit for good. A watch session rebuilds through the same
warm worker, so a limit that shrank between rebuilds would throw the program
away each time.

`transformSources` is required and must be a boolean. There is no declaration
mode: the worker only ever transforms sources.

Response shape:

```json
{
  "diagnostics": [
    {
      "category": "error",
      "code": "invalid-request",
      "message": "protocol must equal 1"
    }
  ],
  "transformed": [
    {
      "fileName": "C:/abs/project/src/example.ts",
      "text": "export const phase = \"before:after:start\";\n"
    }
  ],
  "afterDeclarationsTransformers": 0
}
```

`afterDeclarationsTransformers` is how many `afterDeclarations` transformers
the worker built for the project after flattening. The worker never runs them;
the count exists so rotor can warn (see "Declaration Emit Is Native").

Responses may include `metrics` with request wall/CPU time, Node version,
per-plugin factory/visit time, and an optional `phases` object. Its integer
millisecond fields are `configMs` (config and root synchronization), `programMs`
(LanguageService program), `factoriesMs` (plugin loading and factory creation),
`transformMs` (`transformNodes`), `parentRepairMs`, and `printMs` (final printing
and trace-map generation/serialization). Validation requests measure plugin
loading under `factoriesMs` without invoking factories. These are per-request
values, including completed work on failures. They overlap: `transformMs`
includes parent repair, and plugin times overlap factory and transform phases;
they must not be summed. Other request work and protocol response encoding are
outside these phases. Existing `--verbose` output displays the phase details;
`--timings` continues to report Node wall/CPU totals without these phase fields.

`diagnostics` may contain:

- TypeScript config/program diagnostics converted to `{ category, code, file, start, length, message }`
- transformer resolution warnings using code `transformer-not-found`
- a `typescript-not-found` error when the project has no resolvable `typescript` package
- request validation errors using code `invalid-request`
- internal worker failures using code `sidecar-internal`

## Semantics Mirrored From Upstream

The worker mirrors the upstream `roblox-ts` transformer behavior in these areas:

- `getPluginConfigs` only accepts plugin objects with string `transform` fields.
- transformer modules resolve relative to `projectDir`.
- factory invocation follows upstream `type` handling for `program`, `config`, `checker`, `raw`, and `compilerOptions`.
- transformed files run through a single `typescript.transformNodes(...)` pass.
- transformer flatten order intentionally stays `after`, then `before`.
- transformed `SourceFile`s are reprinted with `typescript.createPrinter().printFile(...)`.

## Declaration Emit Is Native

rotor emits every `.d.ts` from tsgo, whether or not the project runs
transformer plugins. Upstream `roblox-ts` emits declarations from the JS
`program.emit`, with two `afterDeclarations` transformers of its own: a
`types` triple-slash rescope and a `paths` -> relative-specifier rewrite (the
Luau runtime resolves neither `baseUrl` nor `paths`). Both are ported to Go in
`internal/compile/declpaths.go` and `internal/compile/declmap.go`, applied to
the emitted text and driven by a reparse of it — never by a regex.

Two consequences:

- **`afterDeclarations` transformers do not run.** tsgo has no
  custom-transformer hook, so there is nowhere to run them. A project that
  declares one gets a one-line warning per build (`afterDeclarations
  transformers are not supported; declarations are emitted natively`), never a
  silent drop. Both halves of the detection are in play: rotor reads the
  `"afterDeclarations": true` flag off the tsconfig plugin entry, and the
  worker reports `afterDeclarationsTransformers` for a factory that returns
  the shape without the flag.
- **`import("alias/...")` type specifiers are now rewritten.** The JS
  transformer resolved specifiers through a module-resolution cache seeded
  with what the program had already resolved for the file, which is not where
  the declaration emitter's synthesized import types for INFERRED types come
  from — those kept their alias spelling and shipped unresolvable. The native
  rewriter falls back to a real resolver pass for any specifier the program
  did not resolve, so they are rewritten too. This is a deliberate divergence
  from rbxtsc, listed on the macros and extensions page of the documentation site.

The `.d.ts.map` is generated by tsgo from the text it printed, before the
specifier splice, so `internal/compile/declmap.go` shifts the generated
columns of every mapping that sits past a rewritten specifier on the same
line. Only the generated-column field is touched; names, sources, and source
positions are copied through byte for byte.

## Deliberate Divergence: `plugins` And `extends`

Upstream `roblox-ts` walks the `extends` chain itself and **concatenates** the
`compilerOptions.plugins` of every config in it (`Project/transformers/
getPluginConfigs.ts`). A child that sets `"plugins": []`, or that lists a
different transform, still runs everything its parents declared — there is no
way to opt out of an inherited transformer.

Rotor takes the **resolved** value instead: `getPluginConfigs` reads
`plugins` off the compiler options the config parse already produced, so the
rule is TypeScript's own. `plugins` is an array-valued compiler option, and
`extends` replaces array-valued options rather than merging them:

| child `compilerOptions.plugins` | resolved list | rbxtsc |
| --- | --- | --- |
| absent | the parent's | the parent's |
| `[]` | empty | the parent's |
| `[{ "transform": "x" }]` | `x` only | `x` + the parent's |

`tsc --showConfig` on the child reports exactly the middle column; run it to
confirm what a project resolves to. Note that this rule is what makes a
test-only config (`tsconfig.spec.json` extending a package's `tsconfig.json`)
able to compile without its package's transform.

Two consequences for projects migrating from rbxtsc:

- A config that declares its own `plugins` no longer inherits a shared base's
  transformers. List them alongside its own if it needs both.
- `internal/compile` gates the Node worker on the same rule, so a project that
  resolves to no plugins never spawns one.

## Verification

JS worker suite (also run in CI):

```bash
cd tools/sidecar
node --test test/*.test.js
```

Real-package integration (Flamework + rbxts-transform-env), exercising the
full production path — embedded extraction, project typescript, warm session:

```bash
cd testdata/transformers/project && bun install --no-save && cd ../../..
go test ./internal/compile -run TestTransformersFixtureFlameworkAndEnv -count=1
```

The JS suite covers plugin discovery through `extends`, named/default factory
loading, `checker`/`compilerOptions` factory instantiation, the
`after -> before` execution quirk, the `afterDeclarations` count the worker
reports instead of running, stdio protocol handling with warm overlay updates,
per-project typescript resolution, and the stdout-protection rule.

## Experimental project-attested two-worker builds

Serial execution is the default. A project may opt into exactly two workers
with `parallelTransformers` in `rotor.toml`. The owner asserts that the entire
ordered stage, including module initialization, factories, per-file state and
side effects, is deterministic and independent of batch boundaries. This is
compatibility assurance for reviewed project code, not authentication or a
general guarantee about transformer plugins.

```toml
[parallelTransformers]
workers = 2
batchIndependent = true
typescript = "<reviewed loaded ts.version>"

[[parallelTransformers.plugins]]
config = '''{"transform":"my-transformer","silent":true}'''
sha256 = "<owner-reviewed package digest>"

[[parallelTransformers.dependencies]]
from = 0
specifier = "my-patched-dependency"
sha256 = "<owner-reviewed dependency digest>"
```

Each `plugins` entry holds the complete plugin JSON options object (including
`transform`), in stage order. Object key order is immaterial; values, array
order and omitted properties must match. The owner lists all patched or
locally built dependencies on the plugins' load paths. `from` is the zero-based
plugin index from whose entry directory Node resolves that dependency. The
host cannot infer whether this owner-maintained list is complete.

The host checks these facts before invoking any transformer factory:

- Provenance comes from the actual one-shot CLI `build` path, including
  `build --build --project`. Its local solution coordinator drains once,
  deduplicates canonical configs and builds each scheduled project once.
  API, watch, dev, check, diagnostics and resident/unknown callers stay serial.
  A shared nonblocking token permits one outstanding split per CLI invocation;
  busy projects run serially. Project profiling labels/tasks remain intact.
- The slot has never been used, the pipeline's initial external prefix is its
  entire external stage, and there is no later external suffix. Native
  Flamework is the only following transform. Both workers close after this call.
- The prepared request has no narrowed `rootFileNames`. Overlays are authored
  compile sources only; config, declaration and package overlays stay serial.
- Every compile source is inside `projectDir`, with no intervening
  `package.json` or `node_modules` scope. Unique ancestor directories are
  checked without traversing dependency stores. Directory aliases and symlinks
  in those directories conservatively select serial. The root's common
  `node_modules` entry is allowed.
- Both normal `main.js` sessions check the same ordered plugin options,
  resolved package content identities, listed dependency identities and loaded
  TypeScript version. Normal plugin resolution, export validation and the
  existing same-TypeScript-instance check are retained at their ordinary point
  after Program creation. Preflight only resolves paths and reads metadata/code
  bytes; it never evaluates a plugin or its TypeScript peer, creates no Program
  and calls no factory. The loaded engine version binds TypeScript identity;
  a forwarding wrapper's package version is not the engine version.

Missing, mismatched or unsupported attestations fall back before transform
dispatch, using the ordinary full request and first session. Config decoding
keeps the existing TOML errors/warnings; an unknown attestation key disables
admission. A failed or timed-out transport after dispatch is an error with no
serial replay: repeating plugin side effects would be unsafe. The existing
serial transport's retry policy is unchanged. A worker-0 preflight transport
failure consumes its existing first-attempt retry budget. A rejected/timed-out
auxiliary is reaped before ordinary serial execution on unchanged worker 0 with
a fresh request timeout. Parent cancellation remains an error.

Content identity uses SHA-256 with the domain `rotor-package-v1\0`. Resolve the
entry normally and take its physical installed package root after the last
`node_modules` segment, including a scope if present. For local code outside
`node_modules`, take the nearest ancestor with a named `package.json`, skipping
unnamed module-format markers. Missing/ambiguous package ownership selects serial.
Enumerate every regular file below that package root except nested
`node_modules`. Sort relative paths by JavaScript code-unit order with `/`
separators. For each file append UTF-8 `relativePath\0decimalByteLength\0`, then
the exact file bytes. Include code and `package.json`; no timestamps or absolute
paths enter the digest. Links/special files, over 8,192 entries, over 4,096 files,
or over 64 MiB select serial. `packageDigest(entry)` in `lib/parallel.js`
implements this format. Recording a digest is the owner's review decision;
Rotor neither supplies private expected values nor treats a live digest as
approval. A changed backend or dependency requires renewed chain parity;
recording a new digest alone does not qualify it. Registry dependency closure,
TypeScript engine/lib files, Node,
signatures and benchmark manifests are outside this contract.

Requests differ only in their contiguous `compileFileNames` ranges and a
private response-partition marker. Roots, overlays, config paths and plugin
options remain identical. Outputs concatenate in range order. Diagnostic
partitions retain serial phase order: the common factory/config prefix once,
then all missing-source diagnostics, then transform diagnostics in range order,
without sorting, normalization or deduplication.
The first serial-shaped catch (one error `sidecar-internal` diagnostic and no
outputs) discards all outputs and excludes stderr from later ranges. After both
workers reap, stderr uses the existing serial line forwarding in range order,
including its CRLF handling, line-length cap and unterminated-line behavior.
Abnormal cleanup or incomplete drainage, including `exec.ErrWaitDelay`, cannot
turn an otherwise successful split green. An earlier ordered internal failure
keeps the serial failure shape. The split token is released after cleanup.

Metrics describe the coordinator's transform elapsed wall time, summed worker
CPU and summed plugin work. Overlapping worker phase metrics are omitted;
they are not summed as elapsed wall time. Transport bytes include preflight,
and spawn telemetry includes the second worker, including a rejected preflight.
Verbose output emits `transformer parallel batch completed (workers=2)` only
after successful split responses, merging and cleanup; spawns alone do not
prove admission.
An owner's attestation still carries TypeScript's lazy-checker-order residual
and requires parity validation of the chosen chain. Process initialization
warnings may appear once per worker. The owner must disable external timing
and other batch-sensitive side effects for attested runs. Attestation does not
universally guarantee plugin semantics. No full-build performance
or general plugin-safety claim follows from enabling this experiment.
