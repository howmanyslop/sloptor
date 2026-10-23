# Mirror of microsoft/typescript-go internals

- Source: https://github.com/microsoft/typescript-go
- Commit: 1f955e97ae01ad4f47b6a3b0bf5942b330ae6ef3
- Changes: files copied from internal/ with import paths rewritten
  ("github.com/microsoft/typescript-go/internal/" -> "rotor/tsgo/"); *_test.go files and testdata/ directories omitted.
  Unused upstream api, execute, format, fourslash, pprof, project,
  testrunner, and testutil trees are omitted. Shared ls/lsp dependencies stay.
  The unused ls/lsp server entry points, ls/autoimport, and ls/change are
  omitted too; ls/lsutil and lsp/lsproto remain compiler dependencies.
  Rotor-owned edits are re-applied from tools/mirror/patches after
  import rewriting. Overlay shims from tools/mirror/overlay
  (e.g. checker/rotor_exports.go) are applied automatically.
  Regenerate a pinned ref with:
  go run ./tools/mirror -ref <sha>
