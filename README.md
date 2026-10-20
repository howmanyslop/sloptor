<p align="center">
  <img src="media/logo.png" alt="sloptor — an all in one roblox toolchain" width="480">
</p>

<p align="center"><em>TypeScript in, Roblox out — at native speed.</em></p>

<p align="center">
  <a href="https://github.com/howmanyslop/sloptor/releases/latest"><img src="https://img.shields.io/github/v/release/howmanyslop/sloptor" alt="latest release"></a>
  <a href="https://github.com/howmanyslop/sloptor/actions/workflows/ci.yml"><img src="https://github.com/howmanyslop/sloptor/actions/workflows/ci.yml/badge.svg" alt="ci"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
</p>

sloptor is an all-in-one Roblox toolchain, written in Go. At its core is a native rewrite of the [roblox-ts](https://roblox-ts.com) compiler built on [typescript-go](https://github.com/microsoft/typescript-go): a drop-in `rbxtsc` replacement with upstream-parity Luau output, plus a native Luau toolchain and Open Cloud asset and deployment pipelines, all in one binary.

📖 **[Documentation](https://docs.howmanysmall.com/sloptor/)** · 🤝 [Contributing](CONTRIBUTING.md) · 🗺️ [Roadmap](https://docs.howmanysmall.com/sloptor/roadmap/) · 📝 [Changelog](CHANGELOG.md)

## Install

```sh
mise use -g github:howmanyslop/sloptor@2.8.3   # or rokit, aftman, foreman
pnpm add -D @rotor-rbx/rotor                    # or npm, bun, yarn
```

Release archives, source builds, and every other option: [Install](https://docs.howmanysmall.com/sloptor/guide/install/).

## What it does

```sh
sloptor check ./my-game -w     # native full-strictness typecheck: 222 files in 161 ms
sloptor build ./my-game -w     # parity Luau, watch mode, incremental rebuilds
sloptor dev                    # watch + incremental compile + rojo serve to Studio
sloptor bundle entry.luau -o bundle.luau --minify
sloptor asset sync             # upload new and changed assets, typed codegen
sloptor deploy plan -e prod    # diff config against live state
```

- **Compile and check**: the same `tsconfig.json`, `@rbxts/*` packages, and transformer plugins as `rbxtsc`, plus compile-time [macros](https://docs.howmanysmall.com/sloptor/macros/) like `$env`, `$asset`, and `$keys`. [Migrate from rbxtsc](https://docs.howmanysmall.com/sloptor/guide/migrate/).
- **Luau toolchain**: `dev`, `bundle`, `minify`, `pack`, and `sourcemap` work on any Rojo project.
- **Cloud**: assets and deployment from one typed [`rotor.toml`](https://docs.howmanysmall.com/sloptor/configuration/rotor-toml/).

Every command and flag: [Commands](https://docs.howmanysmall.com/sloptor/commands/).

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
