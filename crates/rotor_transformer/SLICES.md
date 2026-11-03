# Where each Go file lives

Stage 4b is complete: every file of `internal/transformer` is ported, one Rust file per Go file.
Stage 4a ported the core at the top level of `src/`; Stage 4b ported the rest into six module
directories. Each `mod.rs` holds the entry points other modules call and delegates to its files.

## Core (`src/`)

`access.go`, `assignment.go`, `binary.go`, `bitwise.go`, `call.go`, `conditional.go`, `delete.go`,
`diagnostics.go`, `diagservice.go`, `dispatch.go`, `ensureorder.go`, `exports.go`, `identifier.go`,
`ismethod.go`, `literals.go`, `logical.go`, `loops.go`, `loops_numeric.go`, `macromanager.go`,
`methodassignment.go`, `newexpression.go`, `nodemodulespath.go`, `optionalchain.go`, `pointer.go`,
`receivertype.go`, `refwalker.go`, `runtimelib.go`, `selectsymlinkpath.go`, `sourcefile.go`,
`sourcepositions.go`, `spreadhelpers.go`, `state.go`, `statementlist.go`, `statements.go`,
`truthiness.go`, `types.go` (the predicates live in `rotor_tsrs::types`), `unary.go`,
`valuetoidstr.go` and `voidexpr.go`, each as the same-named module. `skipupwards.go` is
`rotor_tsrs::ast::skip_upwards` and `skip_downwards`.

Helpers several modules call live in core rather than with their Go file:

- `crate::shared`: `isOmittedBindingElement` (`binding.go`), `getExtendsNode`, `findConstructor`,
  `createClassNameFunction` and `isClassHoisted` (`classes.go`), `needsInverseEntry` (`enums.go`),
  `transformInitializer` (`parameters.go`), `registerOptimizableVarArgsForFunction`
  (`varargsoptimization.go`), `thisParameter`, `typeGenericNames` and `luauTypedSignatures`
  (`luautypes_emit.go`).
- `crate::state`: the array append plan types and `State::get_array_append_target`.
- `crate::methodassignment`: `getAllSuperTypeNodes` and `validateHeritageClause` (`classes.go`).
- `crate::extras` (`mod.rs`): `isReturnBlockedByTryStatement` and `isBreakBlockedByTryStatement`
  (`trystatement.go`).
- `crate::types`: `symbolIsAmbient`, `isUndefinedTypeCheck`, `getRecursiveBaseTypes` and
  `IsSharedTableType`.

## Modules

| Module | Go files |
| --- | --- |
| `expressions` | `binding.go`, `bindingarray.go`, `bindingiterator.go`, `bindingobject.go`, `logicalassignment.go`, `spread.go`, `spreaddestruct.go` |
| `control` | `functions.go`, `parameters.go`, `namedfunction.go`, `varargsoptimization.go`, `switch.go`, `forof.go`, `arrayappendoptimization.go` |
| `classes` | `classes.go`, `enums.go`, `namespaces.go` |
| `macros` | `callmacros.go`, `constructormacros.go`, `propertycallmacros.go`, `stringmacros.go`, `arraymacros.go`, `arraymacros2.go`, `collectionmacros.go`, `envmacro.go`, `nameofmacro.go`, `keysmacro.go`, `filemacro.go` |
| `luau_types` | `luautypes.go`, `luautypes_alias.go`, `luautypes_builder.go`, `luautypes_class.go`, `luautypes_emit.go`, `luautypes_typefunction.go`, `luautypes_typefunction_prelude.go`, `nativetypes.go`, `nativetypes_alias.go`, `nativetypes_policy.go`, `nativetypes_spelling.go` |
| `extras` | `jsx.go`, `jsxtext.go`, `async.go` (as `async_`), `decorators.go`, `trystatement.go`, `labeledstatement.go`, `imports.go`, `importexpr.go`, `sourcemap.go` |

The macro registration tables in `macros/mod.rs` keep Go's table order, because `macromanager`
iterates them. `sourcemap.go`'s `RenderWithSourceMap` is `State::render_with_source_map`.

Go tests are ported with their expected strings verbatim: the core's in `src/tests/`, each module's
in `src/<module>/tests.rs` or `src/<module>/tests/`.

## Fixture gates

- **diff**: `tests/diff.rs` compiles each fixture of `testdata/diff/golden` listed in
  `tests/fixtures/*.txt` and requires byte equality. The lists cover every fixture except the rows
  the divergence ledger marks upstream-corrected (`26_stringmacros`, `29_iteration`,
  `31_mixed3b`), which `tests/diff.rs` refuses; their behavioral checks are unit tests.
- **conformance** (`testdata/conformance`) and the Flamework sidecar projects
  (`testdata/transformers`) are rotor_compile gates.

## Determinism limits

- The test harness runs one checker. Union-typed macro selection and the pass-wide dedupe caches
  (`IsReportedBy*`) depend on checker history and checker count, so no fixture may rely on them.
  Stage 5 must group checkers the way Go does.
- Go's select-symlink path is nondeterministic (`SyncSet.ToSlice`). Do not add fixtures that depend
  on which symlink it picks.

## Internal errors

Go panics are `state.ice(...)` plus a placeholder. The harness matches Go's `compile.go` recover: a
file with an internal error gets no text and no diagnostics. rotor_compile must do the same. If it
can, it should also stop writing the pass-wide caches in `MultiState` after an internal error. A
`MultiState` belongs to one checker: `State::new` records the first checker's id and reports an
internal error for a state built on another checker.
