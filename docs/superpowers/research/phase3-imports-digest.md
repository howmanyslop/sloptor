# Phase 3 Imports Digest — imports, export-from, module resolution, runtime lib, NewExpression

Source of truth for porting roblox-ts v3.0.0 module machinery to Go without reading the TS.
All paths relative to `reference/roblox-ts/src/` unless prefixed. Every checker/type-API usage
is flagged `CHECKER:`. Builds on `phase2-transforms-digest.md` (P2) and
`phase2b-transforms-digest.md` (P2b). SCOPE: import declarations (default/named/namespace/
side-effect/equals), import elision via the emit resolver, export-from + `export =`, the full
`createImportExpression` pipeline, RojoResolver/PathTranslator API surface (NOT vendored in
reference/ — extracted from shipped dist), runtime-lib emission per ProjectType (with real
rbxtsc ground truth for the diff fixture project), NewExpression + constructor macros, and the
multi-file CompileFile spec.

Dispatch entries (`nodes/statements/transformStatement.ts`):

- L80 `ts.SyntaxKind.ExportAssignment → transformExportAssignment`
- L81 `ExportDeclaration → transformExportDeclaration`
- L87 `ImportDeclaration → transformImportDeclaration`
- L88 `ImportEqualsDeclaration → transformImportEqualsDeclaration`
Expression-side: dynamic `import("x")` intercepted in `transformCallExpressionInner`
(`nodes/expressions/transformCallExpression.ts` L121–123: `if (ts.isImportCall(node)) return
transformImportExpression(state, node)`); `NewExpression` via transformExpression dispatch.

---

## 1. Import statements

### 1.1 transformImportDeclaration — `nodes/statements/transformImportDeclaration.ts` L39–131

```ts
export function transformImportDeclaration(state: TransformState, node: ts.ImportDeclaration) {
	// no emit for type only
	const importClause = node.importClause;
	if (importClause && importClause.isTypeOnly) return luau.list.make<luau.Statement>();   // L42

	const statements = luau.list.make<luau.Statement>();

	assert(ts.isStringLiteral(node.moduleSpecifier));                                       // L46
	const importExp = new Lazy<luau.IndexableExpression>(() =>
		createImportExpression(state, node.getSourceFile(), node.moduleSpecifier),
	);                                                                                       // L47–49
```

`Lazy` (`Shared/classes/Lazy.ts`) is a get/set memo: `get()` runs the factory once;
`set(v)` overrides. The TS.import call is therefore only BUILT if some binding survives
elision — a fully-elided import emits NOTHING, not even the require (see 1.4).

Steps with an importClause (L51–120):

1. **Use counting** (L53, helper `countImportExpUses` L13–37): counts how many bindings will
   actually read `importExp`:
   - default name: +1 if `state.resolver.isReferencedAliasDeclaration(importClause) && (!symbol
     || isSymbolOfValue(symbol))` where `symbol = getOriginalSymbolOfNode(state.typeChecker,
     importClause.name)` (L16–21). CHECKER: emit-resolver call (§1.5) + `getSymbolAtLocation`
     - `ts.skipAlias` (getOriginalSymbolOfNode.ts L3–9).
   - namespace import: unconditional +1 (L24–25). NO elision check — `import * as ns` always
     binds if the clause survived the type-only check.
   - each named element: same referenced+value check as default, on the element (L27–32).
2. **Temp var when uses > 1** (L54–65):

   ```ts
   const moduleName = node.moduleSpecifier.text.split("/");
   const id = luau.tempId(cleanModuleName(moduleName[moduleName.length - 1]));
   luau.list.push(statements, luau.create(luau.SyntaxKind.VariableDeclaration, { left: id, right: importExp.get() }));
   importExp.set(id);
   ```

   `cleanModuleName` (`util/cleanModuleName.ts`): `name.replace(/\W/g, "_")` — last specifier
   segment, non-word chars → `_`. tempId render gets the usual `_` prefix: ground truth
   `local __scratch_util = TS.import(...)` for specifier `"./_scratch_util"` (§5.3).
   With uses == 1 the call is inlined at the single use site; uses == 0 → nothing.
3. **Default import** (L68–88): if name passes the same referenced+value check:

   ```ts
   const moduleFile = getSourceFileFromModuleSpecifier(state, node.moduleSpecifier);
   const moduleSymbol = moduleFile && state.typeChecker.getSymbolAtLocation(moduleFile);     // CHECKER (L73)
   if (moduleSymbol && state.getModuleExports(moduleSymbol).some(v => v.name === "default")) {
       ...transformVariable(state, importClauseName, luau.property(importExp.get(), "default"))
   } else {
       ...transformVariable(state, importClauseName, importExp.get())                        // synthetic default interop
   }
   ```

   CHECKER: `state.getModuleExports` = cached `typeChecker.getExportsOfModule(moduleSymbol)`
   (TransformState.ts L308–312). If the target has a real `default` export → `.default`
   property; otherwise (allowSyntheticDefaultImports over an `export =` module) the default
   binding receives the WHOLE module table. Each binding is wrapped
   `state.capturePrereqs(() => transformVariable(...))` — transformVariable
   (`transformVariableStatement.ts` L19–55) validates the identifier, prereqs a
   `local <name> = <right>` (or `name = right` if hoisted via checkVariableHoist; or an
   `exports.X =` assignment if isSymbolMutable — never true for imports), and registers
   nothing else. So imports reuse the standard variable machinery.
4. **Namespace import** (L93–99): `transformVariable(state, bindings.name, importExp.get())` —
   whole module table, no per-member binding.
5. **Named elements** (L102–118): per surviving element,
   `transformVariable(state, element.name, luau.property(importExp.get(), (element.propertyName ?? element.name).text))`
   — alias `import { greet as g }` reads property `greet` (propertyName), binds local `g`.
6. **Emission ordering**: temp decl (if any) → default → namespace → named elements, in source
   order of the clause.

### 1.2 Side-effect imports + verbatimModuleSyntax — L122–128

```ts
// ensure we emit something
if (!importClause || (state.compilerOptions.verbatimModuleSyntax && luau.list.isEmpty(statements))) {
	const expression = importExp.get();
	if (luau.isCallExpression(expression)) {
		luau.list.push(statements, luau.create(luau.SyntaxKind.CallStatement, { expression }));
	}
}
```

`import "./x"` (no clause) → bare `TS.import(script, ...)` CallStatement. Under
verbatimModuleSyntax, an import whose bindings all elided still emits the call (side effects
preserved). The `luau.isCallExpression` guard skips the case where importExp was already
`set()` to a temp id (cannot happen here in practice — uses==0 when statements is empty).

### 1.3 transformImportEqualsDeclaration — `nodes/statements/transformImportEqualsDeclaration.ts` L10–44

