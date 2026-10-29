# sloptor for Zed

Shows sloptor's transformer diagnostics in TypeScript files by running `sloptor lsp`.

## Install

1. Put `sloptor` on your `PATH`.
2. In Zed, open the command palette and run `zed: install dev extension`.
3. Select this directory, `editors/zed`.

Zed builds the extension itself, so it needs a Rust toolchain installed through `rustup`.

## Settings

All optional, in Zed's `settings.json`:

```json
{
	"lsp": {
		"sloptor": {
			"binary": { "path": "/path/to/sloptor" },
			"initialization_options": { "typeErrors": true }
		}
	}
}
```

`typeErrors` also shows TypeScript errors. Leave it off while Zed's own TypeScript server is running, or they show up twice.
