<h1 align="center">sloptor</h1>

<p align="center"><em>TypeScript to Luau, at native speed.</em></p>

<p align="center">
  <a href="https://github.com/howmanyslop/sloptor/releases/latest"><img src="https://img.shields.io/github/v/release/howmanyslop/sloptor" alt="latest release"></a>
  <a href="https://github.com/howmanyslop/sloptor/actions/workflows/ci.yaml"><img src="https://github.com/howmanyslop/sloptor/actions/workflows/ci.yaml/badge.svg" alt="ci"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
</p>

sloptor is a native TypeScript-to-Luau compiler for Roblox, written in Go. It ports [roblox-ts](https://roblox-ts.com) onto [typescript-go](https://github.com/microsoft/typescript-go), with upstream-parity output and verified fork behavior.

📖 **[Documentation](https://docs.howmanysmall.com/sloptor/)** · 🤝 [Contributing](CONTRIBUTING.md) · 🗺️ [Roadmap](https://docs.howmanysmall.com/sloptor/roadmap/) · 📝 [Changelog](CHANGELOG.md)

## Install

```sh
mise use -g github:howmanyslop/sloptor@2.8.3   # or rokit, aftman, foreman
pnpm add -D @rotor-rbx/rotor                    # or npm, bun, yarn
```

Release archives, source builds, and every other option: [Install](https://docs.howmanysmall.com/sloptor/guide/install/).

## What it does

```sh
sloptor check ./my-game -w     # native TypeScript checking in watch mode
sloptor build ./my-game -w     # compile to Luau with incremental rebuilds
```

- **Compile and check**: compile TypeScript and TSX to Luau using your existing `tsconfig.json`, `@rbxts/*` packages, and supported transformer plugins. [Migrate from rbxtsc](https://docs.howmanysmall.com/sloptor/guide/migrate/).
- **Incremental builds**: watch source files, rebuild affected outputs, and build project references in dependency order.
- **Compiler outputs**: emit Luau, TypeScript declarations, and source maps. Integrate builds through the typed JavaScript `createBuildSession()` API.
- **Compiler extensions**: native Flamework and JECS transforms, plus compile-time [macros](https://docs.howmanysmall.com/sloptor/macros/) such as `$env`, `$file`, and `$keys`.

The public commands are `build`, `check`, `diagnostics`, `clean`, `schema`, `completion`, and `version`. Every command and flag: [Commands](https://docs.howmanysmall.com/sloptor/commands/).

## Benchmarks

Measured on real production rbxts games, with output byte-identical to `rbxtsc` 3.0.0 on the unaffected benchmark surfaces:

| Workload | sloptor |
|----------|------:|
| Full strict typecheck — 222-file production game | **161 ms** |
| Full build — 95-file production game | **355 ms** |
| Incremental watch rebuild — same game | **180 ms** |

The JS toolchain spends longer than this booting Node. The ~10× speedup is structural: sloptor runs Microsoft's native, parallel TypeScript compiler instead of the single-threaded JS one. Tune it with `--checkers` and `--builders` ([Building](https://docs.howmanysmall.com/sloptor/guide/building/)).

## Contributors

<a href="https://github.com/uproot"><img src="https://github.com/uproot.png" width="56" height="56" alt="uproot"></a>
<a href="https://github.com/Coyenn"><img src="https://github.com/Coyenn.png" width="56" height="56" alt="Coyenn"></a>

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE). sloptor stands on [roblox-ts](https://github.com/roblox-ts/roblox-ts) (MIT) and [typescript-go](https://github.com/microsoft/typescript-go) (Apache-2.0) — see [credits](https://docs.howmanysmall.com/sloptor/architecture/#credits-and-licenses).