- `import x = require("./y")` (ExternalModuleReference, L12–36): assert string literal;
  `importExp = createImportExpression(...)` EAGERLY (no Lazy);
  CHECKER: `aliasSymbol = typeChecker.getSymbolAtLocation(node.name)` (assert), then
  `if (isSymbolOfValue(ts.skipAlias(aliasSymbol, state.typeChecker)))` →
  `transformVariable(state, node.name, importExp)` — whole module table, no `.default`
  unwrapping ever. verbatimModuleSyntax fallback CallStatement as in 1.2 (L28–34).
- `import A = B.C` (Identifier/QualifiedName, L37–43, issue #1895):
  `transformVariable(state, node.name, transformEntityName(state, moduleReference))` — plain
  aliasing of a namespace path, no import machinery.

### 1.4 Elision semantics — what `isReferencedAliasDeclaration` is asked

Upstream resolver: `state.resolver = typeChecker.getEmitResolver(sourceFile)`
(TransformState.ts L61 — strada's per-file getEmitResolver forces the file to be CHECKED so
alias-reference marks exist). Five call sites, all in this phase's files:

- transformImportDeclaration.ts L18, L29 (countImportExpUses), L71 (default), L105 (named)
- transformExportDeclaration.ts L14 (export specifiers, §2.1)
Asked with: `ts.ImportClause` (for the default name), `ts.ImportSpecifier`,
`ts.ExportSpecifier`. Never with NamespaceImport (unconditionally kept).

Strada semantics: true iff the alias symbol was marked `referenced` during checking
(markAliasReferenced fires when an identifier resolves through the alias in a VALUE position,
excluding type-only uses and const-enum-only targets unless preserveConstEnums), OR (for
exported aliases) the alias target is a value. Combined in the transform with
`!symbol || isSymbolOfValue(symbol)` over the SKIPPED alias (`util/isSymbolOfValue.ts`:
`!!(symbol.flags & ts.SymbolFlags.Value) && !(symbol.flags & ts.SymbolFlags.ConstEnum)`) —
so type-only and const-enum imports drop even when "referenced".

**tsgo offers this directly** — no shim needed:

- `tsgo/checker/emitresolver.go` L688–712 `func (r *EmitResolver) IsReferencedAliasDeclaration(node *ast.Node) bool`:

  ```go
  if !c.canCollectSymbolAliasAccessibilityData || !ast.IsParseTreeNode(node) { return true }
  ...
  if ast.IsAliasSymbolDeclaration(node) {
      if symbol := c.getSymbolOfDeclaration(node); symbol != nil {
          aliasLinks := c.aliasSymbolLinks.Get(symbol)
          if aliasLinks.referenced { return true }
          target := aliasLinks.aliasTarget
          if target != nil && node.ModifierFlags()&ast.ModifierFlagsExport != 0 &&
              c.getSymbolFlags(target)&ast.SymbolFlagsValue != 0 && ... { return true }
      }
  }
  return false
  ```

- `canCollectSymbolAliasAccessibilityData = c.compilerOptions.VerbatimModuleSyntax.IsFalseOrUnknown()`
  (`tsgo/checker/checker.go` L922) — under verbatimModuleSyntax the method returns true for
  everything (matches strada; pairs with the 1.2 fallback).
- `aliasLinks.referenced` is set by `markAliasReferenced` (checker.go L28500, sets at L28525)
  DURING CHECKING. PORT RULE: the file must be fully checked (rotor already calls
  `program.GetSemanticDiagnostics(ctx, sourceFile)` before transforming — keep that per file
  in the program-wide loop, §7). tsgo's resolver is program-global:
  `checker.GetEmitResolver()` (checker.go L31857), no sourceFile argument; it does NOT
  trigger checking itself (strada's did). `EmitResolver.MarkLinkedReferencesRecursively`
  (emitresolver.go L799) exists for noCheck emit only — not needed if we check first.
- Related available APIs: `IsValueAliasDeclaration` (L714), `IsTopLevelValueImportEqualsWithEntityName`
  (L780), `GetExternalModuleFileFromDeclaration` (L820). tsgo's own import elision transformer
  (`tsgo/transformers/tstransforms/importelision.go`) uses the same resolver — cross-check.

### 1.5 moduleIdBySymbol registration

`setModuleIdBySymbol` has exactly two call sites: `transformSourceFile.ts` L206
(`state.setModuleIdBySymbol(symbol, luau.globals.exports)` — the FILE symbol maps to the
`exports` identifier) and `transformModuleDeclaration.ts` L58 (namespace containerId, P2).
Imports do NOT register module ids; they only create local bindings. Consumers:
`getModuleIdFromNode` (TransformState.ts L351–354; walks `getModuleAncestor` =
nearest SourceFile/ModuleDeclaration, traversal.ts L57–59 — CHECKER: getSymbolAtLocation on
the source file / module name) used by transformExportDeclaration L68 +
transformExportAssignment L22, and `getModuleIdPropertyAccess` (L356–364, export-let reads,
P2) which consults `getModuleExportsAliasMap` (L314–328; CHECKER: getExportsOfModule +
ts.skipAlias; ExportSpecifier declarations alias under `declaration.name.text`).

---

## 2. Export-from and `export =`

### 2.1 transformExportDeclaration — `nodes/statements/transformExportDeclaration.ts` L125–133

```ts
if (node.isTypeOnly) return luau.list.make<luau.Statement>();   // export type { ... }
if (node.moduleSpecifier) return transformExportFrom(state, node);
return luau.list.make<luau.Statement>();                        // plain `export { x }` — no emit here
```

Plain `export { x }` (no specifier) emits nothing at the statement; handleExports collects it
via getExportPair (§2.3).

`isExportSpecifierValue` (L9–24): `element.isTypeOnly` → false; else
`state.resolver.isReferencedAliasDeclaration(element)` → true; else fallback
CHECKER: `aliasSymbol = typeChecker.getSymbolAtLocation(element.name)` and
`isSymbolOfValue(ts.skipAlias(aliasSymbol, ...))` → true; else false. (The fallback makes
re-exports of values kept even when "unreferenced" locally.)

`transformExportFrom` (L40–123):

1. Use counting (L26–38): NamedExports → count of value specifiers; namespace
   (`export * as ns`) or bare star → always 1.
   uses == 1 → inline `createImportExpression`; uses > 1 → temp
   `luau.tempId(cleanModuleName(lastSegment))` + VariableDeclaration (L50–62), same shape as
   imports. uses == 0 → return empty (L64–66).
2. `const moduleId = state.getModuleIdFromNode(node)` (L68) — file level ⇒ `exports`.
3. `export { a, b as c } from "./m"` (L70–83): per value specifier
   `exports.<element.name> = <importExp>.<propertyName ?? name>` Assignment at the STATEMENT
   position.
4. `export * as foo from "./m"` (L84–94): `exports.foo = <importExp>`.
5. `export * from "./m"` (L95–118):

   ```ts
   const keyId = luau.tempId("k"); const valueId = luau.tempId("v");
   // ForStatement: for _k, _v in <importExp> or {} do exports[_k] = _v end
   // importExp may be `nil` in .d.ts files, so default to `{}`
   expression: luau.binary(importExp, "or", luau.map()),
   ```

6. `state.hasExportFrom = true` (L120) — forces the `local exports = {}` path in
   handleExports.

### 2.2 transformExportAssignment — `nodes/statements/transformExportAssignment.ts` L45–60

```ts
const symbol = state.typeChecker.getSymbolAtLocation(node.expression);                 // CHECKER L46
if (symbol && isSymbolMutable(state, symbol)) DiagnosticService.addDiagnostic(errors.noExportAssignmentLet(node));
if (symbol && !isSymbolOfValue(ts.skipAlias(symbol, state.typeChecker))) return luau.list.make(); // type-only `export =`
if (node.isExportEquals) return transformExportEquals(state, node);
else return transformExportDefault(state, node);
```

- `transformExportEquals` (L10–27): sets `state.hasExportEquals = true`. If the node is the
  FINAL statement of the file → direct `return <expr>`; otherwise
  `local exports = <expr>` (left: `state.getModuleIdFromNode(node)` = `exports`) and
  handleExports appends `return exports` later.
- `transformExportDefault` (`export default <expr>`, L29–43): captures prereqs, then
  `local default = <expr>` (literal identifier `default`); collected into the exports table by
  getExportPair like any other export. (Named function/class default exports never reach
  here — they're FunctionDeclaration/ClassDeclaration with ExportDefault modifier, P2b §1.2.)

### 2.3 handleExports interplay — `nodes/transformSourceFile.ts` L94–189

(Already ported in rotor `internal/transformer/exports.go`; restated for the export-from
additions.) Selection loop (L100–130) over CHECKER `state.getModuleExports(fileSymbol)` skips:

1. `ignoredExportSymbols` (L47–67 `getIgnoredExportSymbols`): for each
   `export * from "./m"` statement, ALL of m's exports
   (CHECKER: `getOriginalSymbolOfNode(typeChecker, statement.moduleSpecifier)` →
   `state.getModuleExports(moduleSymbol)`); for `export * as id`, the id's own symbol
   (CHECKER: getSymbolAtLocation).
2. `SymbolFlags.Prototype` members.
3. `isExportSymbolFromExportFrom` (L33–45): any declaration that is an ExportSpecifier whose
   ExportDeclaration has a moduleSpecifier — i.e. `export { x } from` symbols, already handled
   as statement-position assignments (§2.1.3).
4. non-values (skipAlias + isSymbolOfValue), mutables (→ `mustPushExports = true`),
   declare-only (`isExportSymbolOnlyFromDeclare` L78–86).
Then: hasExportEquals → only the conditional `return exports` (L132–142);
`mustPushExports` (= hasExportFrom || mutable export) → `local exports = {}` UNSHIFTED to the
file top + trailing `exports.<key> = <id>` per surviving pair + `return exports` (L143–167);
else plain `return { k = v, ... }` (L168–188).

### 2.4 Star-export insertion order — ground truth for the exports.go TODO

`internal/transformer/exports.go` L355–358 TODO(phase-3 imports) worries the filename-primary
sort key cannot reproduce cross-file interleave. Ground truth shows the trailing-pairs problem
is SMALLER than feared because star/export-from symbols never reach exportPairs (skips 1+3
above) — the interleave lives in STATEMENT POSITION. Real rbxtsc 3.0.0 output for

```ts
export const own = 1;
export * from "./_scratch_util";
export { greet as hello } from "./_scratch_util";
```

is (verbatim, fixture project, `--type model`):

```lua
-- Compiled with roblox-ts v3.0.0
local TS = require(script.Parent.include.RuntimeLib)
local exports = {}
local own = 1
for _k, _v in TS.import(script, script.Parent, "_scratch_util") or {} do
	exports[_k] = _v
end
exports.hello = TS.import(script, script.Parent, "_scratch_util").greet
exports.own = own
return exports
```

Each export-from statement created its OWN TS.import (uses counted per declaration, both 1 ⇒
inline, no temp). `exports.own` (the only exportPairs survivor) trails. Remaining ordering
risk for rotor: `getModuleExports` iteration order of the FILE symbol still drives trailing
pairs and the plain return-table — rotor's (file, functions-first pass, pos) sort already
matches single-file goldens; with export-from the ignored sets must be computed from the
same symbol identities (use `checker.GetExportsOfModule` results, compare by pointer).

---

## 3. createImportExpression — `util/createImportExpression.ts` (THE core)

### 3.1 Entry — L212–220 + getImportParts L184–210

```ts
export function createImportExpression(state, sourceFile, moduleSpecifier): luau.IndexableExpression {
	const parts = getImportParts(state, sourceFile, moduleSpecifier);
	parts.unshift(luau.globals.script);
	return luau.call(state.TS(moduleSpecifier.parent, "import"), parts);
}
```

ALWAYS `TS.import(script, <root expr>, "<name>"...)` — `state.TS` sets `usesRuntimeLib`
(TransformState.ts L189–197, plus `warnings.runtimeLibUsedInReplicatedFirst` for Game files
whose rbxPath[0] == "ReplicatedFirst", computed in the ctor L63–65). WaitForChild semantics
live INSIDE RuntimeLib's TS.import at runtime — the emitted AST carries plain strings.

`getImportParts` (L184–210):

1. `const moduleFile = getSourceFileFromModuleSpecifier(state, moduleSpecifier)`; missing →
   `errors.noModuleSpecifierFile` + `[luau.none()]` (every error path returns `[luau.none()]`
   so the call renders as `TS.import(script, nil)` — diagnostics carry the failure).
2. `const virtualPath = state.guessVirtualPath(moduleFile.fileName) || moduleFile.fileName`
   (TransformState.ts L367–385: reverse-symlink lookup via
   `program.getSymlinkCache?.().getSymlinkedDirectoriesByRealpath()`, walking dirname upward;
   CHECKER/internal: program symlink cache — tsgo has `tsgo/symlinks`; only matters for
   symlinked node_modules (pnpm). Port later; `|| fileName` fallback is safe default).
3. `ts.isInsideNodeModules(virtualPath)` (strada helper ≈ path contains `/node_modules/`;
   tsgo: no public ast helper — reimplement trivially) →
   `moduleOutPath = state.pathTranslator.getImportPath(state.nodeModulesPathMapping.get(getCanonicalFileName(path.normalize(virtualPath))) ?? virtualPath, /* isNodeModule */ true)`
   then `getNodeModulesImportParts`. The mapping (d.ts → main .lua) is built per typeRoot
   package from package.json `main` + `types`/`typings` (`Project/functions/createNodeModulesPathMapping.ts`
   L6–34: key `getCanonicalFileName(resolve(pkgPath, typesPath))`, value
   `resolve(pkgPath, main)`). `getCanonicalFileName` = lowercase unless case-sensitive FS
   (`Shared/util/getCanonicalFileName.ts`).
4. else `moduleOutPath = state.pathTranslator.getImportPath(virtualPath)` (rebased into
   outDir) → `moduleRbxPath = state.rojoResolver.getRbxPathFromFilePath(moduleOutPath)`;
   missing → `errors.noRojoData(moduleSpecifier, relative(projectPath, moduleOutPath), false)`;
   else `getProjectImportParts`.

### 3.2 getSourceFileFromModuleSpecifier — `util/getSourceFileFromModuleSpecifier.ts` L4–33

```ts
const symbol = state.typeChecker.getSymbolAtLocation(moduleSpecifier)
            ?? state.typeChecker.resolveExternalModuleName(moduleSpecifier);     // CHECKER (internal #2)
if (symbol) {
	const declaration = symbol.valueDeclaration;
	if (declaration && ts.isModuleDeclaration(declaration) && ts.isStringLiteralLike(declaration.name)) {
		// ambient module decl: chase the REAL file through the program's resolution cache
		const mode = state.program.getModeForUsageLocation(sourceFile, declaration.name);
		const resolvedModuleInfo = state.program.getResolvedModule(sourceFile, declaration.name.text, mode);
		if (resolvedModuleInfo?.resolvedModule)
			return state.program.getSourceFile(resolvedModuleInfo.resolvedModule.resolvedFileName);
	}
	if (declaration && ts.isSourceFile(declaration)) return declaration;
}
// Fallback for $getModuleTree when module is not referenced by any regular import
if (ts.isStringLiteralLike(moduleSpecifier)) {
	const result = ts.resolveModuleName(moduleSpecifier.text, sourceFile.path, state.compilerOptions, ts.sys);
	if (result.resolvedModule) return state.program.getSourceFile(result.resolvedModule.resolvedFileName);
}
```

tsgo equivalents, all public: `Checker.ResolveExternalModuleName`
(`tsgo/checker/exports.go` L104), `Program.GetModeForUsageLocation`
(`tsgo/compiler/program.go` L1509), `Program.GetResolvedModule` (L468) /
`GetResolvedModuleFromModuleSpecifier` (L477), `Program.GetSourceFile`. The raw
`ts.resolveModuleName` fallback maps to tsgo `module.Resolver` — only needed for
`$getModuleTree` of never-imported modules; defer.

### 3.3 getNodeModulesImportParts — L70–134

```ts
const moduleScope = path.relative(state.data.nodeModulesPath, moduleOutPath).split(path.sep)[0]; // "@rbxts"
if (!moduleScope.startsWith("@"))  → errors.noUnscopedModule, [none]
if (!validateModule(state, moduleScope)) → errors.noInvalidModule, [none]
```

`validateModule` (L49–59): `path.join(nodeModulesPath, scope)` must `path.normalize`-equal one
of `state.compilerOptions.typeRoots`. (`data.nodeModulesPath` = `<pkgJsonDir>/node_modules`,
`Project/functions/createProjectData.ts` L31.)

- **Package** projectType (L89–110): find rbxPath via the FIRST matching of
  `state.pkgRojoResolvers` (one `RojoResolver.synthetic(typeRoot)` per typeRoot,
  compileFiles.ts L77; helper `findRelativeRbxPath` L61–68); none →
  `errors.noRojoData(..., relative(projectPath, moduleOutPath), /*isPackage*/ true)`. Emit:

  ```ts
  propertyAccessExpressionChain(
      luau.call(state.TS(moduleSpecifier.parent, "getModule"),
          [luau.globals.script, luau.string(moduleScope), luau.string(moduleName)]),  // moduleName = relativeRbxPath[0]
      relativeRbxPath.slice(1))
  ```

  → `TS.getModule(script, "@rbxts", "services")` or `TS.getModule(script, "@rbxts", "pkg").out.foo`.
- **Game/Model** (L111–133): `moduleRbxPath = state.rojoResolver.getRbxPathFromFilePath(moduleOutPath)`
  (else noRojoData/true); the scope must appear in the rbxPath with `NODE_MODULES`
  ("node_modules", Shared/constants.ts L11) immediately before it:
  `indexOfScope === -1 || moduleRbxPath[indexOfScope - 1] !== NODE_MODULES` →
  `errors.noPackageImportWithoutScope(specifier, relativePath, moduleRbxPath)`. Then falls
  through to getProjectImportParts (package modules are imported like project files).

### 3.4 getProjectImportParts — L136–182

```ts
const moduleRbxType = state.rojoResolver.getRbxTypeFromFilePath(moduleOutPath);
if (moduleRbxType === RbxType.Script || moduleRbxType === RbxType.LocalScript)
	→ errors.noNonModuleImport, [none]
const sourceOutPath = state.pathTranslator.getOutputPath(sourceFile.fileName);
const sourceRbxPath = state.rojoResolver.getRbxPathFromFilePath(sourceOutPath);
if (!sourceRbxPath) → errors.noRojoData(sourceFile, relative(projectPath, sourceOutPath), false), [none]
```

- **Game** (L158–178): network check first — skipped when `ts.isImportCall(moduleSpecifier.parent)`
  (dynamic `import("")` may be guarded by RunService checks at runtime):
  `getNetworkType(moduleRbxPath) === Server && getNetworkType(sourceRbxPath) !== Server` →
  `errors.noServerImport`. Then `fileRelation = rojoResolver.getFileRelation(sourceRbxPath, moduleRbxPath)`:
  OutToOut | InToOut → `getAbsoluteImport(moduleRbxPath)`; InToIn →
  `getRelativeImport(sourceRbxPath, moduleRbxPath)`; OutToIn → `errors.noIsolatedImport`.
- **Model/Package** (L179–181): always `getRelativeImport`.

`getAbsoluteImport` (L15–24): `[createGetService(moduleRbxPath[0]), "p1", "p2", ...]` —
`createGetService` (`util/createGetService.ts`) = `game:GetService("X")` MethodCallExpression;
remaining segments as plain string args (TS.import WaitForChilds them at runtime). Full call:
`TS.import(script, game:GetService("ReplicatedStorage"), "shared", "mod")`.

`getRelativeImport` (L26–47): `relativePath = RojoResolver.relative(sourceRbxPath, moduleRbxPath)`;
leading `RbxPathParent` entries become PARENT_FIELD ("Parent", constants.ts L33) segments of
ONE `propertyAccessExpressionChain(luau.globals.script, ["Parent", ...])` (so the root expr is
`script.Parent.Parent...`), remaining string segments as separate args:
`TS.import(script, script.Parent, "_scratch_util")`.

### 3.5 Dynamic import + $getModuleTree

`nodes/expressions/transformImportExpression.ts` L8–30: non-string-literal arg →
`errors.noNonStringModuleSpecifier`, `luau.none()`. Else

```ts
luau.call(luau.property(state.TS(node, "Promise"), "new"), [FunctionExpression(
    parameters: [resolve], statements: [CallStatement(resolve(<createImportExpression(...)>))])])
```

→ `TS.Promise.new(function(resolve) resolve(TS.import(script, ...)) end)`.
`macros/callMacros.ts` L56–60 `$getModuleTree`: `getImportParts` then
`luau.array([parts.shift()!, luau.array(parts)])` → `{ root, { "rest", "of", "path" } }`.

---

## 4. RojoResolver + PathTranslator API surface

NOT in reference/ — separate packages. Shipped implementation extracted from
`testdata/diff/project/node_modules/@roblox-ts/rojo-resolver/out/RojoResolver.{d.ts,js}`
(345-line JS = the entire library) and `@roblox-ts/path-translator/out/PathTranslator.{d.ts,js}`
(136 lines). Both version **1.1.0** (package.json). **Phase 3 must vendor the repos**
(github.com/roblox-ts/rojo-resolver @ 1.1.0, github.com/roblox-ts/path-translator @ 1.1.0)
into reference/ for line-cited porting; the dist JS below is complete enough to port from
directly if vendoring stalls.

### 4.1 RojoResolver (RojoResolver.js line refs)

Types: `RbxPath = ReadonlyArray<string>`; `RbxPathParent = Symbol("Parent")` (L113);
`RelativeRbxPath = ReadonlyArray<string | RbxPathParent>`; enums `RbxType`
{ModuleScript, Script, LocalScript, Unknown}, `FileRelation` {OutToOut, OutToIn, InToOut,
InToIn}, `NetworkType` {Unknown, Client, Server}.
Constants (L11–46): exts — module exts {.luau, .json, .toml}, script exts {.luau}; `.lua`
inputs converted to `.luau` everywhere via `convertToLuau` (L107–112); subext map `""`→Module,
`.server`→Script, `.client`→LocalScript (L32–36); DEFAULT_ISOLATED_CONTAINERS =
[StarterPack], [StarterGui], [StarterPlayer,StarterPlayerScripts],
[StarterPlayer,StarterCharacterScripts], [StarterPlayer,StarterCharacter],
[PluginDebugService] (L37–44); CLIENT_CONTAINERS = StarterPack/StarterGui/StarterPlayer;
SERVER_CONTAINERS = ServerStorage/ServerScriptService (L45–46); config names
`default.project.json` / legacy `roblox-project.json` / `*.project.json` (L22–24).

Methods roblox-ts calls (call sites in §1–3, compileFiles, TransformState):

- `static findRojoConfigFilePath(projectPath)` (L115–131): default.project.json wins; else
  candidates by regex, warn on multiple. → `{ path?, warnings }`.
- `static fromPath(configPath)` (L146–150) — parseConfig(resolve(path), doNotPush=true).
- `static synthetic(basePath)` (L151–155) — `parseTree(basePath, "", { $path: basePath }, true)`;
  used for outDir-less packages and per-typeRoot pkg resolvers.
- `static fromTree(basePath, tree)` (L156–160).
- `parseConfig/parseTree/parsePath/searchDirectory` internals (L161–244): tree walk pushes
  child names onto `this.rbxPath`; `$className: "DataModel"` ⇒ `isGame = true` (L187–189);
  `$path` resolution: module-ext file → `filePathToRbxPathMap.set(itemPath, [...rbxPath])`;
  directory containing default.project.json → nested parseConfig; else
  `partitions.unshift({ fsPath, rbxPath })` (LIFO — later/deeper partitions match FIRST) +
  recursive searchDirectory (which also recurses into nested `*.project.json` files).
- `getRbxPathFromFilePath(filePath)` (L245–264): resolve + lua→luau; exact map hit; else first
  partition where `isPathDescendantOf`; strip exts (`stripRojoExts` L60–72: module ext, then
  `.server`/`.client` subext for script exts), split relative, drop trailing `init`
  (INIT_NAME) for script exts; `partition.rbxPath.concat(relativeParts)`. Returns undefined
  if uncovered (→ noRojoData).
- `getRbxTypeFromFilePath(filePath)` (L265–276): script ext → subext map (unknown subext ⇒
  RbxType.Unknown); non-script ext (.json/.toml/anything) → ModuleScript.
- `getFileRelation(fileRbxPath, moduleRbxPath)` (L288–308): containment in
  isolatedContainers (prefix match `arrayStartsWith`, only when `isGame` — L277–287
  getContainer); both-same → InToIn, both-diff → OutToIn, file-only → InToOut, module-only →
  OutToIn, neither → OutToOut.
- `isIsolated(rbxPath)` (L309–311), `getNetworkType(rbxPath)` (L312–320: server containers
  checked before client).
- `static relative(rbxFrom, rbxTo)` (L321–340): first index where they differ; one
  RbxPathParent per remaining `from` segment, then the `to` tail.
- `getWarnings()` (L143), `isGame` field, `getPartitions()` (L341).
- NOTE `isolatedContainers` is fixed to the defaults — no config extension in 1.1.0.

### 4.2 PathTranslator (PathTranslator.js line refs)

`new PathTranslator(rootDir, outDir, buildInfoOutputPath, declaration, useLuauExtension)`.
roblox-ts constructs it in `Project/functions/createPathTranslator.ts` L8–18:
`rootDir = findAncestorDir([program.getCommonSourceDirectory(), ...getRootDirs(compilerOptions)])`,
`outDir = compilerOptions.outDir`, `buildInfoPath = ts.getTsBuildInfoEmitOutputFilePath(...)`,
`declaration`, `useLuauExtension = data.projectOptions.luau` (DEFAULT true ⇒ `.luau`,
constants.ts L54).
PathInfo model (L10–30): fileName + ALL dot-extensions as a stack (so `a.spec.ts` →
exts [".spec", ".ts"]; `extsPeek(1)` sees `.d` of `.d.ts`).

- `getOutputPath(filePath)` (L45–56): if last ext is `.ts`/`.tsx` and not `.d.*`: pop ext,
  `index` → `init` rename, push `.luau`; rebase rootDir→outDir. (`src/foo/index.ts` →
  `out/foo/init.luau`.)
- `getImportPath(filePath, isNodeModule=false)` (L120–134): pops `.ts`/`.tsx` AND a `.d`
  beneath it, index→init, push `.luau`; if isNodeModule → join WITHOUT rebasing (node_modules
  d.ts files were already remapped to the shipped .lua location via nodeModulesPathMapping);
  else rebase rootDir→outDir.
- `getOutputDeclarationPath` (L57–65): → `.d.ts` in outDir.
- `getOutputTransformedPath` (L66–76): inserts `.transformed` ext.
- `getInputPaths(outPath)` (L77–119): reverse mapping (used by watch/cleanup) — `.luau`
  (non-index) → candidates `.ts`, `.tsx`, and for `init` also `index.ts(x)`; declaration
  reverse for `.d.ts`; plus identity.
Upstream callers: getOutputPath (TransformState ctor L63, createRuntimeLibImport L230,
transformSourceFile L216, getProjectImportParts L149, compileFiles L194);
getImportPath (getImportParts L194/L200); getOutputDeclarationPath/getInputPaths only in
Project cleanup/copy machinery (Phase 4).

---

## 5. Runtime lib emission

### 5.1 runtimeLibRbxPath computation — `Project/functions/compileFiles.ts` L80–98

```ts
const projectType = data.projectOptions.type ?? inferProjectType(data, rojoResolver);
// inferProjectType (L27–35): isPackage (no "name" in default.project.json? — actually data.isPackage
//   = package.json has no `"private": true`... see createProjectData; Package wins) → Package;
//   rojoResolver.isGame → Game; else Model.
if (projectType !== ProjectType.Package && data.rojoConfigPath === undefined)
	→ emitResultFailure("Non-package projects must have a Rojo project file!")
let runtimeLibRbxPath: RbxPath | undefined;
if (projectType !== ProjectType.Package) {
	runtimeLibRbxPath = rojoResolver.getRbxPathFromFilePath(path.join(data.projectOptions.includePath, "RuntimeLib.lua"));
	if (!runtimeLibRbxPath) → emitResultFailure("Rojo project contained no data for include folder!")
	else if (rojoResolver.getNetworkType(runtimeLibRbxPath) !== NetworkType.Unknown)
		→ emitResultFailure("Runtime library cannot be in a server-only or client-only container!")
	else if (rojoResolver.isIsolated(runtimeLibRbxPath))
		→ emitResultFailure("Runtime library cannot be in an isolated container!")
}
```

Packages get `runtimeLibRbxPath = undefined` ⇒ `_G[script]` form. Note `.lua` filename here;
RojoResolver's convertToLuau normalizes. rojoResolver itself: `data.rojoConfigPath ?
RojoResolver.fromPath(...) : RojoResolver.synthetic(outDir)` (L61–63).

### 5.2 createRuntimeLibImport — `TSTransformer/classes/TransformState.ts` L202–265

Emitted by transformSourceFile L227–230 (`if (state.usesRuntimeLib)
luau.list.push(headerStatements, state.createRuntimeLibImport(node))`) AFTER the
`-- Compiled with roblox-ts v3.0.0` comment; headerStatements are unshifted after handleExports
and after `--!directive` comments are pulled to the very top (L232–240).

- **Game** (L205–228): `local TS = require(game:GetService("<p0>"):WaitForChild("<p1>"):...)`
  — chain of `WaitForChild` MethodCallExpressions over the runtimeLibRbxPath tail, wrapped in
  `require`. (Imports use plain strings; the runtime lib require is the ONE place WaitForChild
  is emitted literally.)
- **Model** (L229–253): per-FILE relative chain:

  ```ts
  const sourceOutPath = this.pathTranslator.getOutputPath(sourceFile.fileName);
  const rbxPath = this.rojoResolver.getRbxPathFromFilePath(sourceOutPath);
  if (!rbxPath) → errors.noRojoData(sourceFile, relative(projectPath, sourceOutPath), false); local TS = nil
  // local TS = require(script.<chain>) where chain = RojoResolver.relative(rbxPath, runtimeLibRbxPath)
  //   with RbxPathParent → "Parent", everything as PROPERTY accesses (no WaitForChild)
  ```

- **Package** (L254–264): `local TS = _G[script]` (ComputedIndexExpression `_G[script]`);
  comment in source: "we pass RuntimeLib access to packages via `_G[script] = TS`".

### 5.3 Ground truth — differential fixture project (Model type)

`testdata/diff/project/default.project.json`:
`{"name":"fixture","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}`
— tree root maps the out dir; rbxPath of `out/X.luau` = ["fixture","X"]; RuntimeLib =
["fixture","include","RuntimeLib"]; relative = [Parent,"include","RuntimeLib"].
Compiled 2026-06-06 with real rbxtsc 3.0.0 (`.\node_modules\.bin\rbxtsc.cmd --type model`),
scratch cleaned up after. **src/_scratch_util.ts**:

```ts
export const VALUE = 123;
export function greet(name: string) {
	return `Hello, ${name}`;
}
export default function () {
	return VALUE;
}
```

**src/_scratch_main.ts**:

```ts
import greeter, { VALUE, greet as g } from "./_scratch_util";
const x = VALUE + greeter();
print(g("world"), x);
export {};
```

**out/_scratch_main.luau** (verbatim):

```lua
-- Compiled with roblox-ts v3.0.0
local TS = require(script.Parent.include.RuntimeLib)
local __scratch_util = TS.import(script, script.Parent, "_scratch_util")
local greeter = __scratch_util.default
local VALUE = __scratch_util.VALUE
local g = __scratch_util.greet
local x = VALUE + greeter()
print(g("world"), x)
return nil
```

**out/_scratch_util.luau** (verbatim):

```lua
-- Compiled with roblox-ts v3.0.0
local VALUE = 123
local function greet(name)
	return `Hello, {name}`
end
local function default()
	return VALUE
end
return {
	greet = greet,
	default = default,
	VALUE = VALUE,
}
```

Confirms: uses=3 ⇒ temp `__scratch_util` (tempId named `_scratch_util`); binding order
default→named in clause order; real-default module ⇒ `.default`; `export {}` ⇒ no exports ⇒
ModuleScript `return nil` (transformSourceFile L213–220 — CHECKER-free:
pathTranslator.getOutputPath + rojoResolver.getRbxTypeFromFilePath == ModuleScript);
util's return-table order = functions-first then pos (greet, default, VALUE) matching rotor's
existing sort; anonymous `export default function` named literal `default` (P2b §1.2).
Star-export ground truth in §2.4.

---

## 6. NewExpression

### 6.1 transformNewExpression — `nodes/expressions/transformNewExpression.ts` L10–24

```ts
export function transformNewExpression(state: TransformState, node: ts.NewExpression) {
	validateNotAnyType(state, node.expression);                                  // CHECKER (P2 §6)

	const symbol = getFirstConstructSymbol(state, node.expression);              // CHECKER
	if (symbol) {
		const macro = state.services.macroManager.getConstructorMacro(symbol);
		if (macro) return macro(state, node);
	}

	const expression = convertToIndexableExpression(transformExpression(state, node.expression));
	const args = node.arguments ? ensureTransformOrder(state, node.arguments) : [];
	return luau.call(luau.property(expression, "new"), args);
}
```

Fallback covers user classes AND `new Instance("Part")` → `Instance.new("Part")` — there is
NO separate Instance macro (verified: no Instance entry anywhere in macros/). `new C` without
parens → empty args. `getFirstConstructSymbol` (`util/types.ts` L226–242): CHECKER —
`state.getType(expression)`, then `type.symbol.getDeclarations()`, first InterfaceDeclaration,
first `ts.isConstructSignatureDeclaration` member → `member.symbol`. Registration side
(`classes/MacroManager.ts` L112–117): for each CONSTRUCTOR_MACROS name, global symbol by name
(`SymbolFlags.Interface`), first interface declaration, its construct-signature symbol — i.e.
the macro key IS the construct-signature symbol of the GLOBAL interface, so user-defined
shadowing types won't collide. Guards already in rotor: `noConstructorMacroWithoutNew` /
`noMacroExtends` fire from transformIdentifier when a constructor-macro identifier is indexed
without `new` (transformIdentifier.ts L140–149).

### 6.2 Constructor macro table — `macros/constructorMacros.ts` L98–106

```ts
export const CONSTRUCTOR_MACROS: MacroList<ConstructorMacro> = {
	ArrayConstructor, SetConstructor, MapConstructor,
	WeakSetConstructor: (state, node) => wrapWeak(state, node, SetConstructor),
	WeakMapConstructor: (state, node) => wrapWeak(state, node, MapConstructor),
	ReadonlyMapConstructor: MapConstructor,
	ReadonlySetConstructor: SetConstructor,
};
```

- **ArrayConstructor** (L16–22): args present → `table.create(<args>)` (`new Array<T>(8)` →
  `table.create(8)`; second arg fills); else `{}`.
- **SetConstructor** (L24–53): no args → `luau.set()` (`{}` rendering with `[v] = true`
  fields); arg is ArrayLiteral WITHOUT spreads → set literal from
  `ensureTransformOrder(arg.elements)`; else `local set = {}` pushToVar("set") + prereq
  `for _, _v in <expr> do set[_v] = true end` (ForStatement ids `[tempId(), valueId("v")]`),
  returns the temp.
- **MapConstructor** (L55–96): no args → `luau.map()`; transformed arg is a luau Array whose
  members are ALL Arrays (i.e. `[[k, v], ...]` literal after transform) → map literal of
  (members.head, members.head.next) pairs; else `local map = {}` pushToVar("map") + prereq
  `for _, _v in <transformed> do map[_v[1]] = _v[2] end` (ComputedIndex with numbers 1/2).
  Note the decision is on the TRANSFORMED luau AST (`luau.isArray` + every member isArray),
  not the TS AST — spreads/iterables fall to the loop path.
- **wrapWeak** (L9–14): `setmetatable(<macro result>, { __mode = "k" })` — `__mode = "k"` for
  BOTH WeakSet and WeakMap (upstream quirk: weak KEYS only; port verbatim).

CHECKER inventory for §6: state.getType, type.symbol, symbol.getDeclarations,
construct-signature member symbol lookup; MacroManager global-symbol-by-name (rotor's
centralized MacroManager prerequisite — memory note — should land before this).

---

## 7. Multi-file compile implications — what CompileFile must become

Current rotor (`internal/compile/compile.go` L27–69): one Program per CompileFile call, one
file transformed, no Rojo/PathTranslator/runtime-lib inputs;
`transformer.NewState(program, chk, sourceFile, diagService, multiState)`.
Upstream model (compileFiles.ts L49–213): ONE program; rojoResolver + pkgRojoResolvers +
nodeModulesPathMapping + projectType + runtimeLibRbxPath computed ONCE; per file: pre-emit
diagnostics → `new TransformState(...13 args)` → transformSourceFile → render → write to
`pathTranslator.getOutputPath(fileName)`.

Minimal Phase 3 changes:

1. **CompileProject(projectDir) → map[outRelPath]string**: build Program once (reuse
   SanitizeFS pipeline); enumerate `program.SourceFiles()` filtered to non-declaration files
   under rootDir (upstream uses `getChangedSourceFiles`/all root files; diff harness wants
   all); keep CompileFile as a thin wrapper compiling the project and returning one file
   (manifest stays per-fixture; multi-file fixtures compare N outputs).
2. **Project context struct** (Go analog of ProjectData + compileFiles locals): projectPath,
   nodeModulesPath, rojoConfigPath, includePath, projectType, RojoResolver, []pkgRojoResolvers,
   PathTranslator, nodeModulesPathMapping, runtimeLibRbxPath. Construct per §4–5; the four
   emitResultFailure texts (§8) become hard errors.
3. **TransformState additions**: PathTranslator, RojoResolver, pkgRojoResolvers,
   nodeModulesPathMapping, runtimeLibRbxPath, projectType, projectData ref, `resolver`
   (= `chk.GetEmitResolver()`), `hasExportFrom`/`hasExportEquals` already exist,
   `moduleIdBySymbol` (per-file fine — only file+namespace symbols), and MultiState keeps the
   cross-file `getModuleExportsCache`. CRITICAL ordering: run
   `program.GetSemanticDiagnostics(ctx, sourceFile)` for EACH file before transforming it so
   `aliasSymbolLinks.referenced` is populated for that file's aliases (§1.4); the shared
   checker accumulates marks. Keep one checker for the whole program (per-file
   `GetTypeChecker` release pattern must become program-scoped) — also satisfies the memory
   note to retire per-file Program creation before watch/concurrency.
4. **Module specifier → SourceFile**: §3.2 tsgo APIs (Checker.ResolveExternalModuleName,
   Program.GetResolvedModuleFromModuleSpecifier / GetModeForUsageLocation+GetResolvedModule).
5. **Diff harness**: golden side already produced by real rbxtsc over the whole project;
   change `internal/diff/diff_test.go` to call CompileProject once, then byte-compare every
   enabled fixture's out file; multi-file fixtures = N goldens, one manifest entry listing
   them (or enable by basename as today — both _scratch files demonstrated independent
   outputs).
6. **Go ports of the two libs**: RojoResolver (§4.1, 345 lines incl. JSON-schema validation —
   replace ajv with a hand validator; TOML detection only needs ext checks) and PathTranslator
   (§4.2, 136 lines). Pure path/string code; vendor upstream repos for citation.

---

## 8. Diagnostics (import/export/new related)

From `Shared/diagnostics.ts` (line refs) — ALL already present in rotor's
`internal/transformer/diagnostics.go` (66-factory set):

- L188 `noModuleSpecifierFile` — "Could not find file for import. Did you forget to `npm install`?" (getImportParts L187)
- L189 `noInvalidModule` — "You can only use npm scopes that are listed in your typeRoots." (L85)
- L190 `noUnscopedModule` — "You cannot use modules directly under node_modules." (L80)
- L191 `noNonModuleImport` — "Cannot import a non-ModuleScript!" (L145)
- L192 `noIsolatedImport` — "Attempted to import a file inside of an isolated container from outside!" (L176)
- L193–196 `noServerImport` — "Cannot import a server file from a shared or client location!" + suggestion (L166)
- L206–209 `noRojoData(path, isPackage)` — context error; suggestion only when isPackage (5 call sites: getImportParts L204, getNodeModulesImportParts L93, getProjectImportParts L153, createRuntimeLibImport L234)
- L210–223 `noPackageImportWithoutScope(path, rbxPath)` — context error w/ node_modules tree suggestion (L122)
- L138 `noExportAssignmentLet` (transformExportAssignment L48)
- (P2-existing) `noNonStringModuleSpecifier` (transformImportExpression L13)
- warnings L246–248 `runtimeLibUsedInReplicatedFirst` (state.TS, TransformState L193)
- L224–229 `incorrectFileName`, L230–234 `rojoPathInSrc` — project-level errorText (no node);
  rotor has them (diagnostics.go L369–385). Raised by checkFileName/checkRojoConfig
  (compileFiles L69–75) — Phase 3 should wire checkRojoConfig + checkFileName when
  CompileProject lands.
**Not in the 66** (plain `createTextDiagnostic` emit failures, compileFiles L83, L92, L94, L96
— need Go equivalents as hard errors): "Non-package projects must have a Rojo project file!",
"Rojo project contained no data for include folder!", "Runtime library cannot be in a
server-only or client-only container!", "Runtime library cannot be in an isolated container!".
NewExpression adds no new diagnostics (guards `noConstructorMacroWithoutNew`, `noMacroExtends`,
`noAny` all exist).

---

## 9. Inventory

### 9.1 Reference files digested (read in full)

`nodes/statements/transformImportDeclaration.ts`, `transformImportEqualsDeclaration.ts`,
`transformExportDeclaration.ts`, `transformExportAssignment.ts`,
`nodes/transformSourceFile.ts` (handleExports + runtime-lib/header assembly),
`nodes/statements/transformVariableStatement.ts` L19–55 (transformVariable),
`nodes/expressions/transformNewExpression.ts`, `transformImportExpression.ts`,
`transformCallExpression.ts` L115–144 (import-call dispatch),
`transformIdentifier.ts` L140–176 (constructor-macro guards + export access),
`util/createImportExpression.ts`, `util/getSourceFileFromModuleSpecifier.ts`,
`util/cleanModuleName.ts`, `util/getOriginalSymbolOfNode.ts`, `util/isSymbolOfValue.ts`,
`util/createGetService.ts`, `util/traversal.ts` L45–59, `util/types.ts` L226–242,
`macros/constructorMacros.ts`, `macros/callMacros.ts` L46–61, `classes/MacroManager.ts`
L94–130, `classes/TransformState.ts` (resolver, TS(), createRuntimeLibImport, module-id +
exports caches, guessVirtualPath), `Shared/classes/Lazy.ts`, `Shared/constants.ts`,
`Shared/util/getCanonicalFileName.ts`, `Project/functions/compileFiles.ts`,
`createNodeModulesPathMapping.ts`, `createPathTranslator.ts`, `Shared/diagnostics.ts`
L186–249. Dist sources: `@roblox-ts/rojo-resolver@1.1.0 out/RojoResolver.{d.ts,js}`,
`@roblox-ts/path-translator@1.1.0 out/PathTranslator.{d.ts,js}` (complete implementations).
tsgo: `checker/emitresolver.go` (IsReferencedAliasDeclaration L688, IsValueAliasDeclaration
L714, MarkLinkedReferencesRecursively L799), `checker/checker.go` L922/L28500/L31857,
`checker/exports.go` L104–110, `compiler/program.go` L468–485/L1509.

### 9.2 CHECKER call-site inventory (new in Phase 3)

- `resolver.isReferencedAliasDeclaration` — transformImportDeclaration.ts:18,29,71,105;
  transformExportDeclaration.ts:14. tsgo: `Checker.GetEmitResolver().IsReferencedAliasDeclaration`
  — EXISTS, requires the file checked first (GetSemanticDiagnostics).
- `typeChecker.getEmitResolver(sourceFile)` — TransformState.ts:61. tsgo: program-global
  `GetEmitResolver()`, no per-file check trigger.
- `typeChecker.resolveExternalModuleName` — getSourceFileFromModuleSpecifier.ts:7. tsgo:
  `Checker.ResolveExternalModuleName` (exports.go:104).
- `program.getModeForUsageLocation` / `program.getResolvedModule` / `program.getSourceFile` —
  getSourceFileFromModuleSpecifier.ts:13–16. tsgo: program.go:1509/468/477.
- `ts.resolveModuleName` fallback — getSourceFileFromModuleSpecifier.ts:28 ($getModuleTree
  only; defer).
- `typeChecker.getExportsOfModule` (via state.getModuleExports) — transformImportDeclaration.ts:74;
  transformSourceFile.ts:55,105. Already shimmed/sorted in rotor (order trap §2.4).
- `getSymbolAtLocation` + `ts.skipAlias` — getOriginalSymbolOfNode (import/export specifiers),
  transformImportEqualsDeclaration.ts:18–20, transformExportAssignment.ts:46–51,
  transformExportDeclaration.ts:18–19, getIgnoredExportSymbols, getSourceFileFromModuleSpecifier.ts:6,
  transformImportDeclaration.ts:73 (module file symbol).
- `program.getSymlinkCache().getSymlinkedDirectoriesByRealpath()` — TransformState.ts:368
  (guessVirtualPath; tsgo `symlinks` pkg; defer with `|| fileName` fallback).
- NewExpression: `state.getType`, `type.symbol`, declarations walk for construct signature
  (types.ts:226–242); MacroManager global-symbol lookups.
- `ts.isInsideNodeModules` — createImportExpression.ts:193 (reimplement: path contains
  node_modules segment).

### 9.3 Vendoring + sequencing notes

- VENDOR for Phase 3: github.com/roblox-ts/rojo-resolver @ 1.1.0 and
  github.com/roblox-ts/path-translator @ 1.1.0 into `reference/` (only dist JS available
  locally today; §4 captures full behavior with dist line refs).
- Order: Go RojoResolver + PathTranslator ports → CompileProject context (§7.2) →
  createImportExpression + transformImportDeclaration/Equals → export-from (+ revisit
  exports.go TODO with §2.4 ground truth) → runtime-lib emission (unlocks TS.* consumers:
  async/generator from P2b §8.3) → NewExpression/constructor macros (after MacroManager
  centralization).
- Runtime-lib entries surfaced here: `TS.import`, `TS.getModule`, `TS.Promise` (dynamic
  import). Files: `include/RuntimeLib.lua` shipped by upstream; fixture project already has
  `include/` wired in default.project.json.
- Fixture note: scratch outputs in §2.4/§5.3 were generated then deleted; when porting lands,
  promote them to real fixtures via `tools/oracle/oracle.ps1` + `internal/diff/manifest.go`.
