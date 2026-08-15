# Phase 2 Research Digest: TransformState, transformSourceFile, and per-file emission

Source of truth: vendored roblox-ts v3.0.0 at `reference/roblox-ts/src/` and luau-ast at `reference/luau-ast/src/`.
All file:line references are against those vendored copies. Every TypeChecker / ts-internal API call is flagged `CHECKER:`.

---

## 1. `TSTransformer/classes/TransformState.ts` (401 lines) — the per-file transform state

One `TransformState` is constructed **per source file** (`Project/functions/compileFiles.ts:160-174`). It owns prereq-statement stacking, hoisting bookkeeping, runtime-lib usage tracking, type caching, and module-export id mapping.

### 1.1 Constructor (lines 45-66)

```ts
constructor(
    public readonly program: ts.Program,
    public readonly data: ProjectData,
    public readonly services: TransformServices,        // { macroManager } (TSTransformer/types.ts:3-5)
    public readonly pathTranslator: PathTranslator,
    public readonly multiTransformState: MultiTransformState,
    public readonly compilerOptions: ts.CompilerOptions,
    public readonly rojoResolver: RojoResolver,
    public readonly pkgRojoResolvers: Array<RojoResolver>,
    public readonly nodeModulesPathMapping: Map<string, string>,
    public readonly runtimeLibRbxPath: RbxPath | undefined,
    public readonly typeChecker: ts.TypeChecker,
    public readonly projectType: ProjectType,
    sourceFile: ts.SourceFile,
) {
    this.sourceFileText = sourceFile.getFullText();
    this.resolver = typeChecker.getEmitResolver(sourceFile);
    const sourceOutPath = this.pathTranslator.getOutputPath(sourceFile.fileName);
    const rbxPath = this.rojoResolver.getRbxPathFromFilePath(sourceOutPath);
    this.isInReplicatedFirst = rbxPath !== undefined && rbxPath[0] === "ReplicatedFirst";
}
```

- `CHECKER:` line 61 `typeChecker.getEmitResolver(sourceFile)` — **ts-internal** `EmitResolver`. Needs a tsgo equivalent. Its only consumers in the whole codebase (grep `.resolver`):
  - `resolver.getJsxFactoryEntity(node)` — transformJsx.ts:20, transformJsxFragment.ts:10
  - `resolver.getJsxFragmentFactoryEntity(node)` — transformJsxFragment.ts:18
  - `resolver.isReferencedAliasDeclaration(importClause|element)` — transformImportDeclaration.ts:18,29,71,105; transformExportDeclaration.ts:14 (used to elide unused imports/exports).
- `sourceFile.getFullText()` is captured into `private readonly sourceFileText` (line 24) — used only by `getLeadingComments`.

### 1.2 Fields (complete list)

| Field | Line | Type | Purpose |
|---|---|---|---|
| `sourceFileText` | 24 | `string` (private readonly) | full source text incl. leading trivia, for comment extraction |
| `hasExportEquals` | 25 | `boolean = false` | set `true` by `transformExportAssignment.ts:11` when `export = x` seen; changes export emission shape |
| `hasExportFrom` | 26 | `boolean = false` | set `true` by `transformExportDeclaration.ts:120` when `export ... from "..."` seen; forces `local exports = {}` form |
| `classIdentifierMap` | 28 | `Map<ts.ClassLikeDeclaration, luau.AnyIdentifier>` | maps a class decl to the luau id used for the class (class transform support) |
| `resolver` | 42 | `ts.EmitResolver` (public readonly) | see CHECKER above |
| `isInReplicatedFirst` | 43 | `boolean` (private) | computed in ctor; gates `runtimeLibUsedInReplicatedFirst` warning in `TS()` |
| `tryUsesStack` | 68 | `Array<TryUses>` (public readonly) | stack of `{usesReturn, usesBreak, usesContinue}` for try/catch flow-control rewriting |
| `prereqStatementsStack` | 99 | `Array<luau.List<luau.Statement>>` (public readonly) | THE core mechanism: stack of statement lists that prereqs are appended onto |
| `hoistsByStatement` | 180 | `Map<ts.Statement \| ts.CaseClause, Array<ts.Identifier>>` (public readonly) | per-statement list of identifiers needing a `local x` hoist emitted just before that statement |
| `isHoisted` | 181 | `Map<ts.Symbol, boolean>` (public readonly) | memo: symbol already decided for hoisting (presence check, value always `true`) |
| `getTypeCache` | 183 | `Map<ts.Node, ts.Type>` (private) | cache for `getType` |
| `usesRuntimeLib` | 188 | `boolean = false` | set ONLY by `TS()` (line 190 — the only assignment in the entire repo); gates runtime-lib import emission |
| `moduleIdBySymbol` | 339 | `Map<ts.Symbol, luau.AnyIdentifier>` (private readonly) | module symbol -> luau id holding that module's exports table (source file symbol -> `exports`; namespaces -> their container id) |
| `symbolToIdMap` | 387 | `Map<ts.Symbol, luau.TemporaryIdentifier>` (public) | symbol -> replacement temp id (used by `transformIdentifierDefined` for renamed/captured vars) |
| `classElementToObjectKeyMap` | 390 | `Map<ts.ClassElement, luau.SimpleTypes>` (private) | remembers the `key` in `obj[key] = value` for class elements so decorators etc. can refer to it later |

### 1.3 Debug helpers (lines 30-40)

```ts
debugRender(node)      { const state = new RenderState(); solveTempIds(state, node); return render(state, node); }
debugRenderList(list)  { const state = new RenderState(); solveTempIds(state, list); return renderStatements(state, list); }
```

Dev-only; port optional.

### 1.4 tryUses stack (lines 68-97)

```ts
pushTryUsesStack(): TryUses {
    const tryUses = { usesReturn: false, usesBreak: false, usesContinue: false };
    this.tryUsesStack.push(tryUses);
    return tryUses;
}
markTryUses(property: keyof TryUses) {
    if (this.tryUsesStack.length !== 0) {
        this.tryUsesStack[this.tryUsesStack.length - 1][property] = true;   // top of stack only
    }
}
popTryUsesStack() { this.tryUsesStack.pop(); }
```

