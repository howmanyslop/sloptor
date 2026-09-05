# tsgo internal API inventory

This is the direct boundary between non-mirrored code and the upstream
`internal/` packages. Review this list before changing the pinned source ref.
It is deliberately a package inventory: package moves and deleted exports are
the first breakages a re-mirror exposes.

## Direct imports

```text
ast
bundled
checker
compiler
core
diagnostics
diagnosticwriter
fswatch
jsnum
json
ls/lsutil
module
nodebuilder
outputpaths
parser
printer
scanner
sourcemap
stringutil
tsoptions
tspath
vfs
vfs/cachedvfs
vfs/osvfs
vfs/vfstest
vfs/wrapvfs
```

## Named compatibility watchpoints

These are direct uses implicated by the upstream API rename report in issue #43.
They have 17 current call sites across the flamework and transformer
packages, so a port needs an explicit compatibility decision rather than a
blind rename.

```text
ast.NewDiagnosticWithStringCode
ast.GetImplementsTypeNodes
ast.GetExtendsHeritageClauseElement
compiler.EmitResolver.IsReferencedAliasDeclarationUnsafe
compiler.EmitResolver.GetJsxFactoryEntityUnsafe
compiler.EmitResolver.GetJsxFragmentFactoryEntityUnsafe
```
