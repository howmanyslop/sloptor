# luauTypes — full-fidelity Luau type emission (experimental)

**Date:** 2026-10-29
**Status:** Shipped on `feat/luau-types` (experimental, opt-in)

## Summary

`luauTypes` is an opt-in sloptor mode that annotates every compiled file with the richest Luau
type available at each annotation site: parameters, `...`, returns, locals, the `self` of every
method, class instance aliases, enum aliases and `type` / `export type` statements for the file's
interfaces and type aliases. TypeScript mapped, conditional and template literal types become Luau
`type function`s. Luau code that `require`s the output sees real types instead of `any`.

It is separate from `nativeTypes` (the narrow `//!native` annotator) and shares one typed Luau
AST and one TS-to-Luau type builder with it. The emitted Luau deliberately differs from rbxtsc,
so the option defaults off, and the output targets Luau's new type solver.

## Goals

- Mirror TypeScript as closely as Luau allows, at every site Luau can annotate, in every file.
- Fidelity over caution: translate type-level computation into `type function`s where Luau can
  evaluate it; fall back to `any` with a recorded, and where it matters a commented, loss.
- Output that passes `luau-lsp analyze` under the new solver in strict mode without a single
  type or syntax error for every fixture, with a test gate that proves it.
- One builder for both modes. `nativeTypes` must keep producing its byte-exact output.

## Non-goals

- `.d.luau` sidecars or `declare` syntax; annotations are inline only.
- The Luau-to-TypeScript direction.
- Old-solver support. `keyof<>`, `index<>` and user-defined type functions exist only under the
  new solver (`luau-analyze` default since 0.721; Roblox Studio has moved to it), and no
  directive is injected to select it.
- Luau's experimental forms: exact tables, `class` statements, `integer`.
- Wrapping long function types or inline unions; only tables and alias-body unions break.

## The two modes

| | `nativeTypes` | `luauTypes` |
|---|---|---|
| Opt-in | `rbxts.nativeTypes` / `rotor.toml` | `--luauTypes` / `rbxts.luauTypes` / `rotor.toml` |
| Files affected | those with a `//!native` directive | every file |
| Sites | parameters and returns | parameters, `...`, returns, generics, `self`, locals, aliases, classes, enums |
| Policy | widen anything that would hurt native codegen tags | spell everything Luau can say |
| Line layout | single-line, byte-compatible with existing fixtures | tables and alias-body unions wider than 80 columns break one member per line |

Interaction rule, when both are on and a file carries `//!native`: the native policy owns the
signatures (parameters, returns, generics and the colon-form class methods that come with them),
and `luauTypes` owns the locals and the type aliases. `TestLuauTypesKeepsNativeSignatures` pins
that the signature lines of a native file do not change when `luauTypes` joins, and that its
`TS.async` local still gets the `(count: number) -> Promise<number>` annotation.

`luauTypes` needs no comment directive, so `removeComments` cannot disable it. A `//!strict`
comment passes through as `--!strict` above the header as before; nothing else selects the mode.

## Architecture

```text
TS checker types ──► typeBuilder (internal/transformer/luautypes*.go) ──► luau.TypeNode tree
                         │ consults typeSpelling                               │
                         │   defaultSpelling  (luauTypes: full fidelity)       │
                         │   nativeSpelling   (nativeTypes, WP3)               ▼
                         ▼                                      luauTypes: render.LayoutTypes
                 alias registry (per file)                                     │
                 type-function translator                                      ▼
                                                           internal/luau/render (types.go)
```

- `internal/luau` holds a node for every production of the Luau type grammar (`TypeReference`,
  singletons, `TypeOptional`, `TypeUnion`, `TypeIntersection`, `TypeTable` with `read`/`write`
  props and indexer, `TypeArray`, `TypeFunction`, `TypeTypeof`, `TypePack`, `TypeVariadic`,
  `TypeGenericPack`), `CastExpression` (`expr :: T`), `TypeAlias` (generics with defaults,
  `Export`) and `TypeFunctionDeclaration`. The renderer owns parenthesization through a
  slot table, so no producer inspects rendered text.