`TryUses` interface is `TSTransformer/types.ts:7-11`. Used by try/catch transform (return/break/continue must be tunneled out of the pcall closure). `markTryUses` is a silent no-op when stack is empty.

### 1.5 Prereq statement stack (lines 99-178) — THE core mechanic

Expression transforms can't always produce a single Luau expression; side-effectful sub-steps are emitted as "prerequisite statements" appended to the innermost captured list.

```ts
public readonly prereqStatementsStack = new Array<luau.List<luau.Statement>>();

prereq(statement: luau.Statement) {
    luau.list.push(this.prereqStatementsStack[this.prereqStatementsStack.length - 1], statement);
}
prereqList(statements: luau.List<luau.Statement>) {
    luau.list.pushList(this.prereqStatementsStack[this.prereqStatementsStack.length - 1], statements);
}
pushPrereqStatementsStack(): luau.List<luau.Statement> {
    const prereqStatements = luau.list.make<luau.Statement>();
    this.prereqStatementsStack.push(prereqStatements);
    return prereqStatements;
}
popPrereqStatementsStack(): luau.List<luau.Statement> {
    const poppedValue = this.prereqStatementsStack.pop();
    assert(poppedValue);              // crash if stack empty
    return poppedValue;
}
capturePrereqs(callback: () => void): luau.List<luau.Statement> {
    this.pushPrereqStatementsStack();
    callback();
    return this.popPrereqStatementsStack();
}
capture<T>(callback: () => T): [value: T, prereqs: luau.List<luau.Statement>] {
    let value!: T;
    const prereqs = this.capturePrereqs(() => (value = callback()));
    return [value, prereqs];
}
noPrereqs(callback: () => luau.Expression): luau.Expression {
    let expression!: luau.Expression;
    const statements = this.capturePrereqs(() => (expression = callback()));
    assert(luau.list.isEmpty(statements));    // hard assert: callback must not create prereqs
    return expression;
}
```

Notes for the Go port:

- `prereq`/`prereqList` write to the **top** of the stack; calling them with an empty stack is undefined (JS would index `[-1]` -> `undefined.push` crash). In practice every `transformStatement` call site is wrapped in `capture` (transformStatementList.ts:42), so a list is always present during statement transformation.
- `luau.list` is a doubly-linked list (`head`/`tail`/`ListNode{prev,next,value}`); `pushList` splices the entire second list onto the first in O(1) and the source list must not be reused.

### 1.6 getLeadingComments (lines 139-153)

```ts
getLeadingComments(node: ts.Node) {
    const commentRanges = ts.getLeadingCommentRanges(this.sourceFileText, node.pos) ?? [];
    return luau.list.make(
        ...commentRanges.map(commentRange =>
            luau.comment(
                this.sourceFileText.substring(
                    commentRange.pos + 2,                                       // skip "--" => actually skips "//" or "/*"
                    commentRange.kind === ts.SyntaxKind.SingleLineCommentTrivia
                        ? commentRange.end
                        : commentRange.end - 2,                                  // strip trailing "*/"
                ),
            ),
        ),
    );
}
```

- `CHECKER:` `ts.getLeadingCommentRanges(text, node.pos)` — TS scanner API (public). tsgo needs equivalent leading-trivia comment scanning over `[fullStart, start)`.
- `// foo` -> comment text `foo` (keeps space, drops `//`); `/* foo */` -> ` foo ` (drops both delimiters). Multi-line text renders as a `--[[ ... ]]` block (renderComment, see §8.3).

### 1.7 Hoist maps (lines 180-181)

Declared here; populated by `checkIdentifierHoist` (transformIdentifier.ts) and `checkVariableHoist` (util/checkVariableHoist.ts); consumed by `createHoistDeclaration` from transformStatementList. Full system in §7.

### 1.8 getType (lines 183-186)

```ts
private getTypeCache = new Map<ts.Node, ts.Type>();
public getType(node: ts.Node) {
    return getOrSetDefault(this.getTypeCache, node, () => this.typeChecker.getTypeAtLocation(skipUpwards(node)));
}
```

- `CHECKER:` `typeChecker.getTypeAtLocation(...)`.
- Important subtlety: the type is taken at `skipUpwards(node)` — i.e. it climbs through enclosing `NonNullExpression` / `ParenthesizedExpression` / `AsExpression` / `TypeAssertionExpression` / `SatisfiesExpression` wrappers (traversal.ts:27-41) so `(x as Foo)!` queries the type of the outermost wrapper. The cache key is the **original** node, not the skipped one.
- `getOrSetDefault` (Shared/util/getOrSetDefault.ts:7-14): `map.get(key) ?? (set(key, getDefaultValue()), value)` — note it re-computes when the stored value is `undefined`.

### 1.9 usesRuntimeLib + TS() (lines 188-197)

```ts
public usesRuntimeLib = false;
public TS(node: ts.Node, name: string) {
    this.usesRuntimeLib = true;
    if (this.projectType === ProjectType.Game && this.isInReplicatedFirst) {
        DiagnosticService.addDiagnostic(warnings.runtimeLibUsedInReplicatedFirst(node));
    }
    return luau.property(luau.globals.TS, name);
}
```

- Every runtime-lib call in the transformer goes through `state.TS(node, "async")` etc., yielding the expression `TS.async`. This is the **only** place `usesRuntimeLib` is set (verified by grep over the whole repo).
- Warning text (Shared/diagnostics.ts:246-248): "This statement would generate a call to the runtime library. The runtime library should not be used from ReplicatedFirst." Emitted once per `TS()` call (not deduped).
- `node` parameter exists solely for the warning's source location.

### 1.10 createRuntimeLibImport (lines 199-265) — exact emitted forms

```ts
createRuntimeLibImport(sourceFile: ts.SourceFile): luau.VariableDeclaration
```

Three cases:

