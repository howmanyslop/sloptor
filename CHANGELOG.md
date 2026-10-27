All notable changes to sloptor are documented here.

## [v3.0.0-beta.1] - 2026-10-27

Sloptor is now the native compiler only: the bundler, asset and deploy pipelines, scaffolding commands, and the `$asset`, `$git`, and `$buildTime` macros are removed. One-shot builds now share the machine through slots, API sessions transform in sidecars by default, and source maps cover more of the output.

### ⚠️ Breaking Changes

- **sloptor bundle**: `sloptor bundle`, `sloptor minify`, `sloptor pack`, and `sloptor sourcemap` are removed, so scripts that call them fail. Previously they bundled, minified, packed, and generated Rojo sourcemaps; run those steps with separate tools. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **sloptor dev**: `sloptor dev` is removed, so scripts that call it fail. Previously it ran watch, incremental compile, and `rojo serve`; run `sloptor build -w` for the compile loop and start Rojo separately. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **sloptor asset**: `sloptor asset` and `sloptor deploy` are removed along with the Open Cloud upload and deploy pipelines. Previously `asset sync` uploaded files and wrote `rotor-lock.json`, and `deploy` planned and applied changes to live resources. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **sloptor init**: `sloptor init`, `sloptor add`, `sloptor doctor`, and `sloptor migrate` are removed, so remove them from scripts and CI. Previously they scaffolded projects, edited `package.json`, checked setup, and migrated rbxtsc project configuration. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **sloptor build**: `--minify` is removed, so `sloptor build --minify` fails; drop the flag. Previously it stripped comments and whitespace and rewrote `t["x"]` to `t.x`. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **$asset**: `$asset` is removed, so replace each call with a value from `$env` or generated source. Previously it expanded to an asset ID from `rotor-lock.json`. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **$git**: `$git` is removed, so replace each call with a value from `$env` or generated source. Previously it read Git values during the build. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **$buildTime**: `$buildTime` is removed, so replace each call with a value from `$env` or generated source. Previously it expanded to the build timestamp. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **rotor.toml [assets]**: `rotor.toml` now defines only `parallelTransformers` and `flamework`, and the schema rejects other keys, so delete `[assets]` and the other removed sections. Previously `[assets]` configured `sloptor asset sync`. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **rotor.toml**: `rotor.config.ts` and `rotor.config.js` are no longer loaded. Move any supported settings into `rotor.toml`. Previously they were evaluated as executable config. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **@rotor-rbx/rotor**: API sessions now transform in the server's sidecar workers by default. To keep transforms in the client process, pass `createBuildSession({ transformerCallbacks: true })`; previously every transform ran there, one at a time. - by @howmanysmall in [#171](https://github.com/howmanyslop/sloptor/pull/171)

### 🐞 Fixed

