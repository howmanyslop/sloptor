# The tsrs differential

`rotor_tsrs` has to give the transformer the same answers Go rotor gets from its typescript-go
mirror. This differential checks that node by node.

- **Go side:** `tools/tsrs-oracle` loads a project the way `internal/compile` does and calls rotor's
  own `transformer` predicates.
- **Rust side:** `rotor_tsrs::dump` writes the same lines through `rotor_tsrs`.
- **Comparison:** `rotor_tsrs::dump::compare` matches them by key. A difference that none of the
  documented deviations below explains fails the comparison.

## Running it

- `cargo nextest run -p rotor_tsrs --test oracle` compares the conformance project with the
  committed Go dump, `testdata/tsrs-oracle/conformance.tsv`. It does not run Go. The test pins the
  count of each deviation, and `mise run tsrs:oracle` regenerates the dump.
- `mise run tsrs:oracle-bench` runs the benchmark project named by `ROTOR_BENCH_PROJECT`, a tsconfig
  path. It writes the Go dump to a temporary file and runs the ignored test against it.

## Line format

There is one line per node that `IsExpression` or `IsDeclaration` accepts, in every file rotor
compiles. Files and nodes are walked in program order. The fields are listed in
`crates/rotor_tsrs/src/dump.rs`:

- type flags and object flags;
- `typeToString`;
- the symbol at the location and the type's symbol, each as name, flags and declaration count;
- the property count of the apparent type;
- the call and construct signature counts;
- `IsDefinitelyType` and `IsPossiblyType` for each of the 21 leaf checks, the two multi-check
  calls rotor makes, `IsSharedTableType`, the `WalkTypes` leaf count, `GetFirstDefinedSymbol` and
  `getFirstConstructSymbol`.

Both sides make the same choices so the lines can be compared:

- **One checker.** The program runs on one checker, and every file is checked before the walk,
  as rotor's precheck does. This fixes the query order. Type ids, and with them union member order,
  can then match.
- **Predicates last.** They run in a second walk. Go's `MacroManager` resolves its symbols in map
  order. Building it before the first walk could shift type ids.
- **Portable names.** The project directory prints as `<project>`. Internal symbol names, which start
  with `\xFE` in the pin and `\x7F` in tsrs, print as `#` and the rest of the name.
- **Object flags without memo bits.** These record which queries already ran, not what the type is:
  the `Computed` flags, `MembersResolved`, the identical-base-type cache and tsrs's own caches. Both
  sides also print the flags in the pin's layout. TypeScript 7.1 moved `FromTypeNode` from bit 30 to
  bit 29, and tsrs reuses bits 28 to 31 for caches. Before this, 3,819 conformance lines differed in
  those bits alone.
- **Panics.** A node whose queries panic prints `panic`.

## Results

| | Conformance project | Benchmark project |
|---|---|---|
| Go lines | 29,679 | 1,038,934 |
| Rust lines | 29,691 | 1,038,966 |
| Identical lines | 29,628 | 1,021,530 |
| Lines with documented deviations | 63 | 17,810 |
| Unexplained mismatches | **0** | **0** |

A line can need two deviations. The counts below can then add up to more than the line count: 55
benchmark lines need both `TypeToStringTruncation` and `ObjectMemberOrder`.

| Deviation | Conformance | Benchmark |
|---|---|---|
| `JsxSpreadAttributeDeclaration` | 12 | 406 |
| `HeritageTypeReference` | 0 | 374 |
| `TypeOnlyImportClause` | 0 | 5,200 |
| `TypeToStringTruncation` | 36 | 11,356 |
| `ObjectMemberOrder` | 0 | 70 |
| `EntityNameQualification` | 0 | 1 |
| `EnumDynamicMemberName` | 3 | 0 |
| `UnionPropertyDeclarations` | 6 | 127 |
| `InstantiationExpressionDeclarations` | 4 | 3 |
| `SyntheticDefaultDeclarations` | 0 | 159 |
| `ThislessMemberInstantiation` | 0 | 210 |
| `DeclaredConditionalConstraint` | 2 | 0 |

No difference turned out to be a port bug.

## Evidence

Each deviation comes from a change between the two TypeScript versions:

- **Pin:** rotor's typescript-go pin, `microsoft/typescript-go@1f955e97`, mirrored in `tsgo/`.
- **TypeScript 7.1:** the 7.1.0-beta tsrs ports, `microsoft/TypeScript@3e9236361fa9`, with its Go
  sources under `tsc/internal/`.

We checked each one in two ways:

