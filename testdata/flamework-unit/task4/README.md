# Flamework v1.3.2 Task 4 oracle corpus

This corpus records observable output from the pinned upstream
`rbxts-transformer-flamework@1.3.2`. The valid oracle fixes `salt`, `hashPrefix`,
`idGenerationMode`, package name, source paths, and the guard-dedup threshold.
`deterministic-random.cjs` fixes both upstream UUID call sites to the documented
UUID `00000000-0000-4000-8000-000000000004`; load it through `NODE_OPTIONS`
for every oracle command. Obfuscation remains disabled, so upstream's
`Math.random` shuffle is not invoked.

`inventory.tsv` accounts for every pinned upstream `src` entry. Empty
`.gitkeep` is recorded as a non-behavior sentinel; declaration files are API
metadata inputs rather than executable transforms. `expected/` contains
upstream transformed TypeScript, final roblox-ts Luau, diagnostics, and emitted
Flamework metadata. Paths inside expected artifacts are fixture-relative.

Refresh from `oracle/` with these exact upstream commands:

```sh
npm install --ignore-scripts --package-lock=false --no-audit --no-fund
NODE_OPTIONS=--require=./deterministic-random.cjs ./node_modules/.bin/rbxtsc --project tsconfig.json
NODE_OPTIONS=--require=./deterministic-random.cjs ./node_modules/.bin/rbxtsc --project tsconfig.invalid-path.json
NODE_OPTIONS=--require=./deterministic-random.cjs ./node_modules/.bin/rbxtsc --project tsconfig.invalid-template.json
```

The invalid commands must exit 1 with the records in
`expected/diagnostics.jsonl`. The valid command must exit 0. Capture transformed
TypeScript with the same preload and TypeScript program/config, then compare
final Luau and metadata to `expected/`. Normalize only a terminal newline when
comparing JSON because upstream writes JSON without one. Never regenerate
expected output from Rotor's native implementation.
