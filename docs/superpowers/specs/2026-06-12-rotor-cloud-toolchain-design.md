# rotor cloud toolchain — assets, deploy, TS config, scaffolding

**Date:** 2026-06-12 · **Status:** approved direction (user) — assets + deploy in parallel, deploy is full IaC architecture, TypeScript config for everything.

Expands rotor from compiler + Luau toolchain into the all-in-one Roblox toolchain the wordmark promises: asset management (asphalt-style), declarative deployment (mantle-style), a typed TypeScript config file, and project scaffolding. All native Go; Open Cloud only (no cookie auth in v1).

## Decisions (from user)

1. **Assets and deploy built in parallel** — two sub-projects on shared foundations.
2. **Deploy is full mantle-style IaC** — resource graph + state + plan/apply diffing, not a one-shot publisher. v1 ships the engine with a core resource set; more resource types are roadmap items, not architecture changes.
3. **TypeScript configures everything** — one `rotor.config.ts` at the project root configures assets, deploy environments, and future tools. No TOML/YAML.
4. The existing compiler and Luau toolchain are untouched. Byte-parity remains law for the compiler.

## Architecture

```text
rotor.config.ts ──(internal/config: esbuild transpile → goja eval)──► Config struct
                                                                        │
        ┌──────────────────────────┬────────────────────────────────────┤
        ▼                          ▼                                    ▼
  internal/assets            internal/deploy                     cmd/rotor
  sync · lockfile ·          resource graph · state ·            asset / deploy /
  codegen (.luau + .d.ts)    plan/apply · diff                   init / sourcemap
        │                          │
        └───────────┬──────────────┘
                    ▼
             internal/cloud
   Open Cloud REST client: auth (ROBLOX_API_KEY),
   rate limit, retry, operations polling, typed endpoints
```

### `internal/cloud` — Open Cloud client (shared foundation)

- Auth: `ROBLOX_API_KEY` env var (CI-friendly), overridable per-call site from config. Never written to disk by rotor.
- `Client` with: rate limiting (token bucket per host), retries with backoff on 429/5xx honoring `Retry-After`, long-running **operations polling** (`/v1/operations/{id}`) for asset uploads.
- Typed endpoint wrappers used by assets + deploy: assets (create/update asset, get operation), universes (get/update), places (get/update config, **publish place file** via `universes/{u}/places/{p}/versions`), badges, game passes, icons/thumbnails.
- All HTTP behind an interface so assets/deploy tests run against `httptest` fakes; no network in tests.

### `internal/config` — `rotor.config.ts` runtime

- Loader: transpile TS → JS with **esbuild** (Go API, transpile-only), evaluate in **goja** (pure-Go JS engine). No Node required for config.
- v1 constraint: the config file is self-contained — relative imports of other project `.ts` config files are bundled by esbuild; npm imports are not supported (clear error).
- The `"rotor/config"` module is **virtual**: the loader resolves it in-memory (esbuild plugin) to `export const defineConfig = (c) => c`; editor typing comes from a generated `rotor-config.d.ts` with `declare module "rotor/config"` (written by `rotor init`, refreshed by any command when stale).
- Shape (all sections optional):

```ts
import { defineConfig } from "rotor/config";

export default defineConfig({
  assets: {
    paths: ["assets/**/*.png", "assets/**/*.ogg"],
    output: { luau: "src/shared/assets.luau", types: "src/shared/assets.d.ts" },
    creator: { type: "group", id: 12345 },
  },
  deploy: {
    environments: {
      dev:  { universeId: 111, places: { start: { file: "build/game.rbxl", placeId: 222 } }, payments: "free" },
      prod: { universeId: 333, places: { start: { file: "build/game.rbxl", placeId: 444 } },
              experience: { name: "My Game", playability: "public" },
              badges: { winner: { name: "Winner!", description: "...", icon: "assets/badge.png" } } },
    },
  },
});
```

- Config validation produces rotor-style diagnostics with the config path; unknown keys warn (forward compat).

### `internal/assets` — `rotor asset sync` (asphalt-style)

- Scan config globs → content-hash each file (BLAKE2/SHA-256).
- Lockfile `rotor-lock.json` (committed): `{ "assets/logo.png": { "hash": "...", "assetId": 123 } }` — unchanged hashes never re-upload.
- Upload changed/new via Open Cloud assets API (decals for png/jpg/tga/bmp, audio for ogg/mp3), poll the operation for the final asset id. Audio/image moderation failures surface per-file, don't abort the batch.
- Codegen, nested by directory, both targets written atomically:
  - `assets.luau`: `return { logo = "rbxassetid://123", sounds = { hit = "rbxassetid://456" } }`
  - `assets.d.ts`: matching `declare const assets: {...}; export = assets` so rbxts code gets typed `assets.sounds.hit`.