1. **`runtimeLibRbxPath` defined AND projectType === Game** (lines 204-228):
   - `serviceName = runtimeLibRbxPath[0]` (assert non-empty).
   - Start with `createGetService(serviceName)` = `game:GetService("<serviceName>")` (util/createGetService.ts:7-13, a MethodCallExpression on `luau.globals.game`).
   - For each remaining path segment `i = 1..len-1`: wrap in `:WaitForChild("<segment>")` MethodCallExpression.
   - Wrap whole chain in `require(...)`.
   - Emit `local TS = require(game:GetService("ReplicatedStorage"):WaitForChild("rbxts_include"):WaitForChild("RuntimeLib"))` (shape; actual names from the rojo path).

2. **`runtimeLibRbxPath` defined AND projectType !== Game** (i.e. Model) (lines 229-253):
   - Compute this file's own `rbxPath` via `pathTranslator.getOutputPath(sourceFile.fileName)` -> `rojoResolver.getRbxPathFromFilePath(...)`.
   - If no rbxPath: add error `noRojoData(sourceFile, relative(projectPath, sourceOutPath), false)` and emit `local TS = nil` (`right: luau.none()` — None renders as error normally; here it produces `local TS` with no initializer? No — `luau.none()` as VariableDeclaration.right renders as just `local TS` because renderVariableDeclaration skips None) — for the Go port treat it as "declaration without value"; compilation has already failed via the diagnostic.
   - Else: `RojoResolver.relative(rbxPath, runtimeLibRbxPath)` gives a relative path of segments where parent steps are the sentinel `RbxPathParent`; map each parent step to the string `"Parent"` (`PARENT_FIELD`, Shared/constants.ts:33), then build `propertyAccessExpressionChain(luau.globals.script, segments)` and wrap in `require`:
     `local TS = require(script.Parent.Parent.include.RuntimeLib)` (shape).

3. **`runtimeLibRbxPath === undefined`** (Package projects; lines 254-264):

   ```ts
   // we pass RuntimeLib access to packages via `_G[script] = TS`
   return local TS = _G[script]   // ComputedIndexExpression(_G, script)
   ```

   Emits exactly: `local TS = _G[script]`.

`runtimeLibRbxPath` is computed in compileFiles.ts:86-98: only for non-Package projects, as `rojoResolver.getRbxPathFromFilePath(path.join(includePath, "RuntimeLib.lua"))`, with hard failures if missing / in server-or-client-only container / in isolated container.

### 1.11 pushToVar family (lines 267-306)

```ts
pushToVar(expression: luau.Expression | undefined, name?: string): luau.TemporaryIdentifier {
    const temp = luau.tempId(name || (expression && valueToIdStr(expression)));
    this.prereq(luau.create(luau.SyntaxKind.VariableDeclaration, { left: temp, right: expression }));
    return temp;
}
```

- `expression` may be `undefined` -> emits `local _temp` with no value (used to pre-declare).
- Temp-id name hint: explicit `name`, else `valueToIdStr(expression)` (util/valueToIdStr.ts:25-32):
  - Identifier `X` -> `"x"` (uncapitalize first letter); PropertyAccess `A.B` -> `"b"`; CallExpression `X.new()` -> `"x"`; anything else -> `""` (anonymous temp). Result must pass `luau.isValidIdentifier` else `""`.
  - Final rendered names are resolved later by `solveTempIds` (suffix-dedupe `_name`, `_name_1`, ...).

```ts
pushToVarIfComplex<T>(expression: T, name?: string): Extract<T, luau.SimpleTypes> | luau.TemporaryIdentifier {
    if (luau.isSimple(expression)) return expression;
    return this.pushToVar(expression, name);
}
```

- `luau.isSimple` kind set (luau-ast/src/LuauAST/impl/typeGuards.ts:89-97): **Identifier, TemporaryIdentifier, NilLiteral, TrueLiteral, FalseLiteral, NumberLiteral, StringLiteral**.

```ts
pushToVarIfNonId<T>(expression: T, name?: string): luau.AnyIdentifier {
    if (luau.isAnyIdentifier(expression)) return expression;
    return this.pushToVar(expression, name);
}
```

- `luau.isAnyIdentifier` = Identifier | TemporaryIdentifier (typeGuards.ts:10).

(For reference, `isSimplePrimitive` used by ensureTransformOrder = the literal kinds only, no identifiers: typeGuards.ts:99-105.)

### 1.12 Module exports helpers (lines 308-364)

```ts
getModuleExports(moduleSymbol) {
    return getOrSetDefault(this.multiTransformState.getModuleExportsCache, moduleSymbol,
        () => this.typeChecker.getExportsOfModule(moduleSymbol));   // CHECKER: getExportsOfModule
}

getModuleExportsAliasMap(moduleSymbol) {  // cached in multiTransformState.getModuleExportsAliasMapCache
    const aliasMap = new Map<ts.Symbol, string>();
    for (const exportSymbol of this.getModuleExports(moduleSymbol)) {
        const originalSymbol = ts.skipAlias(exportSymbol, this.typeChecker);   // CHECKER: ts.skipAlias (internal helper: follows Alias-flagged symbols via getAliasedSymbol)
        const declaration = exportSymbol.getDeclarations()?.[0];
        if (declaration && ts.isExportSpecifier(declaration)) {
            aliasMap.set(originalSymbol, declaration.name.text);    // `export { a as b }` -> original -> "b"
        } else {
            aliasMap.set(originalSymbol, exportSymbol.name);
        }
    }
    return aliasMap;
}

private getModuleSymbolFromNode(node) {
    const moduleAncestor = getModuleAncestor(node);   // nearest SourceFile or ModuleDeclaration ancestor (traversal.ts:57-59)
    const exportSymbol = this.typeChecker.getSymbolAtLocation(   // CHECKER: getSymbolAtLocation
        ts.isSourceFile(moduleAncestor) ? moduleAncestor : moduleAncestor.name);
    assert(exportSymbol);
    return exportSymbol;
}

private getModuleIdFromSymbol(moduleSymbol) { const id = this.moduleIdBySymbol.get(moduleSymbol); assert(id); return id; }
setModuleIdBySymbol(moduleSymbol, moduleId) { this.moduleIdBySymbol.set(moduleSymbol, moduleId); }
getModuleIdFromNode(node) { return this.getModuleIdFromSymbol(this.getModuleSymbolFromNode(node)); }

getModuleIdPropertyAccess(idSymbol): luau.PropertyAccessExpression | undefined {
    if (idSymbol.valueDeclaration) {
        const moduleSymbol = this.getModuleSymbolFromNode(idSymbol.valueDeclaration);
        const alias = this.getModuleExportsAliasMap(moduleSymbol).get(idSymbol);
        if (alias) return luau.property(this.getModuleIdFromSymbol(moduleSymbol), alias);   // e.g. exports.foo
    }
    // implicitly returns undefined
}
```