- **@rotor-rbx/rotor**: API builds under Node's `--experimental-package-map` now resolve the project's TypeScript and transformer packages from each project and its ancestors. Previously those lookups failed with missing-package errors. - by @christopher-buss in [#166](https://github.com/howmanyslop/sloptor/pull/166)
- **$env**: Changing a `$env` value or a `$file` target now rebuilds files that use it in incremental and watch builds. Previously those builds kept stale output. - by @christopher-buss and @howmanysmall in [#168](https://github.com/howmanyslop/sloptor/pull/168)
- **sloptor build**: With `compilerOptions.sourceMap`, `.luau.map` now maps callback bodies, function expressions, object methods, class members, and continuation lines to TypeScript, and runtime header and comment lines stay unmapped. Previously only statement starts had mappings, leaving 3,603 of 6,003 non-blank lines in a 300-function fixture unmapped. - by @christopher-buss in [#167](https://github.com/howmanyslop/sloptor/pull/167)

### 🔧 Changed

- **sloptor build**: One-shot `sloptor build` and `sloptor check` runs now take one of several machine-wide slots (one per 6 GiB of memory, at most one per two CPUs), and split memory and sidecar workers among the runs holding slots. Previously each run sized itself as if it owned the machine; set `ROTOR_MAX_BUILDS=0` to disable slots. - by @howmanysmall in [#171](https://github.com/howmanyslop/sloptor/pull/171)
- **sloptor build**: One-shot builds now cap the Go memory limit at the larger of 8 GiB or twice the live heap, and at held memory plus three quarters of free memory. Previously the limit was 75% of memory up to 16 GiB, so the heap could grow to about five times the live set. - by @howmanysmall in [#171](https://github.com/howmanyslop/sloptor/pull/171)
- **sloptor build**: `.luau.map` now carries segments for expressions, object fields, and operators, mapped to their exact TypeScript columns in UTF-16. Previously each Luau line had one column-zero statement segment; generated Luau is unchanged. - by @christopher-buss in [#169](https://github.com/howmanyslop/sloptor/pull/169)

## [v2.9.0-beta.1] - 2026-10-21

This beta changes how `sloptor build` resolves package imports and restores incremental state, adds opt-in transformer paths, and makes `@rotor-rbx/rotor` ES-module only. Incremental projects rebuild once after upgrading because the manifest format changed.

### ⚠️ Breaking Changes

- **@rotor-rbx/rotor**: The package is ES-module only and no longer has a `require` export condition, so CommonJS code that called `require("@rotor-rbx/rotor")` must use `await import("@rotor-rbx/rotor")` instead. Node support is now `^20.19.0 || >=22.12.0`, previously `>=18`. - by @howmanysmall [<samp>(4bcb76a)</samp>](https://github.com/howmanyslop/sloptor/commit/4bcb76a)

### 🚀 Added

- **rotor.toml [parallelTransformers]**: Set `workers = 2` and `batchIndependent = true` to split one-shot `sloptor build` transformers across two workers, after attesting the chain with `typescript` and `sha256`. It is experimental and off by default; watch, dev, and API builds stay serial. - by @christopher-buss in [#147](https://github.com/howmanyslop/sloptor/pull/147)
- **sloptor build**: With `ROTOR_REACT_SYNTAX_HOST=1`, a sole supported pinned React compiler runs on authored source without a TypeScript Program. Previously every transformer ran through a Program; mixed chains and unsupported versions still do. - by @christopher-buss in [#146](https://github.com/howmanyslop/sloptor/pull/146)

### 🐞 Fixed

- **sloptor build**: Rojo discovery no longer follows junction and symlink cycles until the path limit. Previously a loop in `node_modules` slowed every build. - by @christopher-buss in [#159](https://github.com/howmanyslop/sloptor/pull/159)
- **sloptor build**: Manifests now use project-relative source identities, so a manifest restored from another checkout or OS selects zero sources when nothing changed. Previously absolute paths made the restored manifest rebuild every file. - by @christopher-buss in [#160](https://github.com/howmanyslop/sloptor/pull/160)
- **sloptor build**: Later plugins and source traces now use positions in the file on disk after a native jecs rewrite, matching the JavaScript jecs transformer. Previously they pointed into the rewritten text. - by @howmanysmall [<samp>(d2d6bcf)</samp>](https://github.com/howmanyslop/sloptor/commit/d2d6bcf)

### 🔧 Changed

- **sloptor build**: Package imports now ignore named `*.project.json` files inside dependencies, so `TS.getModule(script, "@scope", "lib").out` is emitted even when a dependency has a generated test project. Previously that file redirected the path, which `rbxtsc 3.0.0` still does. - by @christopher-buss in [#145](https://github.com/howmanyslop/sloptor/pull/145)
- **sloptor build**: Solution watch (`sloptor build --build -w`) now watches a package's layout files, so editing its `default.project.json` or a `*.project.json` it references rebuilds that package. Previously these files were not watched. - by @christopher-buss in [#145](https://github.com/howmanyslop/sloptor/pull/145)
- **sloptor build**: Sidecar transforms of 512 or more files now split across up to `min(4, CPUs/2)` Node workers; set `ROTOR_SIDECAR_SHARDS=1` to keep one. Previously one worker transformed every file. - by @howmanysmall in [#150](https://github.com/howmanyslop/sloptor/pull/150)
- **sloptor build**: `rbxts-transformer-jecs` runs natively when the installed package matches the build the port was written for. Other builds stay in the sidecar, and `ROTOR_NATIVE_JECS=0` forces the sidecar. - by @howmanysmall in [#150](https://github.com/howmanyslop/sloptor/pull/150)
- **sloptor build**: Derived programs default to 8 checkers unless `--checkers` or tsconfig `checkers` is set. Previously they used the default checker count. - by @howmanysmall in [#150](https://github.com/howmanyslop/sloptor/pull/150)
- **sloptor build**: No-change builds skip building package import mappings when no source file is selected. Previously those mappings were built on every build. - by @christopher-buss in [#142](https://github.com/howmanyslop/sloptor/pull/142)
- **sloptor build**: Overlay updates parse changed files in parallel on multi-checker programs. This can raise peak memory during the update. - by @christopher-buss in [#149](https://github.com/howmanyslop/sloptor/pull/149)

## [v2.8.3] - 2026-10-17

This patch changes solution watch rebuilds, watch-mode GC and memory defaults, and solution build scheduling. It also adds transformer phase timings to `--verbose` output and changes several error messages.

### 🚀 Added

- **rotor build**: With `--verbose`, transformer worker phases now print with their durations in ms: config, program, factories, transform (includes parent repair), parent repair, and print/trace map. Previously these phases were not reported. - by @christopher-buss in [#133](https://github.com/howmanyslop/sloptor/pull/133)

### 🐞 Fixed

- **rotor build**: Solution watch (`rotor build --build -w`) now rebuilds only the changed source after a comment-only edit in a referenced project, and still rebuilds dependents when a declaration changes. Previously every dependent recompiled, and a referenced project's output could trigger a second build cycle. - by @christopher-buss in [#130](https://github.com/howmanyslop/sloptor/pull/130)

### 🔧 Changed

- **rotor build**: `rotor build -w`, `rotor check -w`, and `rotor dev` now default to `GOGC=100` and a soft Go memory limit capped at 6 GiB, unless `GOGC` or `GOMEMLIMIT` is set. Previously they used `GOGC=400` and a soft limit of 75% of effective memory, capped at 16 GiB. - by @christopher-buss in [#131](https://github.com/howmanyslop/sloptor/pull/131)
- **rotor build**: Declaration files now emit concurrently per checker group, and overlap the transformer sidecar round trip when one runs. Previously each checker group emitted serially after the transformer finished. - by @howmanysmall in [#127](https://github.com/howmanyslop/sloptor/pull/127)
- **rotor build**: In a solution build with `--builders N`, a project waiting on a dependency no longer occupies one of the N workers, so independent projects start while it waits. Previously a blocked project held its worker until the dependency finished. - by @christopher-buss in [#133](https://github.com/howmanyslop/sloptor/pull/133)
- **tsconfig.json**: `rbxts` option errors now begin with `invalid "rbxts" config in`, and tsconfig parse failures begin with `failed to parse tsconfig at`. Previously both began with a capital letter, `Invalid` and `Failed`. - by @howmanysmall in [#132](https://github.com/howmanyslop/sloptor/pull/132)
- **rotor migrate**: Its missing-config and overwrite errors no longer end in a period, such as `re-run with --force to replace it`. Previously they ended with `replace it.` - by @howmanysmall in [#132](https://github.com/howmanyslop/sloptor/pull/132)

## [v2.8.2] - 2026-10-16

This patch release fixes `rotor build` diagnostics, Windows path resolution, and incremental rebuilds after a dependency's Rojo project changes. Incremental projects do one full rebuild on the first build after upgrading, and the Windows path change only affects Windows.

### 🐞 Fixed

- **rotor build**: A build with type errors now reports them from every file in one run, including `--json` output, and type errors take precedence over comment-directive errors, as in `rbxtsc`. Previously the build stopped at the first file with errors. - by @howmanysmall in [#123](https://github.com/howmanyslop/sloptor/pull/123)
- **rotor build**: Paths reached through a Windows directory junction, `subst` drive, or 8.3 short name now resolve to their real spelling, matching the Node transformer sidecar. Previously, transformer output on these paths could be rejected as missing from the program. - by @howmanysmall in [#124](https://github.com/howmanyslop/sloptor/pull/124)
- **rotor build**: Incremental builds now rebuild when a dependency's Rojo project changes its path mappings, even when the file's modification time is unchanged. Previously, renaming a dependency's Rojo project left stale `TS.import` paths in unchanged files. - by @howmanysmall in [#125](https://github.com/howmanyslop/sloptor/pull/125)

## [v2.8.1] - 2026-10-12

This patch release changes incremental `rotor build` rebuilds and JSX emit for underscore-prefixed components, and adds a diagnostic for referenced functions declared after a final statement. Existing incremental manifests may rebuild affected importers once after upgrading.

### 🐞 Fixed

- **rotor build**: Editing any `.d.ts` file now rebuilds its importers, including re-exports, external packages, and global declarations, which can rebuild every source. Previously a caller kept table indexing after its imported return type changed to `LuaTuple<[number, string]>`, instead of `local count, text = values()`. - by @christopher-buss in [#120](https://github.com/howmanyslop/sloptor/pull/120)
- **rotor build**: Referencing a function declared after a final `return`, `break`, or `continue` now fails with `Function "name" is unreachable after a final statement.`, so move the declaration before the first statement that uses it. Previously the build succeeded and the function was a nil binding. - by @christopher-buss in [#121](https://github.com/howmanyslop/sloptor/pull/121)

### 🔧 Changed

- **rotor build**: JSX tags such as `<_View />` now emit the component binding `React.createElement(_View)`, which diverges from `rbxtsc 3.0.0`. Previously they emitted the host-tag string `React.createElement("_View")`. Lowercase and hyphenated intrinsic tags are unchanged. - by @christopher-buss in [#120](https://github.com/howmanyslop/sloptor/pull/120)

## [v2.8.0] - 2026-10-11

This release adds project-reference import resolution to `rotor build`, fixes package `exports`, symlinked paths, and vararg shorthand captures in `rotor build`, and changes solution watch in `rotor dev` to refresh only the projects affected by an edit.

### 🚀 Added

- **rotor build**: Solution builds now resolve imports of referenced projects to their emitted Luau, including transitive references, separate `declarationDir` paths, and each referenced project's own `luau` setting. Referenced projects that omit `composite: true`, disable emit, or share a `tsBuildInfoFile` or output directory with another project now fail the build with a diagnostic. - by @christopher-buss in [#115](https://github.com/howmanyslop/sloptor/pull/115)

### 🐞 Fixed

- **rotor build**: Package `exports` entries with object-valued custom conditions, and wildcard subpaths such as `"./test/*"`, now resolve to their runtime Luau target. Previously the import fell back to a `.luau` sibling of the declaration, which is wrong when `declarationDir` and the output directory differ. - by @christopher-buss in [#113](https://github.com/howmanyslop/sloptor/pull/113)
- **rotor build**: An explicit `--rojo` or `--includePath` through a symlinked directory now resolves to its real path, matching config and output paths. Previously a project-reference build could fail with a false "must mount" error. - by @christopher-buss in [#117](https://github.com/howmanyslop/sloptor/pull/117)
- **rotor build**: A hand-written `index.d.ts` outside `rootDir` now maps to the `init.luau` beside it. Previously the import waited on an `index` child that does not exist. - by @christopher-buss in [#115](https://github.com/howmanyslop/sloptor/pull/115)
- **rotor build**: A rest parameter used only as a shorthand property, such as `function f(...args) { return { args }; }`, keeps its `local args = { ... }` capture. Previously the capture was dropped and `args` was `nil` at runtime. - by @howmanysmall in [#106](https://github.com/howmanyslop/sloptor/pull/106)
- **rotor dev**: Solution watch recovers when a referenced project is removed. It reopens only the watches that failed or terminated, and directory or root `.json` events now rebuild the owning project. - by @christopher-buss in [#118](https://github.com/howmanyslop/sloptor/pull/118)

### 🔧 Changed

- **rotor dev**: Solution watch refreshes reference metadata only for projects affected by an edit. Previously every edit rebuilt it for the whole solution. In the 40-package benchmark in the PR, a leaf edit falls from about 575–626 ms to 31–37 ms. - by @christopher-buss in [#118](https://github.com/howmanyslop/sloptor/pull/118)

## [v2.7.2] - 2026-10-06

This patch release changes how `rotor build` starts its transformer sidecar when `NODE_OPTIONS` contains `--experimental-package-map`, which pnpm can set.

### 🐞 Fixed

- **rotor build**: Builds with transformer plugins now resolve `typescript` and plugins when the parent `NODE_OPTIONS` includes `--experimental-package-map`, as pnpm sets with `nodeExperimentalPackageMap`. The sidecar drops only that flag from its own environment, and the parent environment is unchanged. Previously the build failed with "Could not resolve the `typescript` package" from the transformer sidecar. - by @christopher-buss in [#105](https://github.com/howmanyslop/sloptor/pull/105)

## [v2.7.1] - 2026-10-03

This patch release fixes `rotor api-server` request handling and sidecar stderr draining, stops rewriting outputs whose bytes are unchanged, and makes `.d.ts` member order deterministic. Flamework builds with `noInclude` no longer write two `include/flamework` files.

### 🐞 Fixed

- **rotor api-server**: `rotor api-server` now decodes requests and callback responses of any size, and fails transport errors with `BUILD_FAILED` and their cause. Previously a response over 16 MiB stopped the server, and a cancelled multi-config build could panic with `Checker was previously cancelled`. - by @christopher-buss in [#98](https://github.com/howmanyslop/sloptor/pull/98)
- **rotor build**: `rotor build` now reads sidecar stderr lines of any length and keeps at most 64 KiB of each in the compiler log, marked `[truncated]`. Previously a stderr line over 1 MiB stalled the transformer worker until its response timeout. - by @christopher-buss in [#98](https://github.com/howmanyslop/sloptor/pull/98)
- **rotor api-server**: Transformer plugins whose real path is outside the project, such as a pnpm store, now resolve `typescript` from the project, as the CLI does. Previously the API build failed with `Cannot find module 'typescript'`. - by @christopher-buss in [#101](https://github.com/howmanyslop/sloptor/pull/101)
- **rotor build**: Declaration files keep the same member order across builds, including object-literal properties widened to `?: undefined`. Previously the order depended on file scheduling, so identical builds could produce different `.d.ts` output. - by @christopher-buss in [#103](https://github.com/howmanyslop/sloptor/pull/103)

### 🔧 Changed

- **rotor build**: Outputs, copied non-compiled files, `include/` runtime files, Flamework artifacts, and `rbxts.copyfiles.json` are left untouched when their bytes match what is on disk, even without `--writeOnlyChanged`. Previously every build rewrote them, so mtime-keyed caches such as Nx re-cached every output. - by @christopher-buss in [#100](https://github.com/howmanyslop/sloptor/pull/100)
- **rotor build**: With `noInclude`, a build now writes only `include/flamework/flamework.build` and skips `include/flamework/config.json` and `globs.json`. This diverges from `rbxtsc 3.0.0` and `rbxts-transformer-flamework` 1.3.2, which write both for every game build. A project whose only build uses `noInclude` no longer gets these two files. - by @christopher-buss in [#102](https://github.com/howmanyslop/sloptor/pull/102)

## [v2.7.0] - 2026-10-02

This release changes Flamework `addPaths` to emit static imports, moves the npm package's native executable into platform packages, adds an opt-in `optimizedArrayAppends` option and a JavaScript build API, and fixes solution builds, `tsconfig.json` inheritance, external library emit, and source map paths.

### ⚠️ Breaking Changes

- **@rotor-rbx/rotor**: The npm package now gets its executable from an optional dependency, so `--no-optional` installs fail with a missing-package error instead of downloading the binary. Reinstall without `--no-optional`, or install the named `@rotor-rbx/rotor-<platform>` package at the same version. - by @christopher-buss in [#92](https://github.com/howmanyslop/sloptor/pull/92)

### 🚀 Added

- **@rotor-rbx/rotor**: Adds `createBuildSession()`, an async client that keeps one native server open across builds. Use `await using session = createBuildSession();` and `session.build({ project: "." })`, or `session.build({ roots })` for several solution roots. - by @christopher-buss in [#92](https://github.com/howmanyslop/sloptor/pull/92)
- **rotor build**: Adds the opt-in `--optimizedArrayAppends` flag, or `optimizedArrayAppends` under `rbxts` in `tsconfig.json`, off by default. When enabled, a loop over a fresh local array turns `table.insert(result, value)` into `_resultLength += 1` and `result[_resultLength] = value`, which diverges from `rbxtsc 3.0.0`. - by @christopher-buss in [#81](https://github.com/howmanyslop/sloptor/pull/81)

### 🐞 Fixed

- **tsconfig.json**: `extends` now resolves package-export subpaths like `@scope/tsconfig/roblox` and dotted names like `./tsconfig.base`, and a child config keeps an inherited `types` list. Previously package-export extends failed with `Failed to resolve tsconfig extends`, and an inherited `types` list was replaced by `["*"]`. - by @christopher-buss in [#77](https://github.com/howmanyslop/sloptor/pull/77) and [#80](https://github.com/howmanyslop/sloptor/pull/80)
- **rotor build**: A solution coordinator whose empty `files` and `include` are inherited through `extends` is skipped, and watch rebuilds its references when its config chain changes. Previously the coordinator was built, which could delete a referenced project's `out/index.d.ts` and fail dependents with `TS6305`. - by @christopher-buss in [#80](https://github.com/howmanyslop/sloptor/pull/80)
- **rotor build**: A workspace package resolved through `exports.source` and a custom condition is now type-checked but not transformed into Luau. Previously its `src` files were also transformed, failing with `noRojoData` when Rojo had no mapping for them. - by @christopher-buss in [#84](https://github.com/howmanyslop/sloptor/pull/84)
- **rotor build**: Adjacent `.luau.map` files now set `file` to the generated Luau name and make `sources` relative to the map, so checkouts at different paths produce identical maps. Previously both fields held absolute paths from the checkout. - by @christopher-buss in [#85](https://github.com/howmanyslop/sloptor/pull/85)

### 🔧 Changed

- **emit**: A standalone `Flamework.addPaths` or `addPathsGlob` call, including one nested in a function, now emits one `TS.import` per matching module, sorted by path. Previously it emitted a runtime `Flamework._addPaths` loader that bundlers cannot follow, which diverges from `rbxtsc 3.0.0`. - by @christopher-buss in [#72](https://github.com/howmanyslop/sloptor/pull/72) and [#76](https://github.com/howmanyslop/sloptor/pull/76)
- **rotor build**: Adding or removing a module in a plain `addPaths` directory now recompiles the calling file in incremental and watch builds, because the emitted imports depend on that set. - by @christopher-buss in [#76](https://github.com/howmanyslop/sloptor/pull/76)
- **@rotor-rbx/rotor**: The native executable now comes from an exact-version platform package for macOS, Linux, or Windows on x64 or arm64, not a GitHub Releases download. The `@rotor-rbx/rotor/scripts/install.js` subpath still resolves the binary without downloading it. - by @christopher-buss in [#92](https://github.com/howmanyslop/sloptor/pull/92)

## [v2.6.0] - 2026-09-25

This release adds NDJSON output for `--watch --json` in `rotor build` and `rotor check`, changes how `rotor build --build` inherits `rbxts` options, and fixes packed bundles, declaration-emit diagnostics, and referenced-project reuse. Solutions that relied on an entry tsconfig's `rbxts` options need a config edit.

### ⚠️ Breaking Changes

- **rotor build**: Under `--build`, referenced projects no longer inherit `rbxts` options from the entry tsconfig, except a coordinator's key, and flags now override their own `rbxts` keys. Previously the entry's options applied and keys beat flags; set inherited options in each reference's `rbxts` key instead. - by @christopher-buss in [#70](https://github.com/howmanyslop/sloptor/pull/70)

### 🚀 Added

- **rotor build**: `rotor build --watch --json`, `rotor build --build --watch --json`, and `rotor check --watch --json` stream NDJSON on stdout, with one `buildStart` and one `buildEnd` per build, then a `watching` event after the first build. Previously `--json` was ignored with `--watch` and the output stayed styled. - by @christopher-buss in [#68](https://github.com/howmanyslop/sloptor/pull/68)

### 🐞 Fixed

- **rotor build**: After a direct build, `--build` reuses up-to-date referenced projects instead of recompiling them. Previously the incremental state differed from the direct build, so every `--build` run recompiled the referenced projects. - by @christopher-buss in [#70](https://github.com/howmanyslop/sloptor/pull/70)
- **rotor pack**: Packed Luau modules resolve string requires (`require("./x")`, `require("@self/x")`) against the bundle, accept `if` expressions as operands, and fit modules with 199 top-level locals under Luau's 200-register limit. Previously these failed at runtime or during compilation of the bundle. - by @howmanysmall and @uproot in [#64](https://github.com/howmanyslop/sloptor/pull/64)
- **diagnostics**: Declaration-emit failures such as `TS4023` now report their TypeScript code, message, and file location in regular, solution, and declaration-only builds. Previously the build failed with a generic compile error. - by @howmanysmall in [#65](https://github.com/howmanyslop/sloptor/pull/65)
- **diagnostics**: Generic `this` receivers that can become `void`, and mismatched method and callback assignments, now report `noUnstableThisType`, `expectedMethodGotFunction`, or `expectedFunctionGotMethod`, as the pinned upstream `rbxtsc` does. - by @howmanysmall in [#69](https://github.com/howmanyslop/sloptor/pull/69)
- **rotor check**: Edits made right after a `rotor check --watch` result are no longer missed. Previously the watch loop recorded file state after reporting, so an edit in that window could be skipped. - by @christopher-buss in [#68](https://github.com/howmanyslop/sloptor/pull/68)

### 🔧 Changed

- **emit**: Destructuring Set, Map, and generator values keeps iterator position across omitted and rest elements, numeric `for` loops re-read changing bounds, and `switch` cases compare against the original operand. Previously iterators restarted, literal-typed bounds were hoisted, and cases saw mutated operands; output now diverges from `rbxtsc 3.0.0`. - by @howmanysmall in [#69](https://github.com/howmanyslop/sloptor/pull/69)
- **emit**: Luau `vector` math methods compile to operators: `a.sub(b)` now emits `a - b`, where `rbxtsc 3.0.0` emits `a:sub(b)`, which errors at runtime. Projects whose `@rbxts/types` lacks these declarations still compile, but those calls need the newer declarations to type-check. - by @howmanysmall in [#66](https://github.com/howmanyslop/sloptor/pull/66)
- **diagnostics**: `rotor build --json` reports diagnostic `file` paths relative to the project directory, or the solution tsconfig directory with `--build`, matching `rotor check --json`. Previously they were relative to the working directory. - by @christopher-buss in [#68](https://github.com/howmanyslop/sloptor/pull/68)
- **rotor build**: Styled `--build --watch` passes show structured diagnostics with code frames and real durations, and a failed config reload prints as a failed build. Previously a reload error printed as a bare line on stderr. - by @christopher-buss in [#68](https://github.com/howmanyslop/sloptor/pull/68)

## [v2.5.0] - 2026-09-18

This release lets `rotor build` compile named `async` and generator function expressions, fixes Flamework guard imports in pnpm workspaces, and makes declaration-only builds stop calling transformer plugin factories. Projects whose transformer plugins load a second TypeScript copy now fail with a new diagnostic.

### ⚠️ Breaking Changes

- **rotor build**: A transformer plugin that resolves a different `typescript` file than the project now fails with `typescript-instance-mismatch`. Previously the build did not check this. Remove the plugin's nested `node_modules/typescript` so it resolves the project's copy. - by @howmanysmall in [#56](https://github.com/howmanyslop/sloptor/pull/56)

### 🚀 Added

- **rotor build**: Named `async function` and `function*` expressions now compile, including as call arguments such as `registerOnClose(async function onBindToCloseAsync() {})`. Previously they failed with `Function expression names are not supported!`, which `rbxtsc 3.0.0` also reports. Async generator expressions still fail with `noAsyncGeneratorFunctions`. - by @howmanysmall in [#62](https://github.com/howmanyslop/sloptor/pull/62)

### 🐞 Fixed

- **rotor build**: Flamework `createGuard` in a pnpm workspace no longer fails with `noUnscopedModule` when `@rbxts/t` resolves through a workspace-root hoist and the consuming package has no runtime link to it. The guard now imports through Flamework's prelude. Previously the error named the first authored import, which resolved correctly. - by @howmanysmall in [#63](https://github.com/howmanyslop/sloptor/pull/63)
- **rotor build**: An overlay on a file reached through a symlinked parent directory now replaces that file's emitted Luau. Transformer plugins also see the physical project path as their working directory and `--project`, which matters for macOS `/var` aliases and Windows short names. Previously the overlay was ignored and the on-disk file was emitted. - by @howmanysmall in [#56](https://github.com/howmanyslop/sloptor/pull/56)
- **rotor build**: Comments now reach later transformer plugins even when `removeComments` is enabled. Previously the intermediate print dropped them. - by @howmanysmall in [#56](https://github.com/howmanyslop/sloptor/pull/56)

### 🔧 Changed

- **rotor build**: With `--build --emitDeclarationOnly`, transformer plugins are validated (module loads, exports a factory, same TypeScript instance) without calling their factories or running their visitors. Previously the full plugin round trip ran and its output was discarded, so a plugin that threw during transform failed the declaration build. - by @howmanysmall in [#56](https://github.com/howmanyslop/sloptor/pull/56)

## [v2.4.0] - 2026-09-03

This release adds native declaration output, declaration-signature incremental builds, and the tsgo-compatible `--singleThreaded` build control. It also improves transformer-sidecar visibility and restores native Flamework parity for macro imports, decorator identities, section comments, and boolean union guards.

### ⚠️ Breaking Changes

- **`afterDeclarations` transformers**: Native declaration emit no longer runs custom `afterDeclarations` transformers, so markers or other edits previously added to `.d.ts` files are no longer emitted; upgrade by removing the `afterDeclarations` dependency or keeping its behavior out of published declarations, and expect one warning per affected project - by @christopher-buss in [#50](https://github.com/howmanyslop/sloptor/pull/50)

### 🚀 Added

- **`rotor build`**: Adds the tsgo-compatible `--singleThreaded` flag, which overrides `compilerOptions.singleThreaded`, forces one solution builder and one checker per program, and takes precedence over `--builders` and `--checkers;` use `rotor build --singleThreaded` for serialized builds - by @howmanysmall [<samp>(5dc5d68)</samp>](https://github.com/howmanyslop/sloptor/commit/5dc5d68)

### 🐞 Fixed

- **Flamework macros**: Preserves synthesized imports and hoisted prerequisites when `Dependency<T>()` or another native Flamework macro appears under an assignment, compound assignment, unary update, or `delete` expression, preventing missing `Flamework` bindings and TS2304 failures - by @teakzc in [#52](https://github.com/howmanyslop/sloptor/pull/52)
- **Flamework decorators**: Keys `Reflect.decorate` UIDs from the decorator declaration file rather than the file applying the decorator, including named ancestors such as `Decorators.Component`, so cross-file decorator lookups agree with the declaration UID - by @teakzc in [#53](https://github.com/howmanyslop/sloptor/pull/53)
- **Flamework output**: Emits separate metadata and decorator section comments for the class and each decorated member, retaining member names such as `Class.member` and matching the reference output - by @teakzc in [#54](https://github.com/howmanyslop/sloptor/pull/54)
- **Flamework guards**: Collapses unions containing both boolean literals into `t["boolean"]`, preserving `t["literal"](true)` for a lone boolean literal and matching upstream guard order - by @teakzc in [#55](https://github.com/howmanyslop/sloptor/pull/55)
- **`rotor build`**: Invalidates incremental output when an external transformer’s tsconfig entry, declared package version, or entry-file contents change, so files are not mixed between old and new transformer output - by @christopher-buss in [#50](https://github.com/howmanyslop/sloptor/pull/50)
- **Transformer sidecar**: Keeps global scripts, `.d.mts` and `.d.cts` files, and modules containing `declare global` in narrowed incremental root sets, preserving checker results for ambient declarations - by @christopher-buss [<samp>(2a774eb)</samp>](https://github.com/howmanyslop/sloptor/commit/2a774eb)
- **Declaration source maps**: Keeps `.d.ts.map` generated columns aligned after declaration module-specifier rewrites, including UTF-16 columns, CRLF files, empty lines, and multiple edits - by @christopher-buss in [#50](https://github.com/howmanyslop/sloptor/pull/50)

### 🔧 Changed

- **`rotor build`**: Emits `.d.ts` and `.d.ts.map` files natively through tsgo for projects with or without transformer plugins, using the original pre-transform program so source transformers cannot alter published types; declaration `paths` and `baseUrl` specifiers are rewritten to runtime-resolvable relative paths, including synthesized and nested `import()` types - by @christopher-buss in [#50](https://github.com/howmanyslop/sloptor/pull/50)
- **`rotor build`**: Uses declaration signatures to stop incremental rebuild propagation when a changed dependency’s published declaration is unchanged, while still rebuilding importers when the declaration changes or comparison is unavailable; unchanged projects can select no source files and skip the transformer pipeline - by @christopher-buss in [#50](https://github.com/howmanyslop/sloptor/pull/50)
- **`rotor build`**: Makes independent solution projects use separate transformer-sidecar sessions and shares declaration/JSON parsing during one solution drain, reducing unnecessary contention and repeated parsing - by @howmanysmall [<samp>(a646ad0)</samp>](https://github.com/howmanyslop/sloptor/commit/a646ad0) and @howmanysmall [<samp>(c60a862)</samp>](https://github.com/howmanyslop/sloptor/commit/c60a862)
- **`rotor build --timings`**: Changes the timing JSON schema from version 1 to version 2. `totalMs` now reports command wall time, stage values report aggregate `workMs`, and solution builds include per-project status, timing, counts, runtime metadata, sidecar metrics, and parse-cache counts - by @howmanysmall [<samp>(be0672)</samp>](https://github.com/howmanyslop/sloptor/commit/dbe0672)
- **`rotor build --verbose`**: Reports transformer-sidecar wait, preparation, round-trip, response decoding, overlay, and per-plugin timing stages, and includes Node CPU metrics in timing data - by @christopher-buss [<samp>(0b8a13f)</samp>](https://github.com/howmanyslop/sloptor/commit/0b8a13f)
- **Declaration paths**: Resolves rewritten specifiers using the containing file’s module mode, making declaration output deterministic under `module: preserve` and `moduleResolution: bundler` instead of depending on map iteration order - by @christopher-buss [<samp>(4a5167b)</samp>](https://github.com/howmanyslop/sloptor/commit/4a5167b)

## [v2.3.3] - 2026-08-20

This patch release restores compatibility for projects using the legacy `rbxts-transformer-flamework` plugin. Builds use the Node sidecar when native Flamework is not active, while native and legacy Flamework configurations remain mutually exclusive.

### 🐞 Fixed

- **rotor build**: Runs `rbxts-transformer-flamework` through the Node sidecar when no `[flamework]` configuration applies, including projects whose configured `[flamework.profiles.*]` entry does not match the active `tsconfig.json` - by @christopher-buss in [#32](https://github.com/howmanyslop/sloptor/pull/32)
- **rotor doctor**: Checks Node.js and sidecar requirements for legacy `rbxts-transformer-flamework` configurations instead of requiring `rotor migrate flamework`; configuring native `[flamework]` alongside the legacy plugin remains a reported conflict - by @christopher-buss in [#32](https://github.com/howmanyslop/sloptor/pull/32)

## [v2.3.2] - 2026-08-20

This patch release adds compiler-shaped build invocation support for TypeScript integrations such as Nx. You can now invoke the existing build command without naming the `build` subcommand.

### 🚀 Added

- **rotor build**: Accepts `rotor --build [path]` and `rotor -b [path]` as aliases for `rotor build --build [path]`, including compiler integration flags and build help - by @christopher-buss in [#31](https://github.com/howmanyslop/sloptor/pull/31)

## [v2.3.1] - 2026-08-15

This patch release fixes native Flamework builds that use skipped semantic diagnostics, transitive type-only macro calls, and JSX or generated runtime imports. It also fixes clickable diagnostic file links in terminal output and adds an opt-in Flamework source reuse setting.

### 🚀 Added

- **rotor.toml [flamework]**: Adds `skipUnchangedFiles = true`, which reuses parse trees for sources that native Flamework leaves unchanged; the same option is available under `[flamework.profiles.<tsconfig>]` - by @howmanysmall in [#28](https://github.com/howmanyslop/sloptor/pull/28)

### 🐞 Fixed

- **rotor build**: Native Flamework builds with `noSemanticDiagnostics = true` now emit unresolved value identifiers as their raw names instead of failing with an internal compiler error, and preserve runtime imports when the checker cannot resolve their value use - by @howmanysmall in [#28](https://github.com/howmanyslop/sloptor/pull/28)
- **rotor build**: Imports used only as JSX factories, such as `React` in `<Frame />`, now remain in emitted Luau instead of being elided - by @howmanysmall in [#28](https://github.com/howmanyslop/sloptor/pull/28)
- **rotor build**: Native Flamework now injects the required numeric callsite argument for `Modding.Caller<"line">` calls reached through type-only, transitive, diamond, or cyclic imports, avoiding the runtime `callsiteId not generated` failure - by @howmanysmall in [#28](https://github.com/howmanyslop/sloptor/pull/28)
- **rotor build**: Flamework guard generation now emits `t.any` for `any` types, and Flamework projects using `rootDirs` with a null `rootDir` resolve their source roots correctly - by @howmanysmall in [#28](https://github.com/howmanyslop/sloptor/pull/28)
- **diagnostics**: Terminal hyperlinks now use absolute, percent-encoded `file://` URIs with `#line:col` fragments, including correct handling for spaces, empty paths, and UNC paths - by @howmanysmall in [#27](https://github.com/howmanyslop/sloptor/pull/27)

### 🔧 Changed

- **rotor build**: Native Flamework now reprints and re-parses every source by default to preserve upstream source-print parity; set `skipUnchangedFiles = true` to opt into reusing unchanged source trees and the previous sparse-update behavior - by @howmanysmall in [#28](https://github.com/howmanyslop/sloptor/pull/28)

## [v2.3.0] - 2026-08-12

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.3.0).

## [v2.2.1-fork.10] - 2026-08-08

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.10).

## [v2.2.1-fork.9] - 2026-08-07

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.9).

## [v2.2.1-fork.8] - 2026-08-06

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.8).

## [v2.2.1-fork.7] - 2026-08-04

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.7).

## [v2.2.1-fork.6] - 2026-08-03

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.6).

## [v2.2.1-fork.5] - 2026-08-02

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.5).

## [v2.2.1-fork.4] - 2026-07-31

No release notes. See the [tag](https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.4).

[v3.0.0-beta.1]: https://github.com/howmanyslop/sloptor/compare/v2.9.0-beta.1...v3.0.0-beta.1
[v2.9.0-beta.1]: https://github.com/howmanyslop/sloptor/compare/v2.8.3...v2.9.0-beta.1
[v2.8.3]: https://github.com/howmanyslop/sloptor/compare/v2.8.2...v2.8.3
[v2.8.2]: https://github.com/howmanyslop/sloptor/compare/v2.8.1...v2.8.2
[v2.8.1]: https://github.com/howmanyslop/sloptor/compare/v2.8.0...v2.8.1
[v2.8.0]: https://github.com/howmanyslop/sloptor/compare/v2.7.2...v2.8.0
[v2.7.2]: https://github.com/howmanyslop/sloptor/compare/v2.7.1...v2.7.2
[v2.7.1]: https://github.com/howmanyslop/sloptor/compare/v2.7.0...v2.7.1
[v2.7.0]: https://github.com/howmanyslop/sloptor/compare/v2.6.0...v2.7.0
[v2.6.0]: https://github.com/howmanyslop/sloptor/compare/v2.5.0...v2.6.0
[v2.5.0]: https://github.com/howmanyslop/sloptor/compare/v2.4.0...v2.5.0
[v2.4.0]: https://github.com/howmanyslop/sloptor/compare/v2.3.3...v2.4.0
[v2.3.3]: https://github.com/howmanyslop/sloptor/compare/v2.3.2...v2.3.3
[v2.3.2]: https://github.com/howmanyslop/sloptor/compare/v2.3.1...v2.3.2
[v2.3.1]: https://github.com/howmanyslop/sloptor/compare/v2.3.0...v2.3.1
[v2.3.0]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.10...v2.3.0
[v2.2.1-fork.10]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.9...v2.2.1-fork.10
[v2.2.1-fork.9]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.8...v2.2.1-fork.9
[v2.2.1-fork.8]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.7...v2.2.1-fork.8
[v2.2.1-fork.7]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.6...v2.2.1-fork.7
[v2.2.1-fork.6]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.5...v2.2.1-fork.6
[v2.2.1-fork.5]: https://github.com/howmanyslop/sloptor/compare/v2.2.1-fork.4...v2.2.1-fork.5
[v2.2.1-fork.4]: https://github.com/howmanyslop/sloptor/releases/tag/v2.2.1-fork.4
