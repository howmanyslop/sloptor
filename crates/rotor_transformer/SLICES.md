# Stage 4b slices

Stage 4a ported the transformer core. Every remaining file in `internal/transformer` belongs to
exactly one of the six slices below. Each slice owns one module directory under `src/`. The
directory already holds a stub `mod.rs` with the signatures that core and the other slices call.
The slice replaces the stub bodies and adds one Rust file per Go file under its own directory.

## Rules every slice follows

- Edit only two things: your `src/<slice>/` directory and your new
  `tests/fixtures/slice_<slice>.txt`. Put your unit tests in your directory too, as
  `src/<slice>/tests.rs` (or `src/<slice>/tests/`), declared from your `mod.rs` with
  `#[cfg(test)] mod tests;`. The shared helpers in `crate::tests` (`with_state`, `with_source`,
  `render_list`, and the rest) are callable from there.
- Keep the signature of every stub in your `mod.rs`. Other slices call them.
- Per-file state goes in your slice's `SliceState`, which `State::slices` holds. Program-wide caches
  go in your `MultiSliceState`, which `MultiState::slices` holds. Do not add fields to `State` or
  `MultiState`.
- Dead code is covered by one crate-level `#![expect(dead_code)]` in `lib.rs`, removed at the 4b
  merge. Do not add per-item `dead_code` expectations.
- When you call another slice's stub, leave it alone. It reports `rotorNotYetPorted` until that
  slice merges, so keep the fixtures that need it out of your list.
- Go panics become `state.ice(...)` plus a placeholder such as `state.ast.none()`. Never panic.
- The emit resolver queries that `imports.go` and `jsx.go` make through `s.EmitResolver()` are
  checker methods in Rust (`state.ck().get_jsx_factory_entity(...)` and friends). No core edit is
  needed for them.

## Calls that cross slices

Helpers that several slices call are ported in core already. Their owning slice does not port them
again:

- `crate::shared`: `isOmittedBindingElement` (`binding.go`), `getExtendsNode`, `findConstructor`,
  `createClassNameFunction` and `isClassHoisted` (`classes.go`), `needsInverseEntry` (`enums.go`),
  `transformInitializer` (`parameters.go`), `registerOptimizableVarArgsForFunction`
  (`varargsoptimization.go`), `thisParameter`, `typeGenericNames` and `luauTypedSignatures`
  (`luautypes_emit.go`).
- `crate::state`: the array append plan types and `State::get_array_append_target` (Go keeps
  `getArrayAppendTarget` in `state.go`).
- `crate::methodassignment`: `getAllSuperTypeNodes` and `validateHeritageClause` (`classes.go`).
- `crate::extras` (ported bodies): `isReturnBlockedByTryStatement` and
  `isBreakBlockedByTryStatement` (`trystatement.go`).
- `crate::types`: `symbolIsAmbient`, `isUndefinedTypeCheck`, `getRecursiveBaseTypes` and
  `IsSharedTableType`. The `Is*Type` checks are `rotor_tsrs::types::TypeCheck` variants.

Everything else one slice calls in another is a stub in the owner's `mod.rs`. The caller waits for
the owner's wave before it lists a fixture that reaches the stub:

| Caller | Owner | Stubs |
| --- | --- | --- |
| i, ii, vi | i | `transform_binding_name`, the binding pattern transforms, the spread builders |
| iii | ii | `transform_parameters`, `transform_method_declaration_with_receiver` |
| ii, iii | v | `push_type_generics`, `pop_type_generics` (ported), `signature_generics`, `signature_return_type`, `signature_var_args_type`, `get_luau_vararg_type`, `get_luau_parameter_type`, `get_luau_binding_element_type`, `get_luau_self_type`, `get_native_parameter_type`, `luau_default_refinement`, `luau_enum_binding`, `luau_enum_table_cast`, `LuauClass` and `new_luau_class` |
| ii | vi | `wrap_statements_as_generator` |
| iii | vi | `transform_decorators` (no statements and no diagnostic for a class without decorators) |
| iv | vi | `get_module_tree_import_parts` |
| v | iii | `transform_property_initializers` |

With `luauTypes` and `nativeTypes` off, the slice v stubs return what Go returns with the features
off. Slices ii and iii can land before v without changing any default output.