This is how `export let x` reads/writes become `exports.x` (see transformIdentifier.ts:161-171).

### 1.13 guessVirtualPath (lines 366-385)

Reverse-symlink lookup so pnpm-style installs map real paths back to virtual node_modules paths:

```ts
guessVirtualPath(fsPath: string): string | undefined {
    const reverseSymlinkMap = this.program.getSymlinkCache?.().getSymlinkedDirectoriesByRealpath();  // CHECKER: ts-internal program.getSymlinkCache
    if (!reverseSymlinkMap) return;
    const original = fsPath;
    while (true) {
        const parent = ts.ensureTrailingDirectorySeparator(path.dirname(fsPath));   // CHECKER: ts-internal path util
        if (fsPath === parent) break;
        fsPath = parent;
        const symlink = reverseSymlinkMap.get(
            ts.toPath(fsPath, this.program.getCurrentDirectory(), getCanonicalFileName))?.[0];   // CHECKER: ts-internal ts.toPath
        if (symlink) return path.join(symlink, path.relative(fsPath, original));
    }
}
```

Map keys have trailing slashes. Walks ancestors of `fsPath` upward; on a hit, rejoins the relative remainder onto the symlink (virtual) path.

### 1.14 classElementToObjectKeyMap (lines 389-399)

```ts
setClassElementObjectKey(classElement, identifier) { assert(!map.has(classElement)); map.set(...); }   // double-set is a hard assert
getClassElementObjectKey(classElement) { return map.get(classElement); }   // undefined ok
```

### CHECKER summary for TransformState

- L61 `typeChecker.getEmitResolver(sourceFile)` (**internal**) + resolver methods `getJsxFactoryEntity`, `getJsxFragmentFactoryEntity`, `isReferencedAliasDeclaration`.
- L140 `ts.getLeadingCommentRanges` (scanner, public).
- L185 `typeChecker.getTypeAtLocation`.
- L310 `typeChecker.getExportsOfModule`.
- L318 `ts.skipAlias` (**internal**; ≈ `symbol.flags & Alias ? checker.getAliasedSymbol(symbol) : symbol`, transitively).
- L332 `typeChecker.getSymbolAtLocation`.
- L368-379 `program.getSymlinkCache` / `ts.ensureTrailingDirectorySeparator` / `ts.toPath` (**all internal**).

---

## 2. Per-file emission shape

### 2.1 `TSTransformer/index.ts` (4 lines)

Re-exports only: `MacroManager`, `MultiTransformState`, `TransformState` classes plus `transformSourceFile`.

### 2.2 `nodes/transformSourceFile.ts` (243 lines) — full flow

`transformSourceFile(state, node: ts.SourceFile): luau.List<luau.Statement>` (lines 203-243):

1. **Module symbol** (204-206): `symbol = typeChecker.getSymbolAtLocation(node)` (`CHECKER:` — requires the file to be a module; `assert(symbol)` hard-crashes on non-module files, but roblox-ts validates modules elsewhere). Then `state.setModuleIdBySymbol(symbol, luau.globals.exports)` — the file's module id is the identifier `exports`.
2. **Statements** (209): `statements = transformStatementList(state, node, node.statements, undefined)` (see §7.3 — handles per-statement capture, comments, hoists).
3. **Exports** (211): `handleExports(state, node, symbol, statements)` (below).
4. **`return nil` for valueless ModuleScripts** (213-220):

   ```ts
   const lastStatement = getLastNonCommentStatement(statements.tail);   // walks .prev past luau.isComment nodes (lines 191-196)
   if (!lastStatement || !luau.isReturnStatement(lastStatement.value)) {
       const outputPath = state.pathTranslator.getOutputPath(node.fileName);
       if (state.rojoResolver.getRbxTypeFromFilePath(outputPath) === RbxType.ModuleScript) {
           luau.list.push(statements, luau.create(luau.SyntaxKind.ReturnStatement, { expression: luau.nil() }));
       }
   }
   ```

   I.e. plain `return nil` is appended only when (a) the output file maps to a ModuleScript per rojo, and (b) the last non-comment statement isn't already a return.
5. **Header** (222-230):

   ```ts
   const headerStatements = luau.list.make<luau.Statement>();
   luau.list.push(headerStatements, luau.comment(` Compiled with roblox-ts v${COMPILER_VERSION}`));
   if (state.usesRuntimeLib) luau.list.push(headerStatements, state.createRuntimeLibImport(node));
   ```

   Note the comment text starts with a **space**, so it renders `-- Compiled with roblox-ts v3.0.0`. `COMPILER_VERSION` = package.json version (Shared/constants.ts:9). The header is added **here in the transformer**, not in compileFiles/renderAST.
6. **Luau directive comment hoisting** (232-237):

   ```ts
   const directiveComments = luau.list.make<luau.Statement>();
   while (statements.head && luau.isComment(statements.head.value) && statements.head.value.text.startsWith("!")) {
       luau.list.push(directiveComments, luau.list.shift(statements)!);
   }
   ```

   Any run of leading comments whose text begins with `!` (i.e. `--!strict`, `--!native`, `--!optimize 2` — sourced from leading `//!...` TS comments preserved by getLeadingComments) is moved above the header. Note: only comments already at the very head of the statement list qualify; the scan stops at the first non-`!` comment or non-comment.
   (Unrelated gate: `allowCommentDirectives` project option does NOT gate this hoisting — it gates a *diagnostic* on TS comment directives `@ts-ignore`/`@ts-expect-error`/`ts-nocheck` in `Project/preEmitDiagnostics/fileUsesCommentDirectives.ts:5-34`, default `false` per constants.ts:53 → using those directives is an error unless the flag is on.)
