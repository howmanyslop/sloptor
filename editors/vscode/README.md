# sloptor for VS Code

Shows sloptor's transformer diagnostics in TypeScript files by running `sloptor lsp`. It also starts and stops the compiler, opens a file's compiled output and puts a color picker on `Color3` constructors.

The diagnostics are the ones VS Code's TypeScript server can't report: `noAny`, unsupported Luau constructs, macro failures, comment-directive refusals and internal compiler errors. VS Code's TypeScript server keeps reporting type errors. The extension runs beside it, not instead of it.

## Install

1. Install the extension `howmanysmall.sloptor` from the Marketplace.
2. Install sloptor. See [Install](https://docs.howmanysmall.com/sloptor/guide/install/).

The extension looks for the binary in this order:

1. The `sloptor.path` setting. A relative path is relative to the first workspace folder.
2. `node_modules/.bin/rotor`, which the npm package installs.
3. `sloptor`, then `rotor`, on your `PATH`.

The server only checks files in a roblox-ts project: the nearest `tsconfig.json` needs `node_modules/@rbxts` beside it or in a parent directory. See [Editors](https://docs.howmanysmall.com/sloptor/guide/editors/) for the full project rules.

## Commands

All commands are in the command palette under `sloptor`.

| Command                 | What it does                                                                     |
| ----------------------- | -------------------------------------------------------------------------------- |
| Open Output             | Opens the compiled `.luau` file for the active `.ts` or `.tsx` file.             |
| Start Compiler          | Runs `sloptor` with `sloptor.compiler.arguments` in the active file's workspace. |
| Stop Compiler           | Stops the compiler that Start Compiler started.                                  |
| Toggle Compiler         | Starts the compiler if it's stopped, and stops it if it's running.               |
| Restart Language Server | Restarts `sloptor lsp`.                                                          |

Open Output asks the language server for the path, so it finds the same project `sloptor build` would, solution configs in a monorepo included. The file is `.lua` instead of `.luau` when the project builds `.lua`. It opens beside the source unless `sloptor.openOutputToSide` is off. Open Output is also on the editor tab's context menu.

Start Compiler runs `sloptor build --watch` by default. For a monorepo with a solution `tsconfig.json`, set the arguments to build the references:

```json
{
	"sloptor.compiler.arguments": ["build", "--build", "--watch"]
}
```

The status bar has a button that starts and stops the compiler. Turn it off with `sloptor.statusBar.enabled`.

## Color picker

VS Code shows a color swatch on `new Color3(r, g, b)`, `Color3.fromRGB(r, g, b)` and `Color3.fromHSV(h, s, v)`. Click it to pick a color, and the extension rewrites the arguments. `Color3.fromRGB` always gets integers. `new Color3` and `Color3.fromHSV` get `sloptor.colorPicker.precision` decimal places.

## Settings

| Setting                         | Default              | Description                                                                         |
| ------------------------------- | -------------------- | ----------------------------------------------------------------------------------- |
| `sloptor.path`                  | `""`                 | Path to the `sloptor` executable. Empty uses the lookup in [Install](#install).     |
| `sloptor.typeErrors`            | `false`              | Also report TypeScript errors (`TS####`).                                           |
| `sloptor.compiler.arguments`    | `["build", "--watch"]` | Arguments for Start Compiler.                                                     |
| `sloptor.statusBar.enabled`     | `true`               | Show the compiler button in the status bar.                                         |
| `sloptor.colorPicker.enabled`   | `true`               | Show the color picker on `Color3` constructors.                                     |
| `sloptor.colorPicker.precision` | `2`                  | Decimal places, `0` to `6`, for `new Color3` and `Color3.fromHSV`.                  |
| `sloptor.openOutputToSide`      | `true`               | Open the compiled file beside the source instead of in its place.                   |
| `sloptor.trace.server`          | `"off"`              | Log protocol messages: `"off"`, `"messages"` or `"verbose"`.                        |

Leave `sloptor.typeErrors` off. VS Code's TypeScript server already reports those errors, so turning it on shows each one twice.

## Troubleshooting

The extension logs to the `sloptor` output channel. Open it with **Output: Show Output Channels** and pick `sloptor`. If the server doesn't start, check that the binary is somewhere the [lookup](#install) reaches, or set `sloptor.path`.

To see what VS Code and the server send each other, set `sloptor.trace.server` to `"messages"` or `"verbose"`. The trace goes to the same channel.

If no diagnostics show up, check that the file is in a roblox-ts project. A plain TypeScript project has no sloptor diagnostics by design.

## Releasing

For maintainers. From the repository root:

```sh
nr release vscode patch   # or minor, major
```

That runs bumpp on `editors/vscode/package.json`. It packages the extension first, so a broken build fails before anything is pushed. Then it commits, tags `vscode-vX.Y.Z` and pushes. The Marketplace only takes plain `major.minor.patch` versions, so pre-release types are refused.

The tag starts `.github/workflows/release-vscode.yaml`, which publishes through Marketplace Trusted Publishing. There is no token to store. The Marketplace trusts that workflow by file name, so don't rename it.