## Already ported in Stage 4a (core)

`access.go`, `assignment.go`, `binary.go`, `bitwise.go`, `call.go`, `conditional.go`, `delete.go`,
`diagnostics.go`, `diagservice.go`, `dispatch.go`, `ensureorder.go`, `exports.go`, `identifier.go`,
`ismethod.go`, `literals.go`, `logical.go`, `loops.go`, `loops_numeric.go`, `macromanager.go`,
`methodassignment.go`, `newexpression.go`, `nodemodulespath.go`, `optionalchain.go`, `pointer.go`,
`receivertype.go`, `refwalker.go`, `runtimelib.go`, `selectsymlinkpath.go`, `skipupwards.go` (as
`rotor_tsrs::ast::skip_upwards` and `skip_downwards`), `sourcefile.go`, `sourcepositions.go`,
`spreadhelpers.go`, `state.go`, `statementlist.go`, `statements.go`, `truthiness.go`, `types.go`
(the predicates live in `rotor_tsrs::types`), `unary.go`, `valuetoidstr.go`, `voidexpr.go`. The
gate is `tests/fixtures/core.txt` (diff 01 to 13).

Go tests ported to `src/tests/` in Stage 4a: `state_test`, `sourcefile_test`, `diagnostics_test`,
`statements_test`, `statementlist_test`, `literals_test`, `refwalker_test`, `types_test`,
`conditional_test`, `delete_test`, `voidexpr_test`, `template_test`, `operators_test`,
`runtimelib_test`, `nodemodulespath_test`, `selectsymlinkpath_test` and `spreadhelpers_test`.
Stage 4a also ported the parts of `bitwise_test`, `call_test`, `optionalchain_test`, `loops_test`,
`identifier_test` and `sharedtable_test` that need no slice. The sections below assign the rest.

## Fixture gates

- **diff**: `testdata/diff/golden/<name>.luau`, listed in `tests/fixtures/slice_<slice>.txt` and run
  by `tests/diff.rs`. A slice lists a fixture once every construct in it is ported. "Needs" names
  the other slices a fixture waits on. `tests/diff.rs` refuses the rows that the divergence ledger
  marks upstream-corrected (`26_stringmacros`, `29_iteration`, `31_mixed3b`). Their owner ports the
  Go behavioral check instead (see `testdata/forkparity/divergence-ledger.json`).
- **transformer testdata**: `internal/transformer/testdata/<dir>`, used by the Go unit tests that
  the slice ports with their expected strings verbatim.
- **conformance**: `testdata/conformance/golden/tests/<spec>.luau`. Every spec wraps its body in
  `export = () => { ... }` and imports helpers. Each spec therefore needs slice ii and slice vi
  (imports). The owner listed below is the slice whose constructs the spec exercises. These specs
  run at the rotor_compile stage. A slice may add them to its list once the stand-in harness can
  compile the conformance project.
- **testdata/transformers** holds the Flamework sidecar projects, a rotor_compile gate. Its
  transformer coverage is decorators and imports (slice vi).

## Determinism limits

- The test harness runs one checker. Union-typed macro selection and the pass-wide dedupe caches
  (`IsReportedBy*`) depend on checker history and checker count, so no fixture may rely on them.
  Stage 5 must group checkers the way Go does.
- Go's select-symlink path is nondeterministic (`SyncSet.ToSlice`). Do not add fixtures that depend
  on which symlink it picks.

## Internal errors

The harness matches Go's `compile.go` recover: a file with an internal error gets no text and no
diagnostics. rotor_compile must do the same. If it can, it should also stop writing the pass-wide
caches in `MultiState` after an internal error. A `MultiState` belongs to one checker:
`State::new` records the first checker's id and reports an internal error for a state built on
another checker.

## Slice i: expressions and operators (`src/expressions/`)

| Go file | Rust module |
| --- | --- |
| `binding.go` | `expressions::binding` |
| `bindingarray.go` | `expressions::bindingarray` |
| `bindingiterator.go` | `expressions::bindingiterator` |
| `bindingobject.go` | `expressions::bindingobject` |
| `logicalassignment.go` | `expressions::logicalassignment` |
| `spread.go` | `expressions::spread` |
| `spreaddestruct.go` | `expressions::spreaddestruct` |