- CLI: `rotor asset sync [--dry-run]` (plan without uploading), `rotor asset list` (lockfile view). Follow-ups (roadmap, not v1): alpha-bleed image processing, animations (needs cookie auth), `--target studio` local sync.

### `internal/deploy` — `rotor deploy` (mantle-style IaC)

The engine is the deliverable; resource types plug into it.

- **Resource model**: `type Resource interface { Kind() string; ID() string; Inputs() any }` plus per-kind `Provider` with `Create/Update/Delete(ctx, client, inputs, priorOutputs)`. Inputs are hashed (canonical JSON) for drift detection; outputs (ids, versions) persist to state.
- **State**: `.rotor/deploy/<environment>.json` — resource kind+id → input hash + outputs. Local file v1 (commit it or CI-cache it); remote state backends are a follow-up.
- **Plan/apply**: `rotor deploy plan -e prod` prints create/update/delete/no-op per resource (terraform-style, colored); `rotor deploy apply -e prod` executes in dependency order (places before badges that reference them, assets before anything referencing uploaded icons), `--yes` for CI. Deletes only happen for resources removed from config AND present in state (with `--allow-deletes` guard).
- **v1 resource kinds**: place file publish (versioned, `saved` vs `published`), place configuration (name/description/maxPlayers), experience/universe configuration (name/description/playability/payments), badge, game pass, experience icon + thumbnails (paths can reference `assets:` outputs).
- **v1.1 expansion (shipped 2026-06-12)** — mantle-parity coverage on top of the v1 engine, no architecture changes: `game_pass` (`gamepasses:` config, icon files become dependent `asset` resources, deduped with badge icons; `price` omitted = not for sale), `experience_icon` (`icon:`, content-hashed upload), `experience_thumbnails` (`thumbnails:`, ONE resource over the ordered set; v1 semantics are full-replace on any change with per-thumbnail ids kept in outputs so stale ones are deleted), `developer_product` (`products:`), `social_link` (`socials:`, typed enum incl. github), plus `place_config` now fed from `places.<name>.name/description/maxPlayers`, `versionType` wired from config into place publishing, and `experience.privateServers.price` → v2 `privateServerPriceRobux`. Icon/thumbnail/dev-product/social-link endpoints are PATH CHOICE consts (unverified legacy proxies, one-line fixes). `genre`/`ageRating` deliberately skipped: not writable on the cloud/v2 Universe resource.
- Follow-ups (roadmap): remote state, imports of existing resources, `deploy destroy`.

### `cmd/rotor` — new commands

- `rotor asset sync|list`
- `rotor deploy plan|apply -e <env>`
- `rotor init [game|package|plain]` — scaffold: `package.json` (+ @rbxts deps), `tsconfig.json`, `default.project.json`, `rotor.config.ts` + `rotor-config.d.ts`, `src/` starter, `.gitignore`. `plain` = Luau-only project (no rbxts) for bundle/minify/pack users.
- `rotor sourcemap [-o out]` — Rojo-compatible `sourcemap.json` from the project graph (reuses `internal/pack`'s native tree builder) for luau-lsp.

## Error handling

- No API key → one clear error naming `ROBLOX_API_KEY` and the Creator Dashboard URL to mint one (with required scopes per command).
- Open Cloud failures: per-resource/per-asset errors with the API's message; partial progress persists (lockfile/state written after each success) so re-runs resume.
- Config eval errors: file:line from goja mapped through esbuild's sourcemap.

## Testing

- `internal/cloud`: unit tests against `httptest` servers (auth header, retry/backoff, operation polling).
- `internal/config`: golden configs (valid, invalid, npm-import error, relative import) — pure Go tests, no Node.
- `internal/assets`: fake cloud client — sync plan correctness (new/changed/unchanged), lockfile round-trip, codegen goldens for `.luau` + `.d.ts` (the `.luau` golden must parse with `internal/luau/cst`).
- `internal/deploy`: fake client — plan diffing (create/update/noop/delete), state round-trip, dependency ordering, `--allow-deletes` guard.
- CLI: arg-parsing tests per command, mirroring existing `cmd/rotor` test style.
- Real-network smoke (env-gated like the `randomness` acceptance): `ROTOR_CLOUD_SMOKE=1` + a real key uploads one 1×1 png to a test universe.

## Non-goals (v1)

- Cookie-authenticated APIs (animations upload, some legacy settings) — Open Cloud only.
- Remote/state-locking backends, team state sharing.
- Full mantle resource-type parity day one — the engine is built for it; types land incrementally.
- npm imports in `rotor.config.ts`.