- `typeBuilder` maps `*checker.Type` to `luau.TypeNode` with full fidelity. `typeContext`
  carries the position (nested, parameter, return, local, alias body), the receiver that
  `self` means for method members, the polymorphic `this`, the owning declaration and the
  class-instance flag. `typeSpelling` carries the six decisions a post-pass cannot reverse
  (how a type parameter, an enum member and a class value are spelled; which symbols get a
  `type` statement, what it is called, whether its body is kept). `luauTypes` uses
  `defaultSpelling`; `nativeTypes` plugs in its own spelling plus a policy pass over the built
  tree (WP3). Until that migration lands, a `//!native` file keeps its string emitter and
  `spellingFor` always returns the default spelling.
- Every loss is a `fidelityLoss{Kind, TSText, Luau, Reason, Node}` on the builder;
  `State.LuauTypeLosses()` exposes them to tests, and the alias registry turns the
  declaration-level kinds into comments.
- `luautypes_alias.go` is the registry: placement, exports, mirrors and naming.
  `luautypes_emit.go` decides what each signature and local is annotated with.
  `luautypes_class.go` emits the class and enum shapes. `luautypes_typefunction.go` and
  `luautypes_typefunction_prelude.go` translate type-level alias bodies into `type function`s.

## Mapping (TypeScript to Luau), as shipped

Pinned by `luautypes_builder_test.go` over `testdata/luautypes/src/*.ts`.

| TypeScript | Luau | Loss recorded |
|---|---|---|
| `number`, `string`, `boolean` | the same | |
| `any`, `unknown`, `never` | the same; an `any` member makes a union `any` | |
| `undefined`, `null`, `void` | `nil` nested; `()` as a declared return; `T?` as a union member; `null \| undefined` is `nil` | |
| `"x"`, `true`, `false` | singletons, escaped (`"a\"b\\c\n"`); `true \| false` is `boolean`, `boolean \| undefined` is `boolean?` | |
| number literals, `bigint` | `number`; `1 \| 2` is `number`, `"a" \| 1` is `"a" \| number` | NumberLiteral, BigInt |
| template literal | `string` | TemplateLiteral |
| `symbol`, symbol-keyed members | `any`; the member is skipped | SymbolKey |
| `object`, `{}` | `{ [any]: any }`, `any` | |
| `A \| B` | `TypeUnion`, members deduped structurally (`Array<number> \| number[]` is `{ number }`), optional over a union parenthesized `(string \| number)?`, a function member parenthesized `string \| ((n: number) -> string)` | |
| `A & B` | an intersection of tables (`{}` members dropped) or of function types (overloads: `((n: number) -> string) & ((s: string) -> number)`, also inside an alias); anything else, a branded primitive for one, is `any` | Unspellable |
| interface, object type | `TypeTable`; `readonly` is `read`; optional members `T?`; non-identifier and keyword keys quoted (`["kebab-case"]`, `["1"]`, `["end"]`); a method takes `self: <the alias>` and a function-typed property takes no `self`; an optional method is `((self: Shape) -> ())?`; a written `this` parameter is its type; the polymorphic `this` is the alias | |
| a method of an anonymous object with no name to refer to | `self: any` | ObjectLiteralSelf |
| index signatures | one indexer: `[string]: V`, `[number]: V`, both with one value `[any]: V`, both differing `[any]: any`; `readonly` is `read [string]` | IndexerCollapsed |
| `Record<K, V>` (resolved) | props for literal keys, an indexer for `string`/`number` keys; `Partial<Point>` is `{ x: number?, y: number? }`, `Readonly<Point>` is `{ read x: number, read y: number }` | |
| `T[]`, `ReadonlyArray<T>`, `readonly T[]` | `{ T }`, `{ read T }` | |
| `[A, B]` as a value type | `{ A \| B }` (`{ read A \| B }` when readonly, `{ number }` for `[number, number]`) | Tuple |
| `Map`/`ReadonlyMap`/`WeakMap`, `Set`/`ReadonlySet`/`WeakSet` | `{ [K]: V }`, `{ read [K]: V }`; `{ [T]: true }`, `{ read [T]: true }` | |
| `LuaTuple<[...]>` as a return | a pack: `(number, string)`, `(number, string?)`, `(number, ...string)`, `()`; `LuaTuple<[...]> \| undefined` and `LuaTuple<string[]>` are `...any` (`?` after a pack would mean an optional function); inside a callback type `cb: () -> (number, string)` | OptionalPack, Unspellable |
| `LuaTuple` anywhere else | `any` | LuaTupleOutsideReturn |
| function type | `TypeFunction`: own generics declared, constraints dropped; named parameters when the name is a Luau identifier (`until` makes the parameter unnamed: `(number) -> ()`); parameters past the required count `T?`; rest `...E`; `this` first; returns through the pack rules, a union return parenthesized `() -> ({ string } \| { number })`, a function return right-associative `() -> (n: number) -> (s: string) -> boolean` | ConstraintDropped (once per parameter) |
| rest typed by a tuple or a bare type parameter | `...any` | Unspellable |
| type parameter in scope | `T` by name, no constraint substitution; out of scope (a class's parameter at a static, an enclosing alias's) `any` | Unspellable |
| `keyof T`, `T[K]` with `T` unresolved | `keyof<T>`, `index<T, K>` (new solver) | |
| deferred conditional type | its constraint or apparent type, else `any` | Unspellable |
| `typeof x` | `typeof(x)` when `x` is a top-level `const`, function or class of this file declared before the use, not imported and not renamed; otherwise the resolved type | |
| Roblox datatypes, `Instance` subclasses, `Enum`, `EnumItem`, `Enums` | by name (`Vector3`, `vector`, `CFrame`, `Part`, `RBXScriptSignal`); `Enum.Material`, and `Enum.Material.Plastic` is `Enum.Material` (Luau names the enum, never an item); a Roblox type Luau cannot name is `any` | Unspellable |
| class instance | a reference to the class's instance alias (`Foo`, `Box<number>`) | |
| `typeof Foo`, construct signatures | the structural `{ new: (x: number) -> Foo, make: () -> Foo }` table: `new` plus the statics, no `self` on static functions, `prototype` left out; a cycle through it goes through a synthesized `Anonymous_N` alias | |
| enum type, enum member | `Direction`, `"up"`; a numeric enum is `number` | NumberLiteral |
| library interfaces (`Promise<T>`, `Generator<...>`) | one mirrored hoisted alias; a self reference with other arguments is `any` | RecursiveAliasArgs |
| recursive anonymous object type | a hoisted `Anonymous_N` alias | |
| nesting past 64 levels | `any` | DepthExceeded |

