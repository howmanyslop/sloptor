# Pinned compatibility fixtures

Runtime fixtures in `runtime/` are TestEZ modules compiled by both Rotor and
roblox-ts `3.0.0-dev-3106b14` (commit
`3106b1492a2cf5b5b06f73354e94b03e3eb45b4e`). The conformance harness builds a
fresh Roblox place for each compiler and executes it with Lune.

Diagnostic fixtures in `diagnostics/` use the expected Rotor diagnostic ID as
the filename prefix, followed by an optional numeric case suffix. The pinned
compiler must also reject each fixture.

`TestPinnedCompatibility` requires Node, npm, Rojo, and Lune. It installs the
exact upstream npm package and `@rbxts/types` version into the user cache,
verifies the registry `gitHead`, and rejects a compiler with the wrong version.
Set `ROTOR_PINNED_RBXTS_CLI` to an already-installed `cli.js` to use an explicit
local copy; the version check still runs.

Run the complete pinned surface with:

```sh
go test ./internal/conformance -run '^(TestPinnedCompatibility|TestPinnedCompatibilityDiagnostics|TestPinnedPackageInterop)$' -count=1 -v
```

The package test compiles `@rbxts/compat-library` and its declaration file with
each compiler, compiles a consumer with the other compiler, then executes both
mixed output trees through Rojo and Lune.