- **Source:** the TypeScript 7.1 source differs from the pin in the place each deviation names.
- **Runtime:** TypeScript 7.1's own checker gives tsrs's answer. This used the sync API of the
  `typescript@7.1.0-beta` npm package (`typescript/sync`).
  - **Benchmark project:** we asked about all 17,030 lines that are in both dumps and differ, for
    `typeToString`, the symbol at the location and the type's symbol. All 27,486 answers equal
    tsrs's: 16,587 `s`, 5,537 `sym` and 5,362 `tsym`.
  - **Conformance project:** 31 of its 37 differing `typeToString` lines are identical too. The
    other 6 elide in the same places, but infer `any` for `reduce`'s accumulator. The probe ran on
    a plain copy of the project, without rotor's `@rbxts/compiler-types` rewrite.

A TypeScript-version difference can still change what rotor emits. "Notes for later stages" lists
the differences that rotor's compiler code reads.

Each rule in `compare` applies to listed fields, node kinds and flags. Any other difference on the
same line is a mismatch. The unit tests in `dump.rs` hold examples on both
sides of each rule.

### `JsxSpreadAttributeDeclaration`

TypeScript 7.1's `JsxSpreadAttribute` embeds `DeclarationBase` (`tsc/internal/ast/ast_generated.go`).
The pin's does not (`tsgo/ast/ast_generated.go:6869`). `IsDeclaration` is
`DeclarationData() != nil`. tsrs dumps a line the pin has no node for.

### `HeritageTypeReference`

TypeScript 7.1 parses interface `extends` and class `implements` elements with
`parseTypeHeritageClauseElement` (`tsc/internal/parser/parser.go:1851`). That turns an entity name
into a `TypeReference`. The pin keeps an `ExpressionWithTypeArguments`. Its lines for those wrappers
have no tsrs counterpart, and the entity names inside become type positions. The same change renamed
`GetImplementsTypeNodes` to `GetImplementsHeritageClauseElements` (see `API.md`).

The rule excuses the wrapper (`ExpressionWithTypeArguments` in Go, `TypeReference` in Rust) when Go
has a wrapper line that Rust lacks. Beyond that, it excuses only the `Identifier`,
`PropertyAccessExpression` and `QualifiedName` nodes of the entity name: those that start at the
wrapper and end at or before the end of its expression. Nodes in the type arguments, such as a
`typeof` query, must match.

### `TypeOnlyImportClause`

TypeScript 7.1's `IsTypeDeclaration` accepts an `ImportClause` only if it is type-only and has a
default name (`tsc/internal/ast/utilities.go:3711`). The pin accepts any type-only clause
(`tsgo/ast/utilities.go:3555`). For `import type { X }`, the pin's `getTypeOfNode` then passes a nil
symbol to `getDeclaredTypeOfSymbol` and panics (`tsgo/checker/checker.go:23546`). The oracle prints
`panic` for these clauses. TypeScript 7.1 and tsrs return a type.

### `TypeToStringTruncation`

TypeScript 7.1's `visitAndTransformType` checks the truncation budget before anything else and elides
with `...` (`tsc/internal/checker/nodebuilderimpl.go:3247`). The pin's does not
(`tsgo/checker/nodebuilderimpl.go:3062`). Past the 160-character budget, tsrs elides other parts of
long types than Go does. The other fields of these lines match.

The rule needs three things:

- **Go reached the budget.** The budget counts an approximate length that runs ahead of the printed
  text. The rule takes a Go text of 120 characters or more, or one that already contains a marker. No
  elision in either dump starts in a shorter text without a marker.
- **Rust elided.** The Rust text contains a marker: `...`, `{ ...; }` or `... N more ...`.
- **The texts agree outside the elisions.** Each marker can stand for one balanced run of the other
  text; `{ ...; }` and `... N more ...` can stand for several members. A text cut at 160 characters
  can stop at any marker of the other.

The coordinator's first draft also asked for more markers in Rust than in Go. The dumps rule that
out. Among the benchmark lines with a marker, Rust has more markers in 3,244, as many in 6,305 and
fewer in 1,823. Go's `& { ...; }` becomes Rust's `& ...`, which is one marker on each side. TypeScript 7.1's runtime gives
tsrs's text for all of them.

### `ObjectMemberOrder`

These are unions of object literals. TypeScript normalizes such a union by adding `NAME?: never`
members, and 7.1 prints those added members in other places among the rest. The runtime probe found
TypeScript 7.1 prints them as tsrs does, but it was not traced to one function.