## Aliases, exports, imports, naming

Pinned by `luautypes_alias_test.go` over `aliases.ts`, `imports.ts` and `lib/shapes.ts`.

- Every top-level interface, type alias, enum and class of the file gets a `type` (or
  `export type`) statement at its declaration, after the declaration's leading comments. A
  merged interface emits one statement, at its first declaration, with every member. An alias
  body may be any built type: `export type Mode = "r" | "w"`, `export type Literal = "x"`,
  `export type Handler = (value: number) -> ()`.
- Generic aliases keep their generics and defaults: `Boxed<T = number>`, `Pair<A, B = A>`. A
  default is dropped when it names a parameter declared at or after its own, and then every
  default before the last defaulted parameter too, since Luau defaults must be trailing. A
  parameter Luau cannot name (`until`) is left out of the generic list and every reference to
  it widens to `any`: `Labeled<until = string, T = until>` becomes
  `export type Labeled<T = any> = { label: any, value: T }`, and uses drop that argument.
- A recursive alias must reference itself with its own parameters (`Tree`, `Chain<T>` do);
  `Weird<T> = { nested?: Weird<T[]> }` spells the member `any` and prints
  `-- luauTypes: Weird<T[]> spelled as any: a recursive alias must be referenced with its own
  type arguments` above the statement.
- An alias of a `LuaTuple` is never emitted (a pack is not a type); its uses inline the pack at
  returns and widen elsewhere.
- Exported means: an `export` modifier (`export default interface` included), an
  `export { X }` / `export type { X }` specifier, a member of an exported namespace chain, or an
  ambient export context. `export =` modules still emit `export type`; Luau's `export type` is
  independent of the returned value. A mirrored type is never exported again.
