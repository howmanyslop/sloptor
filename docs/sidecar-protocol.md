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

The package pins `typescript@6.0.3` for the worker's own compatibility suite.
This copy is only the fallback for synthetic test fixtures that have no
`node_modules` of their own; project builds still resolve their TypeScript
runtime from the project first.

## Invocation

```bash
node tools/sidecar/main.js
```

The process reads newline-delimited JSON requests from `stdin` and writes one
newline-delimited JSON response per request to `stdout`.

**stdout is reserved for protocol responses.** For each request, `main.js`
captures plugin writes to stdout and stderr and returns their lines in that
request's `logs` field. Rotor forwards those lines to the compiler log after
it decodes the response.

## Persistent sessions

Rotor keeps a private daemon per canonical workspace and a Node worker per
canonical project config and runtime identity. Separate `sloptor` invocations
therefore reuse the same TypeScript `LanguageService` program. The worker key
includes the sidecar protocol and contents, the Node executable contents, the
resolved TypeScript and transformer modules, the effective plugin config, and
a hash of the complete child environment. A changed runtime input gets a new
worker instead of reusing process state loaded by an older build.

Unix builds use a user-private socket; Windows builds use a named pipe limited
to the current user. `sloptor daemon status` lists live workspace daemons, and
`sloptor daemon stop` asks every live daemon for the current user to exit. A
daemon expires after five idle minutes and keeps at most two idle Node workers
per workspace.

The daemon owns the source snapshots used for freshness. Every transform sends
the complete current source file set and complete caller overlay set. The
daemon derives text updates and deletions from those snapshots, so an edit,
deleted disk file, or removed overlay is visible even when the compiler process
that created the worker has exited. Config file-set changes trigger a new parse;
edits to loaded TypeScript or plugin dependency files dispose and recreate the
Node session.

A build-only warm request starts while Go creates its initial program. It asks
the worker to create the JavaScript program without narrowing roots, sending
overlays, recording disk stamps, or invoking transformer factories. Declaration
only builds do not warm the worker. Solution-build warmups are joined by their
own project task, which keeps their concurrency within `--builders`.

Once a transform request has been accepted, Rotor never replays it after a
transport or worker failure because transformer plugins are arbitrary code. A
failure before the request is accepted may retry daemon startup. Startup uses a
private lock and handshake so concurrent compiler processes agree on one live
daemon and recover artifacts whose owning process has exited.

## Protocol v2

Each request is one JSON object with `protocol: 2`, `operation`, `projectDir`,
and `tsConfigPath`. The protocol is a hard cutover; v1 fields are rejected.

A source transform uses this shape:

```json
{
  "protocol": 2,
  "operation": "transform",
  "tsConfigPath": "C:/abs/project/tsconfig.json",
  "projectDir": "C:/abs/project",
  "compileFileNames": ["C:/abs/project/src/example.ts"],
  "fileNames": [
    "C:/abs/project/src/example.ts",
    "C:/abs/project/src/globals.d.ts"
  ],
  "rootFileNames": [
    "C:/abs/project/src/example.ts",
    "C:/abs/project/src/globals.d.ts"
  ],
  "changedFiles": [
    {
      "fileName": "C:/abs/project/src/example.ts",
      "text": "export const phase = \"memory\";\n"
    },
    {
      "fileName": "C:/abs/project/src/removed.ts",
      "deleted": true
    }
  ]
}
```

`fileNames` is the complete source snapshot for freshness and config file-set
invalidation. It does not replace the roots parsed from the config.
`rootFileNames` optionally narrows the worker program to files being compiled
plus project declarations. Within one session the limit only widens, and the
first transform that omits it or sends an empty list disables narrowing for the
rest of that session.

The first response prints transformed text and retains the corresponding
TypeScript transform result under an opaque handle. It does not generate trace
maps:

```json
{
  "diagnostics": [],
  "transformed": [
    {
      "fileName": "C:/abs/project/src/example.ts",
      "text": "export const phase = \"before:after:start\";\n"
    }
  ],
  "resultHandle": "3b17e882da428b238abff264239c92c3",
  "afterDeclarationsTransformers": 0,
  "logs": []
}
```

Rotor requests a trace map only when a diagnostic needs original disk
coordinates or a successful output source map needs composition:

```json
{
  "protocol": 2,
  "operation": "maps",
  "projectDir": "C:/abs/project",
  "tsConfigPath": "C:/abs/project/tsconfig.json",
  "resultHandle": "3b17e882da428b238abff264239c92c3",
  "fileNames": ["C:/abs/project/src/example.ts"]
}
```

A maps response contains `traceMaps` entries with `fileName` and the serialized
`traceMap`. The worker prints from the retained transformed nodes, so requesting
a map never runs a plugin again. Prefix and suffix transformer stages retain
separate handles; Rotor composes their maps with the native transform trace.

Every accepted result is released on success, error, or cancellation:

```json
{
  "protocol": 2,
  "operation": "release",
  "projectDir": "C:/abs/project",
  "tsConfigPath": "C:/abs/project/tsconfig.json",
  "resultHandle": "3b17e882da428b238abff264239c92c3",
  "outcome": "success"
}
```

The daemon reclaims a result when its owning process exits or its PID is
reused. Results from clients without a process owner expire after 30 minutes.
A worker session stays serialized and cannot be refreshed while one of its
results is retained.

`operation: "warm"` accepts only the common identity fields. `operation:
"validate"` loads and validates configured modules for declaration-only builds
without creating a language service or invoking a factory.

`afterDeclarationsTransformers` reports how many such transformers the worker
built or validated. The worker never runs them because declaration emit stays
native. `logs` contains stdout and stderr lines produced by plugins during that
request; the sidecar keeps protocol responses on its separate stdout writer.

`diagnostics` may contain:

- TypeScript config/program diagnostics converted to `{ category, code, file, start, length, message }`
- transformer resolution warnings using code `transformer-not-found`
- a `typescript-instance-mismatch` error when a plugin resolves another TypeScript copy
- a `typescript-not-found` error when the project has no resolvable `typescript` package
- request validation errors using code `invalid-request`
- expired or unknown handle errors using code `invalid-result-handle`
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
  from rbxtsc, listed in the rotor-extension section of `docs.md`.

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