The rule applies to object and union types. It removes the `NAME?: never;` members from both texts,
and requires the rest to be identical and the removed members to be the same multiset. 55 of the 70
lines also need `TypeToStringTruncation`, checked on the texts without those members.

### `EntityNameQualification`

One benchmark line refers to a type through a namespace import. Go prints `check<any>` and tsrs
prints `t.check<any>`, as TypeScript 7.1's runtime does. The rule excuses a Rust text that equals
the Go text with `identifier.` qualifiers added before names. Any other change is a mismatch.

### `EnumDynamicMemberName`

TypeScript 7.1's `getDeclaredTypeOfEnum` skips members where `ast.HasDynamicName` holds
(`tsc/internal/checker/checker.go:24454`). The pin keeps every member with a bindable name
(`tsgo/checker/checker.go:23746`). The conformance enum with the computed key `[constKeyIndex]` gets
the enum's own type, not a `computed` member's.

### `UnionPropertyDeclarations`

TypeScript 7.1's `createUnionOrIntersectionProperty` collects declarations with `core.AppendIfUnique`
(`tsc/internal/checker/checker.go:22064`). The pin appends every constituent's declarations
(`tsgo/checker/checker.go:21465`). In the pin, a property such as `IsA` on a union of instance types
can list hundreds of copies of one declaration.

The rule applies to the `sym` and `tsym` fields only. It requires the same name and flags, the
`Transient` flag set, and fewer declarations in Rust than in Go, but more than zero.

### `InstantiationExpressionDeclarations`

TypeScript 7.1's `getInstantiationExpressionType` gives the new symbol the declarations of the
instantiated type's symbol, with `symbol.SetDeclarations(t.symbol.Declarations())`. The pin's has
none.

### `SyntheticDefaultDeclarations`

TypeScript 7.1's `getTypeWithSyntheticDefaultImportType` gives the anonymous type-literal symbol the
module's declarations (`tsc/internal/checker/checker.go:16065`). The pin's has none
(`tsgo/checker/checker.go:15544`).

### `ThislessMemberInstantiation`

TypeScript 7.1's `instantiateSymbol` returns a member unchanged if the mapper maps only `this` and
the member does not use `this`. The pin makes a transient copy. The symbol at such a property
access has `Transient` in the pin and not in tsrs.

### `DeclaredConditionalConstraint`

This is `getBaseConstraintOfType` on a type alias's own conditional type, at its declaration. When
the conditional type nests four or more levels deep, TypeScript 7.1 doesn't find a constraint. The pin
finds the union of the branches. The runtime probe returned no constraint for such aliases, as tsrs
does, while three levels deep still gave the union. Instantiations of the same alias match in both.

The constraint functions themselves (`computeBaseConstraint`, `getConstraintFromConditionalType`
and its helpers) differ only by a recursion guard of depth 100. So the cause lies elsewhere in 7.1.

The conformance lines are the alias declaration and its name. Expressions get instantiations of the
alias, and those match.

The rule applies only to `TypeAliasDeclaration` nodes and their `Identifier`, with the `Conditional`
type flag. The Rust fields must be what a conditional type without a constraint gives: no apparent
properties, one `WalkTypes` leaf, no `IsSharedTableType`, and the same `IsDefinitelyType` and
`IsPossiblyType` masks, either empty or the array check alone. The rule excuses differences in `ap`,
`d`, `p`, `x` and `w` only.

## Notes for later stages

These differences come from the TypeScript version, but they still reach rotor's compiler code.

- **Declarations on synthetic symbols.** `SyntheticDefaultDeclarations` and
  `InstantiationExpressionDeclarations` give a symbol declarations where the pin gives none.
  Flamework's `guardDeclaration` (`internal/flamework/guard_types.go:27`) returns the first
  declaration of a type's symbol, and returns nil for zero. For these symbols the Rust port would
  return a node where Go returns nil. Stage 6 must treat these synthetic symbols as declarationless,
  or add a parity fixture that shows the guard output does not change.
- **Ambient modules.** The pin binds a pattern module such as `declare module "*.json"` in the
  globals under its quoted text, and `GetAmbientModules` returns one merged symbol per pattern.
  TypeScript 7.1 binds one symbol per declaration. `TypeChecker::get_ambient_modules` keeps those,
  because rotor's only caller (`internal/compile/incremental_manifest.go:362`) collects the
  declaring files, and the union of the declarations is the same. Go ranges over a map, and neither
  list has an order to keep.
- **Truncated type text.** `typeToString` output past the budget differs. Code that compares or
  emits such text, such as diagnostics, doesn't match Go byte for byte.