- Everything that is not a top-level declaration of the file is hoisted after the header (and
  the `local TS` import), before the file's first statement, in registration order: nested
  declarations under flattened names (`Geometry.Size` is `export type Geometry_Size`), ambient
  declarations, class expressions, synthesized `Anonymous_N` aliases, and every type from
  another file, `node_modules` or the lib. Imported types are mirrored under their local import
  name (`import { Box as Container }` gives `type Container<T> = { item: T }`; a namespace import
  gives `NS_Name`) because `TS.import` is opaque to Luau's checker and consumers cannot be
  assumed to have definition files. The lib's `Promise<T>` mirrors with its namespace member
  flattened to `Promise_Status`.
- Names are unique per file, in first-registration order: the base name, then `_2`, `_3`, ...
  on a collision with another alias, a Luau builtin, a type-function name (`keyof`, `index`,
  `setmetatable`, ...), a Roblox extern type (an own `interface Part` is `Part_2`), or any
  type-parameter name used anywhere in the file (a bare `T` inside an alias body must always be
  the generic). Value names are a separate namespace.

## Emission sites

Pinned by `luautypes_emit_test.go` over `emission.ts` and `locals.ts`.

- Parameters: `a: number`; `?` or an initializer makes it `string?`; a rest parameter annotates
  `...: string` (and `restArray(...: string): { string }` keeps `local rest = { ... }`); a
  destructured parameter annotates its temp, `_param: { x: number, y: number }`,
  `_param_1: { string }`; a written `this` parameter is `self: { x: number }`.
- Defaulted parameters: rbxtsc's `if c == nil then c = 2 end` refines nothing in either solver,
  so the default is followed by a same-named shadow, `local c = c :: number`. The shadow is
  skipped when the parameter's type admits `nil` anyway.
- Returns: `: boolean`, `: (number, string)`, `: ()` for a void function, and `: ...nil` for a
  void function whose body has an explicit `return` (rbxtsc emits `return nil` for a bare
  `return`, which a `()` pack rejects, and a `nil` return rejects a body that falls off its end;
  `...nil` satisfies both).
- Async functions: the emitted callback returns the settled type, `TS.async(function(n: number):
  number`, and the local that holds it carries the full type,
  `local fetched: (n: number) -> Promise<number> = ...`; `local value: number = TS.await(...)`
  and a destructured `_binding: { number }` follow the locals rule.
- Generators keep the mirrored alias: `counter(limit: number): Generator<number, nil, unknown>`.
- Generics: `local identity = function<T>(value: T): T`; a function's own type parameters are
  declared on the function, an enclosing function's stay in scope by name.
- Methods are never emitted in colon form: colon-form `self` is a fresh inferred table unrelated
  to any alias, `self.nope` is silently accepted and every `self:method()` returns `unknown`
  (probe `method-colon-self.luau`). Class methods become `function Foo.get(self: Foo)`;
  object-literal methods become `inc = function(self: Counter): ()` with the literal's contextual
  alias or a written `this`, and `self: any` (ObjectLiteralSelf) when the literal has no name.
- Locals get `: T` only where Luau's inference has nothing to go on: a written TypeScript type
  (which may be wider than the initializer, `local value: string | number = if flag then "a"
  else 1`), no initializer (`local label: string?`, the hoisted `local later: () -> number`), an
  imported binding (`local aNumber: number = _primitives.aNumber`), or a value rooted in the
  RuntimeLib (`TS.import(...)`, `TS.async(...)`, `TS.await(...)`, `TS.Promise`), whose members
  are `any` to Luau. `local names = { "a", "b" }` stays unannotated. Classes, enums and
  namespaces are declared by their own transforms.