- diff: `16_destructuring`, and `35_spread` (needs ii).
- Go tests: `binding_test`, `spread_test`, `spreaddestruct_test`, `nonprimitive_spread_test`, and
  `TestBigIntPropertyDiagnostic` from `bitwise_test` (`objectAccessor` is `binding.go`).
- transformer testdata: `destructuring`, `spread`.
- conformance: `destructure.spec`, `object.spec`, `array.spec`.

## Slice ii: statements and control flow (`src/control/`)

| Go file | Rust module |
| --- | --- |
| `functions.go` | `control::functions` |
| `parameters.go` | `control::parameters` |
| `namedfunction.go` | `control::namedfunction` |
| `varargsoptimization.go` | `control::varargsoptimization` |
| `switch.go` | `control::switch` |
| `forof.go` | `control::forof` |
| `arrayappendoptimization.go` | `control::arrayappendoptimization` |

Land this slice first: function declarations and expressions are the first unported construct in
most later fixtures.

- diff: `14_functions`, `15_arrows`, `17_forof`, `18_switch`, `19_closures`, `20_mixed2b`. The
  ledger marks `29_iteration` upstream-corrected, so it gets a behavioral port, not a golden row.
- Go tests: `functions_test`, `namedfunction_test`, `varargsoptimization_test`, `switch_test`,
  `forof_test`, `forof_variadic_test`, `arrayappendoptimization_test`, `ensureorder_storage_test`
  (uses `buildVarArgsOptimizationState`), the `TransformStatementList` half of
  `sharedtable_test`, the closure cases of `loops_test` and the hoist cases of `identifier_test`
  that Stage 4a skipped (they compile function bodies).
- transformer testdata: `functions`, `forof`, `switch`, `arrayappendoptimization`, `hoist`,
  `casehoist`, and the closure files in `control`.
- conformance: `function.spec`, `loop.spec`, `switch.spec`, `hoist.spec`, `tuple.spec`, and the
  core-construct specs that only need the function wrapper: `assignment`, `binary`, `bitwise`,
  `delete`, `exportLet`, `if`, `literal`, `optional`, `template`, `ternary`, `truthiness`, `type`,
  `typeof`.

## Slice iii: classes, enums and namespaces (`src/classes/`)

| Go file | Rust module |
| --- | --- |
| `classes.go` | `classes::classes` (minus the helpers already in core) |
| `enums.go` | `classes::enums` |
| `namespaces.go` | `classes::namespaces` |

- diff: `33_classes`, `38_enums_namespaces` (both need ii).
- Go tests: `classes_test`, `enums_test`, `namespaces_test`.
- transformer testdata: `classes` (except `decorators.ts`, slice vi), `enums`, `namespaces`.
- conformance: `class.spec`, `enum.spec`, `namespace.spec`, `instanceof.spec`.
- TS 7.1 parses `implements` heritage elements as `TypeReference` (see the tsrs ORACLE note
  on `HeritageTypeReference`). `validate_heritage_clause` already handles this. Keep it that way.

## Slice iv: macros (`src/macros/`)

| Go file | Rust module |
| --- | --- |
| `callmacros.go` | `macros::callmacros` |
| `constructormacros.go` | `macros::constructormacros` |
| `propertycallmacros.go` | `macros::propertycallmacros` |
| `stringmacros.go` | `macros::stringmacros` |
| `arraymacros.go` | `macros::arraymacros` |
| `arraymacros2.go` | `macros::arraymacros2` |
| `collectionmacros.go` | `macros::collectionmacros` |
| `envmacro.go` | `macros::envmacro` |
| `nameofmacro.go` | `macros::nameofmacro` |
| `keysmacro.go` | `macros::keysmacro` |
| `filemacro.go` | `macros::filemacro` |

The registration tables in `macros/mod.rs` already list every upstream name in Go's table order.
Replace each stub entry with the real function and keep the order, because `macromanager`
iterates it.

- diff: `23_new`, `25_mathmacros`, `27_arraymacros`, `28_collectionmacros`, `30_optional` (all need
  ii). The ledger marks `26_stringmacros` and `31_mixed3b` upstream-corrected, so they get
  behavioral ports.