7. **Final assembly** (239-242):

   ```ts
   luau.list.unshiftList(statements, headerStatements);
   luau.list.unshiftList(statements, directiveComments);
   return statements;
   ```

**Final byte layout of a compiled file** (rendering by `renderAST` in compileFiles.ts:179; renderer details §8):

```text
--!strict                                                  (only if source had leading //!strict; 0+ lines)
-- Compiled with roblox-ts v3.0.0
local TS = require(game:GetService("ReplicatedStorage"):WaitForChild(...)...)   (only if usesRuntimeLib; form per §1.10)
<transformed statements, tab-indented, one per state.line>
return nil | return exports | return { ... }               (per §2.3; absent for Script/LocalScript outputs)
```

Every rendered statement line ends with `\n` (RenderState.line), so the file ends with exactly one trailing newline and there is **no** blank line separation anywhere unless comments were in the source. No BOM. Indentation is tabs.

### 2.3 handleExports (lines 94-189) — when each return form appears

Helpers first:

- `getExportPair(state, exportSymbol)` (14-31): returns `[exportName, luauId]`. For `export { a as b }` specifiers -> `["b", transformIdentifierDefined(propertyName ?? name)]`. Otherwise `[symbol.name, luau.id(name)]` where `name` is `symbol.name` EXCEPT a default-exported named function/class uses its declared name (`export default function foo` -> `["default", foo]`).
- `isExportSymbolFromExportFrom` (33-45): true if any declaration is an export specifier whose ExportDeclaration has a moduleSpecifier (`export { x } from "..."`).
- `getIgnoredExportSymbols` (47-67): symbols to skip — everything re-exported by `export * from "./m"` (the module's own exports, via `state.getModuleExports`), and the namespace id of `export * as ns from "./m"`. (`CHECKER:` uses `getOriginalSymbolOfNode` and `typeChecker.getSymbolAtLocation`.)
- `isExportSymbolOnlyFromDeclare` (78-86): true iff **every** declaration's ancestor statement has a `declare` modifier (so `export declare const x` is skipped, but `declare const x; export { x };` is not).

Main logic (94-189):

```ts
const ignoredExportSymbols = getIgnoredExportSymbols(state, sourceFile);
let mustPushExports = state.hasExportFrom;
const exportPairs: Array<[string, luau.AnyIdentifier]> = [];
if (!state.hasExportEquals) {
    for (const exportSymbol of state.getModuleExports(symbol)) {
        if (ignoredExportSymbols.has(exportSymbol)) continue;
        if (exportSymbol.flags & ts.SymbolFlags.Prototype) continue;            // ignore prototype
        if (isExportSymbolFromExportFrom(exportSymbol)) continue;               // already assigned where transformed
        const originalSymbol = ts.skipAlias(exportSymbol, state.typeChecker);   // CHECKER
        if (!isSymbolOfValue(originalSymbol)) continue;                         // types/interfaces not exported
        if (isSymbolMutable(state, originalSymbol)) { mustPushExports = true; continue; }  // export let -> handled via exports.x in transformIdentifier
        if (isExportSymbolOnlyFromDeclare(exportSymbol)) continue;
        exportPairs.push(getExportPair(state, exportSymbol));
    }
}
```

Then exactly one of four shapes:

1. **`hasExportEquals`** (132-142): `transformExportAssignment` already created `local exports = <value>` (or assignments). If the file's LAST TS statement is not itself an `export =` assignment, append `return exports`. (If it is the last statement, the return was already emitted by that transform.)
2. **`mustPushExports`** (143-167) — any `export ... from` or any mutable (`export let`) export: `luau.list.unshift(statements, local exports = {})` at the **top of the file** (before everything from the statement list but AFTER nothing — unshift happens before header is prepended, so it lands after `local TS` line in final output); then for each immutable pair append `exports.<key> = <id>`; finally `return exports`.
3. **only immutable exports** (168-188): append a single `return { key = id, ... }` — a Map literal with `MapField{ index: luau.string(exportKey), value: exportId }` (renders as `return { x = f }` etc. via map rendering; string keys that are valid identifiers render unquoted).
4. **no exports at all**: nothing here; `return nil` may still be appended by step 4 of transformSourceFile if the file is a ModuleScript.

`isSymbolMutable` (util/isSymbolMutable.ts:6-20): cached in `multiTransformState.isDefinedAsLetCache`; true if valueDeclaration is a Parameter, or its enclosing VariableDeclarationList has `NodeFlags.Let`. (`const` -> false; `var` doesn't exist in strict roblox-ts input.)

### 2.4 Call site / surrounding pipeline (`Project/functions/compileFiles.ts`)

Per file (lines 151-183): `getPreEmitDiagnostics` + custom pre-emit diags -> bail if errors -> `new TransformState(...)` -> `luauAST = transformSourceFile(transformState, sourceFile)` -> bail if errors -> `source = renderAST(luauAST)` -> queue `{sourceFile, source}`. Writing (188-208): `pathTranslator.getOutputPath`, skip write if `writeOnlyChanged` and bytes identical, `fs.outputFileSync(outPath, source)`. No post-processing of `source` — renderAST output is the literal file content.
Project type inference (27-35): Package if `data.isPackage` (scoped under @rbxts), else Game if `rojoResolver.isGame`, else Model; CLI `--type` overrides (line 80).

---

## 3. MultiTransformState + DiagnosticService

### `classes/MultiTransformState.ts` (13 lines)

State that lives for one whole **compilation step** (shared across all files of that pass; recreated each watch rebuild — compileFiles.ts:57). Pure cache container, no methods:

```ts
isMethodCache: Map<ts.Symbol, boolean>
isDefinedAsLetCache: Map<ts.Symbol, boolean>
isReportedByNoAnyCache: Set<ts.Symbol>
isReportedByMultipleDefinitionsCache: Set<ts.Symbol>
getModuleExportsCache: Map<ts.Symbol, Array<ts.Symbol>>
getModuleExportsAliasMapCache: Map<ts.Symbol, Map<ts.Symbol, string>>
```

### `classes/DiagnosticService.ts` (40 lines)

Global static accumulator (Go port: package-level or injected collector — note roblox-ts relies on it being global/static across modules):

- `addDiagnostic(d)` — push.
- `addDiagnostics(ds)` — push all.
- `addSingleDiagnostic(d)` — dedupe by `d.code` via `singleDiagnostics: Set<number>`.
- `addDiagnosticWithCache(cacheBy, d, cache)` — dedupe by arbitrary key in caller-provided Set.
- `flush()` — returns current array, resets array AND clears singleDiagnostics.
- `hasErrors()` — `hasErrors(diagnostics)` (any with category Error).

---

## 4. util/ digests (requested set — all exist)

### `getStatements.ts` (5 lines)

```ts
getStatements(statement: ts.Statement): ReadonlyArray<ts.Statement> =
    ts.isBlock(statement) ? statement.statements : [statement];
```

### `pointer.ts` (65 lines)

```ts
interface Pointer<T> { name: string; value: T; }
type MapPointer = Pointer<luau.Map | luau.TemporaryIdentifier>;
type ArrayPointer = Pointer<luau.Array | luau.TemporaryIdentifier>;
createMapPointer(name) => { name, value: luau.map() }      // empty inline map literal
createArrayPointer(name) => { name, value: luau.array() }

assignToMapPointer(state, ptr, left, right):
    if luau.isMap(ptr.value): push MapField{index:left, value:right} onto ptr.value.fields   // still inline literal
    else: state.prereq( ptr.value[left] = right )            // Assignment to ComputedIndexExpression

disableMapInline(state, ptr):   if luau.isMap(ptr.value)   ptr.value = state.pushToVar(ptr.value, ptr.name)
disableArrayInline(state, ptr): if luau.isArray(ptr.value) ptr.value = state.pushToVar(ptr.value, ptr.name)
```

Pattern: build object literals inline until something side-effectful forces materialization into a temp (`local obj = { ... }` then `obj[k] = v`).

### `convertToIndexableExpression.ts` (12 lines)

```ts
convertToIndexableExpression(expression: luau.Expression): luau.IndexableExpression {
    if (luau.isIndexableExpression(expression)) return expression;
    return luau.create(luau.SyntaxKind.ParenthesizedExpression, { expression });
}
```

Indexable kinds = Identifier, TemporaryIdentifier, ComputedIndexExpression, PropertyAccessExpression, CallExpression, MethodCallExpression, ParenthesizedExpression (luau-ast typeGuards.ts:19-23, contiguous SyntaxKind range First/LastIndexableExpression).

### `ensureTransformOrder.ts` (55 lines) — evaluation-order preservation

```ts
ensureTransformOrder(state, nodes, transformer = transformExpression): Array<luau.Expression> {
    const expressionInfoList = nodes.map(node => state.capture(() => transformer(state, node)));
    const lastArgWithPrereqsIndex = findLastIndex(expressionInfoList, ([, prereqs]) => !luau.list.isEmpty(prereqs));
    const result = [];
    for (let i = 0; i < expressionInfoList.length; i++) {
        const [expression, prereqs] = expressionInfoList[i];
        state.prereqList(prereqs);                       // re-emit captured prereqs in order
        let isConstVar = false;
        const exp = nodes[i];
        if (ts.isIdentifier(exp)) {
            const symbol = state.typeChecker.getSymbolAtLocation(exp);   // CHECKER
            if (symbol && !isSymbolMutable(state, symbol)) isConstVar = true;
        }
        if (i < lastArgWithPrereqsIndex
            && !luau.isSimplePrimitive(expression)       // literals can't be mutated
            && !luau.isTemporaryIdentifier(expression)   // temps already pinned
            && !isConstVar)                              // consts can't be reassigned by later prereqs
            result.push(state.pushToVar(expression, "exp"));
        else
            result.push(expression);
    }
    return result;
}
```

Rationale: if argument j>i has prereq statements, those statements could mutate values argument i depends on; pin earlier non-safe expressions into temps (`local exp = ...`). Only expressions strictly **before the last** prereq-bearing index are pinned.

### `expressionChain.ts` (26 lines)

```ts
binaryExpressionChain(expressions, operator) = expressions.reduce((acc, cur) => luau.binary(acc, operator, cur));  // left-assoc: a and b and c
propertyAccessExpressionChain(expression, names) = names.reduce((acc, cur) => luau.property(acc, cur), convertToIndexableExpression(expression));  // exp.a.b.c
```

### `isUsedAsStatement.ts` (22 lines)

```ts
isUsedAsStatement(expression: ts.Expression): boolean {
    const child = skipUpwards(expression);    // climb past parens/casts/nonnull/satisfies
    const parent = child.parent;
    if (ts.isExpressionStatement(parent)) return true;
    if (ts.isForStatement(parent) && parent.condition !== child) return true;   // for-initializer/incrementor (not condition)
    if (ts.isDeleteExpression(parent) && isUsedAsStatement(parent)) return true;  // recursive: `delete x.y;` at statement level
    return false;
}
```

### `wrapExpressionStatement.ts` (16 lines)

```ts
wrapExpressionStatement(node: luau.Expression): luau.List<luau.Statement> {
    if (luau.isTemporaryIdentifier(node) || luau.isNone(node)) return luau.list.make();          // drop entirely
    else if (luau.isCall(node)) return list of [ CallStatement{ expression: node } ];            // call|methodcall ok as statement
    else return list of [ VariableDeclaration{ left: luau.tempId(), right: node } ];             // `local _ = <expr>` to keep side effects/valid syntax
}
```

`luau.isCall` = CallExpression | MethodCallExpression (typeGuards.ts:120).

---

## 5. `Shared/constants.ts` relevant bits

- `COMPILER_VERSION` (line 9) = package.json version → `"3.0.0"` for our parity target; used ONLY in the header comment (transformSourceFile.ts:225).
- `PARENT_FIELD = "Parent"` (33) — RbxPathParent → `.Parent` property hops in package/model requires.
- `enum ProjectType { Game = "game", Model = "model", Package = "package" }` (35-39).
- `RBXTS_SCOPE = "@rbxts"` (12), `NODE_MODULES` (11), ext constants `TS_EXT/.tsx/.d.ts` (14-17), `INDEX_NAME/INIT_NAME` (19-20), subexts `.server`/`.client`/`""` (22-24), `FILENAME_WARNINGS` map init.* -> index.* (26-31).
- `DEFAULT_PROJECT_OPTIONS` (41-55): notably `allowCommentDirectives: false`, `optimizedLoops: true`, `luau: true`, `includePath: ""`, `writeOnlyChanged: false`.

---

## 6. Runtime-lib call emission summary

- Transforms needing the runtime library call `state.TS(node, "<name>")` to obtain `TS.<name>` (PropertyAccessExpression on the global identifier `TS`), then build a CallExpression around it; e.g. `luau.call(state.TS(node, "await"), [exp])`.
- That single method flips `state.usesRuntimeLib = true` (per file, monotone), which transformSourceFile checks at line 228 to insert `state.createRuntimeLibImport(node)` into the header block, after the version comment.
- Emitted import forms (§1.10): Game = `local TS = require(game:GetService("X"):WaitForChild("y")...)`; Model = `local TS = require(script.Parent....RuntimeLib)` relative chain with `Parent` hops; Package = `local TS = _G[script]`.
- ReplicatedFirst warning fires on every `TS()` call in Game projects whose file outputs under ReplicatedFirst.

---

## 7. The hoisting system (complete)

Purpose: TS allows use-before-declaration within a scope (function decls, `let` TDZ aside, mutual recursion); Luau locals are visible only after their `local` statement. Fix: pre-declare `local x` before the first statement that referenced it early.

### 7.1 Population — `checkIdentifierHoist` (transformIdentifier.ts:47-109)

Called from `transformIdentifier` (line 173) for every identifier *use* that wasn't handled as undefined/macro/export-mutable. Verbatim logic:

```ts
function checkIdentifierHoist(state, node: ts.Identifier, symbol: ts.Symbol) {
    if (state.isHoisted.get(symbol) !== undefined) return;        // already decided

    const declaration = symbol.valueDeclaration ?? getDeclarationFromImport(symbol);
    // getDeclarationFromImport (38-45): for import symbols, find declaration with an isAnyImportSyntax ancestor

    if (!declaration || getAncestor(declaration, ts.isParameter) || ts.isShorthandPropertyAssignment(declaration))
        return;                                                   // parameters cannot be hoisted

    if (ts.isClassLike(declaration) && isAncestorOf(declaration, node)) return;  // class exprs self-refer

    const declarationStatement = getAncestor(declaration, ts.isStatement);
    if (!declarationStatement || ts.isForStatement(declarationStatement)
        || ts.isForOfStatement(declarationStatement) || ts.isTryStatement(declarationStatement)) return;

    const parent = declarationStatement.parent;
    if (!parent || !isBlockLike(parent)) return;                  // SourceFile | Block | ModuleBlock | CaseClause...

    const sibling = getAncestorWhichIsChildOf(parent, node);      // walk node up until direct child of parent (30-35)
    if (!sibling || !ts.isStatement(sibling)) return;

    const declarationIdx = parent.statements.indexOf(declarationStatement);
    const siblingIdx = parent.statements.indexOf(sibling);
    if (siblingIdx > declarationIdx) return;                      // use is after declaration: no hoist needed
    if (siblingIdx === declarationIdx) {
        // same statement: self-reference allowed (no hoist) for:
        // - non-async FunctionDeclaration
        // - ClassDeclaration
        // - VariableStatement where the use's nearest Statement-or-FunctionLike ancestor IS the declaration statement
        //   (i.e. `const f = () => f()` is fine — the use is inside a nested function)
        if ((ts.isFunctionDeclaration(declarationStatement) && !ts.hasSyntacticModifier(declarationStatement, ts.ModifierFlags.Async))
            || ts.isClassDeclaration(declarationStatement)
            || (ts.isVariableStatement(declarationStatement)
                && getAncestor(node, n => ts.isStatement(n) || ts.isFunctionLikeDeclaration(n)) === declarationStatement))
            return;
    }

    getOrSetDefault(state.hoistsByStatement, sibling, () => []).push(node);   // record at the USE-site sibling statement
    state.isHoisted.set(symbol, true);
}
```

- `CHECKER:` `ts.hasSyntacticModifier` (internal helper; ≈ getModifiers().some(...)); `typeChecker.getSymbolAtLocation` / `getShorthandAssignmentValueSymbol` at transformIdentifier.ts:119-121 (also `isUndefinedSymbol`, `isArgumentsSymbol` at 124-126).
- Key detail: the `local x` is keyed by the **earliest using sibling statement**, not the declaration — the hoist declaration is emitted immediately before the statement that uses the symbol early, and the original declaration later compiles to an *assignment* (the declaration transform checks `state.isHoisted.get(symbol) === true` and emits `x = ...` instead of `local x = ...`).
- Note `isHoisted` is read via `.get(symbol) !== undefined` — once a symbol is decided (always set to `true`), it's never reconsidered.

### 7.2 Population — `checkVariableHoist` (util/checkVariableHoist.ts:6-39) — switch-case leakage

Called at variable-declaration time. If the declaration's parent statement sits directly in a `ts.CaseClause`, determine whether the symbol is referenced outside that case clause:

```ts
const isUsedOutsideOfCaseClause =
    ts.FindAllReferences.Core.eachSymbolReferenceInFile(   // CHECKER: ts-internal FindAllReferences.Core
        node, state.typeChecker, node.getSourceFile(),
        token => { if (!isAncestorOf(caseClause, token)) return true; },   // returning truthy short-circuits => result === true
        caseBlock,                                                          // search container
    ) === true;
if (isUsedOutsideOfCaseClause) {
    getOrSetDefault(state.hoistsByStatement, statement.parent /* the CaseClause */, () => []).push(node);
    state.isHoisted.set(symbol, true);
}
```

This is why `hoistsByStatement` is keyed by `ts.Statement | ts.CaseClause`. This is the hardest CHECKER item: tsgo needs a "find symbol references within container" facility.

### 7.3 Consumption — `createHoistDeclaration` + `transformStatementList`

`util/createHoistDeclaration.ts:7-16`:

```ts
createHoistDeclaration(state, statement: ts.Statement | ts.CaseClause): luau.VariableDeclaration | undefined {
    const hoists = state.hoistsByStatement.get(statement);
    if (hoists && hoists.length > 0) {
        hoists.forEach(hoist => validateIdentifier(state, hoist));    // reserved-word / invalid-luau-id diagnostics
        return luau.create(luau.SyntaxKind.VariableDeclaration, {
            left: luau.list.make(...hoists.map(hoistId => transformIdentifierDefined(state, hoistId))),  // list => `local a, b, c`
            right: undefined,                                          // no initializer
        });
    }
}
```

`nodes/transformStatementList.ts:26-91` — the merge point (also the generic statement-list driver used by transformSourceFile and every block):

```ts
const result = luau.list.make<luau.Statement>();
for (const statement of statements) {
    const [transformedStatements, prereqStatements] = state.capture(() => transformStatement(state, statement));
    if (state.compilerOptions.removeComments !== true)
        luau.list.pushList(result, state.getLeadingComments(statement));   // comments FIRST
    const hoistDeclaration = createHoistDeclaration(state, statement);     // then hoist `local a, b`
    if (hoistDeclaration) luau.list.push(result, hoistDeclaration);
    luau.list.pushList(result, prereqStatements);                          // then prereqs
    luau.list.pushList(result, transformedStatements);                     // then the statement itself
    const lastStatement = transformedStatements.tail?.value;
    if (lastStatement && luau.isFinalStatement(lastStatement)) break;      // stop after break/continue/return (dead code elided)
    if (exportInfo) { /* namespace exports: append containerId.<name> = <name> per mapping (lines 64-80) */ }
}
if (state.compilerOptions.removeComments !== true) {
    const lastToken = getLastToken(parent, statements);                    // trailing `}`/EOF token (lines 7-18)
    if (lastToken) luau.list.pushList(result, state.getLeadingComments(lastToken));  // trailing comments in the block
}
return result;
```

Ordering invariant per statement: **leading comments → hoisted `local a, b` → prereq statements → transformed statement(s) → namespace export assignments**. `luau.isFinalStatement` = Break | Return | Continue (typeGuards.ts:114-118). Hoist timing subtlety: `transformStatement` runs (inside `capture`) *before* `createHoistDeclaration` is consulted, so hoists registered while transforming statement N (a use-before-declare inside N) are picked up for N itself.

---

## 8. luau-ast facts the port needs (rendering contract)

1. `renderAST` (luau-ast/src/LuauRenderer/render.ts:130-139): `new RenderState(); solveTempIds(state, ast); return renderStatements(state, ast);`
2. `renderStatements` (util/renderStatements.ts:12-30): concatenates `render(state, listNode.value)` for each node, pushing the listNode onto `state.listNodesStack` around each render (siblings context, used e.g. for blank-line/`;` disambiguation decisions); asserts nothing follows a final statement except comments.
3. `renderComment` (nodes/statements/renderComment.ts:5-16): single-line text -> `--<text>` line; multi-line -> `--[<eq>[ ... ]<eq>]` block with safe `=` count. Directive comments stored as text `"!strict"` render as `--!strict`.
4. Every statement renderer emits via `state.line(...)` which appends `\n` and current tab indentation → final file ends with `\n`.
5. `luau.globals` ids used here (luau-ast/src/LuauAST/impl/globals.ts): `_G`, `TS`, `exports`, `require`, `script`, `game`.
6. Type-guard kind sets needed by §1.11/§4: `isSimple`, `isSimplePrimitive`, `isAnyIdentifier`, `isIndexableExpression`, `isFinalStatement`, `isCall` — exact member lists given inline above.

---

## 9. Consolidated CHECKER list (tsgo equivalents needed)

| Where | API | Notes |
|---|---|---|
| TransformState.ts:61 | `typeChecker.getEmitResolver(sourceFile)` | internal; only need `isReferencedAliasDeclaration` + JSX factory entity getters |
| TransformState.ts:140 | `ts.getLeadingCommentRanges` | public scanner util |
| TransformState.ts:185 | `typeChecker.getTypeAtLocation` | public |
| TransformState.ts:310 | `typeChecker.getExportsOfModule` | public |
| TransformState.ts:318, transformSourceFile.ts:114 | `ts.skipAlias` | internal; ≈ follow `getAliasedSymbol` while Alias flag set |
| TransformState.ts:332, many | `typeChecker.getSymbolAtLocation` | public |
| TransformState.ts:368-379 | `program.getSymlinkCache`, `ts.ensureTrailingDirectorySeparator`, `ts.toPath` | internal; only for pnpm symlink path guessing |
| transformIdentifier.ts:16,120 | `typeChecker.getShorthandAssignmentValueSymbol` | public |
| transformIdentifier.ts:124-126 | `typeChecker.isUndefinedSymbol`, `isArgumentsSymbol` | public |
| transformIdentifier.ts:95 | `ts.hasSyntacticModifier` | internal; reimplement via modifiers list |
| checkVariableHoist.ts:23 | `ts.FindAllReferences.Core.eachSymbolReferenceInFile` | internal language-service; hardest to replace — need symbol-reference walk within a container |
| transformSourceFile.ts:204 | `typeChecker.getSymbolAtLocation(sourceFile)` | module symbol; must exist (assert) |
| transformStatementList.ts:10-11 | `lastStatement.parent.getLastToken()`, `ts.isNodeDescendantOf` | getLastToken needs token-level AST access in tsgo; isNodeDescendantOf is internal (trivial pos check) |
| handleExports / isSymbolMutable etc. | `symbol.getDeclarations()`, `symbol.flags`, `ts.SymbolFlags.Prototype`, `NodeFlags.Let` | symbol/flag plumbing |