- Logical chains: the `_condition` temp of `a ?? b`, `a || b`, `a && b` is re-bound to the
  expression's TypeScript type once the chain has assigned it (`local _condition = _condition ::
  string`), because Luau types the temp as the union of everything assigned to it. A literal
  spilled to a temp by such a chain keeps its shape (`_object.host = _condition`, never a
  pinned key).

## Classes

Pinned by `luautypes_class_test.go` over `classes.ts`; the probes under
`testdata/luautypes/probes` (re-run by `TestLuauTypesClassIdiomProbes`) settled the shape on
luau-analyze 0.730 and luau-lsp 1.70.1 under both solvers.

```luau
export type Foo = { x: number, get: (self: Foo) -> number }
local Foo = setmetatable({}, {
	__tostring = function()
		return "Foo"
	end,
})
do
	Foo.__index = Foo
	function Foo.new(x: number): Foo
		local self = (setmetatable({}, Foo) :: any) :: Foo
		return Foo.constructor(self, x) or self
	end
	function Foo.constructor(self: Foo, x: number)
		self.x = x
	end
	function Foo.get(self: Foo): number
		return self.x
	end
end
```

- The instance alias is structural, at the declaration, with every property and method, inherited
  ones included, flat: TypeScript classes are structural, so width subtyping gives
  `Derived <: Base` exactly as TS. `typeof(setmetatable({} :: Fields, Foo))` was rejected: the
  metatable part is compared invariantly, so `Derived <: Base` never holds, and the alias would
  have to follow `local Foo`.
- The class table is initialized on its declaration line. rbxtsc's `local Foo` followed by
  `do Foo = setmetatable(...) end` types `Foo` as `T?` inside every function of the block, so
  every `Foo.*` use there errors with "could be nil". Only `Foo.__index = Foo` and the function
  definitions stay in the `do` block. For `extends`, `local super = <heritage>` is declared
  right before the table (`__index = super`), so the heritage expression still runs exactly once.
- `constructor` stays out of the alias and is called through the class table:
  `return Foo.constructor(self, x) or self`. With it in the alias, differing constructor arities
  break `Derived <: Base` (properties are invariant); the plain `or self` is accepted against a
  `-> ()` constructor. `new` takes the constructor's parameters (an implicit derived constructor
  forwards the inherited construct signature instead of a bare `...`), returns `Foo<T...>`, and
  builds `self` through the double cast `(setmetatable({}, Foo) :: any) :: Foo`. Abstract
  classes get no `new`; an abstract class without `extends` stays a plain `{}` table.
- An inherited or overridden method keeps `self` typed as the class that first declared it,
  while its own definition takes the derived class: `Dog`'s alias lists
  `speak: (self: Animal) -> string` and `function Dog.speak(self: Dog): string`. Redeclaring
  `self` as the derived class breaks subtyping (`self` is contravariant), and the
  `Base & { own }` form prints expanded intersections in diagnostics.
- Luau has no class-level generic scope, so every instance function re-declares the class
  generics before its own: `function Box.map<T, U>(self: Box<T>, transform: (value: T) -> U):
  U`. A generic subclass passes its parameter down (`get: (self: Box<T>) -> T` inside
  `Pair<T>`), an instantiated one passes the argument (`self: Box<number>`).
- Statics take the class table: `function Animal.create(self: typeof(Animal), name: string):
  Animal`. Static properties are assigned after the functions and inferred. `__tostring` is
  `function Foo.__tostring(self: Foo): string`.
- Private and protected members are exposed and `readonly` is dropped on class instances
  (constructors write through `self`), one PrivateExposed / ClassReadonlyDropped loss per class.
- A class referenced before its declaration keeps rbxtsc's hoisted `local Late: any`
  (Unspellable): the class table has no type Luau can name before it exists.
- Class expressions hoist their alias under the class name (`Inner`), the assigned variable's
  name (`Anonymous`), or `AnonymousClass_N`; a named class expression keeps its inner name as a
  local of the block.
- An imported class is mirrored like any other imported type, with the interface it implements
  typing the `self` of the methods that interface declared, and the import local is annotated
  with the class value: `local Circle: { new: (radius: number) -> Circle } = TS.import(...)`.
- `typeof Foo` as a TypeScript type stays the structural `{ new: ..., statics }` table.

With `luauTypes` off, classes keep the rbxtsc shape byte for byte
(`TestLuauTypesClassesOffKeepRbxtscShape`).

## Enums

Pinned by `TestLuauTypesEnums` and probe `enum-string.luau`.

- A string enum is a union alias and a read-only map:
  `export type Direction = "up" | "down"` then
  `local Direction: { read Up: "up", read Down: "down" } = { Up = "up", Down = "down" }`.
- A numeric enum is `export type Codes = number` (NumberLiteral) and a writable table with the
  inverse indexer, `local Codes: { A: number, B: number, C: number, [number]: string }`; the
  `setmetatable({}, { __index = _inverse })` it starts from is cast `:: any`, because the empty
  table is not a subtype of the annotated type until the members are assigned. A mixed enum is
  `number | "two"` with the matching member types.
- A `const enum` emits the alias alone; a `declare enum` is mirrored as a hoisted alias when
  referenced. An enum referenced before its declaration keeps the hoisted local, annotated with
  its object type, and is assigned at the declaration after its alias.

## Type functions

Pinned by `luautypes_typefunction_test.go` over `typefunctions.ts`, which asserts the complete
body text of every emitted type function and probes each one through the analyzer with one right
and one wrong table.

- Trigger: a generic type alias whose body contains a mapped, conditional or template literal
  node. A body that is only `keyof T` / `T[K]` stays an alias over Luau's built-ins:
  `export type Values<T> = index<T, keyof<T>>`.
- The translator works from the TypeScript AST of the body and emits Lua over the `types`
  library: a type parameter, `infer` variable or mapped key is the local holding its `type`;
  `keyof T` loops `T:properties()` into a union; `T[K]` is `T:readproperty(K)` (distributed over
  a union key) or, inside the mapped type that iterates `T`, the property record itself; a
  homomorphic mapped type (`[K in keyof T]`, or `[K in P]` with `P` bound by `keyof T`) is
  `types.copy(T)` plus a per-property rewrite honouring `?`, `-?`, `readonly`, `-readonly` and
  an `as` clause; any other mapped type builds `types.newtable()` from the key union (a
  template key becomes a property, `string`/`number` an indexer); a conditional is
  `if item:issubtypeof(U) then X else Y`, distributed over the components of a naked type
  parameter (`types.never` when empty); `infer` is supported in `T extends (infer U)[]` (the
  number indexer's read result) and `T extends (...args: any) => infer R` (the first return);
  a template literal concatenates singleton values and is `types.string` once a hole is not a
  singleton; unions, intersections, optionals, literals, primitives and static object, array
  and function types go through `types.unionof`, `types.intersectionof`, `types.optional`,
  `types.singleton`, `types.newtable`, `types.newfunction`.
- Every body is self-contained: the helpers it needs (`distribute`, `join`, `withoutnil`, `keys`,
  `indexof`, `template`, `returnof`) are `local function`s at the top of that body, because a
  type-function body sees no outer names.
- Every parameter is annotated `T: type` (`type function Partial2(T: type)`): without it an early
  `return T` over-constrains the inferred return type and the body fails to type-check even
  though it evaluates (probe `type-function-param-annotated.luau`).
- The prelude: TypeScript's built-in helpers used generically map to hand-written twins,
  hoisted after the header and never exported: `__Partial`, `__Required`, `__Readonly`, `__Pick`,
  `__Omit`, `__Record`, `__Exclude`, `__Extract`, `__NonNullable`, `__ReturnType`,
  `__Parameters` (`fill<T>(partial: __Partial<T>, defaults: T): T`). `__Partial` is the worked
  `Partial2` body under its own name.
- Call sites: a complete translation keeps the call-site spelling at every use, closed
  arguments included (`partial: Partial2<Config>`, `handlers(): Handlers<"click" | "hover">`),
  except where a return annotation comes from the signature's resolved type (`first(values: {
  number }): number`). A fallback, a prelude helper or an alias of another file with closed
  arguments expands eagerly from the checker's resolved type, which is exact
  (`partialConfig(value: { host: string?, port: number? }): { host: string }`); a generic use
  always names it (`unwrap<T>(value: Unwrap<T>): Unwrap<T>`).
- Fallbacks: an `infer` outside the two shapes, an `as` clause or a value naming another alias
  or type (`Uppercase`, `Box`, `Promise`), or a self reference make the whole alias
  `export type Name<T> = any` under a comment such as `-- luauTypes: Boxed<T> spelled as any:
  ``Box`` names a type outside the type function; type-function bodies see no outer names`, with
  the loss kind of the construct that failed (InferType, KeyRemap, MappedType, ConditionalType).

## Fidelity-loss policy

Kinds: NumberLiteral, Tuple, TemplateLiteral, SymbolKey, BigInt, ConstraintDropped,
IndexerCollapsed, RecursiveAliasArgs, LuaTupleOutsideReturn, OptionalPack,
ClassReadonlyDropped, PrivateExposed, ObjectLiteralSelf, MappedType, ConditionalType,
InferType, KeyRemap, ThisType, DepthExceeded, Unspellable.

Every loss is recorded with the checker's spelling of the TS type, the Luau text that replaced
it, a reason and the owning declaration. Only declaration-level losses attributed to an alias
body become comments, one `-- luauTypes: <TS types> spelled as <Luau>: <reason>` line per
(replacement, reason) pair before the statement: MappedType, ConditionalType, InferType,
KeyRemap and RecursiveAliasArgs. Per-use losses (tuples, number literals, dropped constraints,
collapsed indexers) stay silent to keep the output readable; tests assert them through
`LuauTypeLosses()`.

## Verification

- `TestLuauTypesFixturesAnalyzeClean` renders every fixture under `testdata/luautypes/src` with
  the option on and hands it to a real analyzer: `luau-lsp analyze
  --definitions=testdata/luautypes/sloptor.d.luau --flag:LuauSolverV2=true`, with a `.luaurc`
  of `{"languageMode": "strict"}` next to the module (luau-lsp has no mode flag and defaults to
  the old solver). It falls back to `luau-analyze --mode=strict` without the stub, and skips
  when neither is installed (both are mise tools). Only `TypeError` and `SyntaxError` lines
  count; lints never do. The stub declares `TS`, `script`, `game`, `Enum` and a few extern
  types, and fixtures stay inside that surface.
- `TestLuauTypesClassIdiomProbes` re-runs the hand-written probes that settled the class, enum
  and type-function shapes and asserts the exact set of lines the analyzer rejects; a probe that
  starts failing elsewhere means the analyzer moved, not sloptor.
- The builder, alias, emission, class and type-function tests pin exact rendered lines (in
  order, as whole lines) and the loss kinds, and the type-function tests pin complete bodies
  verbatim. There are no golden files; a change to an unrelated transform does not fail these.
- `TestLuauTypesKeepsNativeSignatures` pins the interaction rule, and the native fixture tests
  pin that `nativeTypes` output is unchanged.

## Known limitations and follow-ups

- New solver only. Under the old solver `keyof<T>`, `index<T, K>` and every `type function`
  are unknown type names, and the class idiom relies on new-solver subtyping.
- `[A, B]` as a value type is `{ A | B }`; positions are lost. Luau has packs only in parameter
  and return positions.
- Number literal types are `number` (Luau has no number singletons), so a numeric
  discriminant loses narrowing. Template literal types are `string`.
- `LuaTuple<[A, B?]>` is `(A, B?)`, and a Luau pack has no omittable elements: a short return
  must pass `undefined` explicitly.
- A generic method inherited through a subtype receiver (`pair.map(...)` where `map` is
  declared on `Box<T>` with `self: Box<T>`) cannot instantiate its generic in the new solver;
  call it through the base type.
- Names are handed out in first-registration order, so a mirrored library or imported type that
  is referenced before the file's own same-named declaration is reached takes the base name and
  the own declaration gets `_2`.
- `export { X as Y }` marks `X` exported but keeps `X` as the alias name; the Luau export is
  not renamed.
- A void function with an explicit `return` is `...nil`, the one pack both `return nil` and
  falling off the end satisfy; a reader expecting `()` will see the odd spelling.
- Static functions inside a `typeof Foo` table carry no `self` parameter, so a TypeScript
  `typeof Foo` value can be called only in dot form.
- A rest parameter typed by a bare type parameter or a tuple is `...any`.
- `declare` output is not emitted; ambient declarations of the project are mirrored into each
  consumer file instead.
- The `nativeTypes` migration onto the shared builder (spelling plus policy pass) is tracked
  separately (WP3); the byte-parity acceptance is that `nativetypes_test.go` passes unchanged.