- Go tests: `callmacros_test`, `arraymacros_test`, `arraymacros_internal_test`,
  `collectionmacros_test`, `stringmacros_test`, `filemacro_test`, `newexpression_test`,
  `TestPropertyCallMacro` and `TestContextualMethodArrayCallbacksInjectSelf` (needs ii) from
  `call_test`, and `TestOptionalChainShapes`, `TestNoOptionalMacroCall`,
  `TestOptionalChainMacroInterplay` from `optionalchain_test`.
- transformer testdata: `arraymacros`, `macroonly`, `new`, and the macro files in `calls`.
- conformance: `macro_math.spec`, `math.spec`, `map.spec`, `set.spec`, `string.spec`,
  `roblox.spec`, `robloxEnum.spec`.

## Slice v: Luau types and native types (`src/luau_types/`)

| Go file | Rust module |
| --- | --- |
| `luautypes.go` | `luau_types::luautypes` |
| `luautypes_alias.go` | `luau_types::luautypes_alias` |
| `luautypes_builder.go` | `luau_types::luautypes_builder` |
| `luautypes_class.go` | `luau_types::luautypes_class` |
| `luautypes_emit.go` | `luau_types::luautypes_emit` |
| `luautypes_typefunction.go` | `luau_types::luautypes_typefunction` |
| `luautypes_typefunction_prelude.go` | `luau_types::luautypes_typefunction_prelude` |
| `nativetypes.go` | `luau_types::nativetypes` |
| `nativetypes_alias.go` | `luau_types::nativetypes_alias` |
| `nativetypes_policy.go` | `luau_types::nativetypes_policy` |
| `nativetypes_spelling.go` | `luau_types::nativetypes_spelling` |

Both features are off by default, and no diff golden depends on this slice. With the features off,
the hooks must not change the output.

- diff: none.
- Go tests: `luautypes_alias_test`, `luautypes_alias_internal_test`, `luautypes_analyze_test`,
  `luautypes_builder_test`, `luautypes_class_test`, `luautypes_class_internal_test`,
  `luautypes_emit_test`, `luautypes_emit_internal_test`, `luautypes_golden_test`,
  `luautypes_typefunction_test`, `nativetypes_test`, `nativetypes_policy_test`.
- transformer testdata: `luautypes`, and `functions/native.ts`, `functions/nonative.ts`,
  `functions/lru*.ts` (with ii).
- conformance: none.

## Slice vi: JSX and the rest (`src/extras/`)

| Go file | Rust module |
| --- | --- |
| `jsx.go` | `extras::jsx` |
| `jsxtext.go` | `extras::jsxtext` |
| `async.go` | `extras::async_` |
| `decorators.go` | `extras::decorators` |
| `trystatement.go` | `extras::trystatement` (minus the two predicates already in `mod.rs`) |
| `labeledstatement.go` | `extras::labeledstatement` |
| `imports.go` | `extras::imports` |
| `importexpr.go` | `extras::importexpr` |
| `sourcemap.go` | `extras::sourcemap` |

- diff: `21a_util`, `21b_main`, `22_exportfrom`, `22a_base`, `24_mixed3a`, `24a_shared`,
  `24b_middle`, `32_jsx`, `34_decorators`, `36_async`, `37_try`, `39_mixed3c` (most need ii,
  and `34_decorators` needs iii).
- Go tests: `jsx_test`, `jsxtext_test`, `async_test`, `decorators_test`, `try_test`,
  `try_exit_test`, `labels_test`, `imports_elision_test`, `sourcemap_test`, `sourcemap_bench_test`
  (as a nextest-skipped bench or a criterion bench), `expression_origins_test`, `origin_body_test`,
  `origin_classes_test` (needs iii), `origin_control_test` (needs ii), `origin_exports_test`,
  `origin_statements_test` and `origin_tagging_test` (they render through `RenderWithSourceMap`).
- transformer testdata: `jsx`, `async`, `try`, `classes/decorators.ts`, and the label files in
  `control`.
- conformance: `roact.spec`, `roact_spread.spec`, `decorator.spec`, `generator.spec`,
  `promise.spec`, `try.spec`, plus the helpers (`helpers/**`), which every spec imports.
