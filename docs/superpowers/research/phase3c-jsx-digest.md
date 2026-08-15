# Phase 3c Digest — JSX (transformJsx, attributes, children, tag names, fragments)

Source of truth for porting roblox-ts 3.0.0 JSX emission without re-reading the TS. All
upstream paths relative to `reference/roblox-ts/src/`; tsgo paths relative to repo root.
Every checker/type-API usage is flagged `CHECKER:`. Builds on `phase2-transforms-digest.md`
(pointers, ensureTransformOrder, truthiness), `phase2-transformstate-digest.md` (capture/
prereq/pushToVar), `phase3-imports-digest.md` (transformEntityName, import retention).

Empirical claims verified 2026-06-07:

- **Oracle shapes**: rbxtsc 3.0.0 (real CLI) run over a THROWAWAY COPY of
  `testdata/diff/project` in `%TEMP%\rotor-jsx-oracle` with `@rbxts/react@17.3.7-ts.1`
  npm-installed and the randomness jsx tsconfig keys added. All §3 Luau is byte-verbatim
  rbxtsc output. Artifacts deleted.
- **tsgo typecheck probe**: a copy of the rotor repo in `%TEMP%\rotor-probe-repo` ran
  `compile.CompileProject` against the same throwaway project: **zero TypeScript
  diagnostics** — the only diagnostics were rotor's own `JsxElement/JsxSelfClosingElement/
  JsxFragment not yet supported`. So tsgo parses .tsx, accepts the sanitized jsx config,
  and typechecks `@rbxts/react`'s d.ts cleanly. **No sanitizer changes are needed for JSX**
  (§5.4). Probe repo deleted.

SCOPE: (1) acceptance-target survey, (2) the five upstream JSX transform files COMPLETE,
(3) oracle-verified worked examples, (4) diagnostics, (5) tsgo node kinds / checker APIs /
options, (6) rotor implementation sketch incl. fixture-project dependency spec, (7) quirks.

---

## 1. Acceptance-target survey (randomness)

### 1.1 tsconfig (C:\Users\user\Source\Roblox\randomness\tsconfig.json)

```jsonc
"jsx": "react",
"jsxFactory": "React.createElement",
"jsxFragmentFactory": "React.Fragment",
```

(plus the standard rbxts required set already handled by the sanitizer: baseUrl "src" →
paths rewrite, downlevelIteration removal, moduleResolution Node → bundler.)

### 1.2 Packages

- `@rbxts/react` **17.3.7-ts.1** (littensy/rbxts-react). `main: src/init.lua`,
  `types: src/index.d.ts`. Runtime dep `@rbxts-js/react@^17.3.7-ts.1` (Lua polyfill scope
  `node_modules/@rbxts-js/*` — outside the `@rbxts` typeRoot; irrelevant to typecheck,
  proven by the probe).
- `@rbxts/react-roblox` 17.3.7-ts.1 (renderer — imported by entry points, not by the JSX
  transform).
- `@rbxts/pretty-react-hooks`, `@rbxts/react-charm`, `@rbxts/ui-labs` — consumers only.
- This is the **new React ecosystem**, NOT legacy `@rbxts/roact`. Upstream's own test
  suite (`reference/roblox-ts/tests`, `jsxFactory: "Roact.jsx"`) is the OLD world; ignore
  its factory config, but its constructs (Event/Change/Ref tables, implicit-true,
  spread ordering) remain valid transform-level tests.

### 1.3 Construct census (36 .tsx files under randomness/src)

| construct | files | notes |
|---|---|---|
| JsxElement / JsxSelfClosingElement | 33 of 41 non-compiling files blocked | the Phase 3c unlock |
| fragments `<>...</>` | 5 | always `<>{children}</>` or element lists |
| `key=` attribute | 7 | template-string values, e.g. ``key={`seg${i}`}`` — **plain prop in 3.0** (§3 case 6) |
| `Event={...}` / `Change={...}` | 9 / 8 | plain object-literal props (§3 case 7) |
| `.map(...)` children | 7 | array passed as a single child (§3 case H) |
| `cond && <el/>` children | 6 | binary expr child |
| ternary children | 1 | if-then-else expr child |
| spread attributes `{...x}` | **0** | still must be ported (upstream parity + future code) |
| JSX text children | **0** | type-impossible: `@rbxts/react`'s `ReactNode = ReactElement \| ReactFragment \| ReactPortal \| boolean \| undefined` excludes string (index.d.ts:314) |
| namespaced tags/attrs `a:b` | 0 | port the branch anyway (small) |
| dotted component tags `<NS.Item/>` | rare | supported (§3 case I) |

Smallest real file (`src/client/app/providers.tsx`) is exactly oracle case K:
`return <>{children}</>;`.

---

## 2. Upstream architecture

JSX has NO util/jsx directory in 3.0 (no getKeyValue/createRoactIndex — those were the
1.x Roact path). The complete surface is five transform files + one util + dispatch:

| upstream file | role |
|---|---|
| `TSTransformer/nodes/expressions/transformJsxElement.ts` (L5-7) | thin: `transformJsx(state, node, node.openingElement.tagName, node.openingElement.attributes, node.children)` |
| `TSTransformer/nodes/expressions/transformJsxSelfClosingElement.ts` (L5-7) | thin: `transformJsx(state, node, node.tagName, node.attributes, [])` |
| `TSTransformer/nodes/expressions/transformJsxFragment.ts` (L9-34) | fragment factory call |
| `TSTransformer/nodes/expressions/transformJsxExpression.ts` (L6-15) | `{expr}` / `{...expr}` child as expression |
| `TSTransformer/nodes/jsx/transformJsx.ts` (L12-46) | the factory-call assembler |
| `TSTransformer/nodes/jsx/transformJsxTagName.ts` (L9-38) | tag → string or value expression |
| `TSTransformer/nodes/jsx/transformJsxAttributes.ts` (L11-103) | props map incl. spread |
| `TSTransformer/nodes/jsx/transformJsxChildren.ts` (L11-39) | child list incl. JsxText |
| `TSTransformer/util/fixupWhitespaceAndDecodeEntities.ts` (L25-87 + entity table L89-345) | TS's own JSX-text algorithm, copied |

Dispatch (`transformExpression.ts` L96-99): `JsxElement`, `JsxExpression`, `JsxFragment`,
`JsxSelfClosingElement` map to the four expression transforms.

### 2.1 transformJsx (transformJsx.ts L12-46) — the core

```text
transformJsx(state, node /*JsxElement|JsxSelfClosingElement*/, tagName, attributes, children):
    jsxFactoryEntity = state.resolver.getJsxFactoryEntity(node)        // CHECKER (EmitResolver)
    assert(jsxFactoryEntity)   // "Expected jsxFactoryEntity to be defined"
                               // (always defined: checker defaults to React.createElement)
    createElementExpression = convertToIndexableExpression(transformEntityName(state, jsxFactoryEntity))

    tagNameExp = transformJsxTagName(state, tagName)                   // §2.2

    attributesPtr = undefined
    if attributes.properties.length > 0:
        attributesPtr = createMapPointer("attributes")
        transformJsxAttributes(state, attributes, attributesPtr)       // §2.3

    transformedChildren = transformJsxChildren(state, children)        // §2.4

    args = [tagNameExp]
    if attributesPtr:             args.push(attributesPtr.value)   // map literal OR _attributes temp
    else if children.length > 0:  args.push(luau.nil())            // nil placeholder ONLY here
    args.push(...transformedChildren)
    return luau.call(createElementExpression, args)
```

Shapes (oracle-proven, §3):

- no attrs, no children → `React.createElement("frame")`
- attrs only → `React.createElement("frame", { ... })`
- children only → `React.createElement("frame", nil, child1, child2)`
- both → `React.createElement("frame", { ... }, child1)` — NO nil between

`transformEntityName` (transformEntityName.ts L8-19): Identifier → `validateIdentifier` +
`transformIdentifier`; QualifiedName → `luau.property(convertToIndexable(transformEntityName(left)), right.text)`.
The factory entity is SYNTHETIC (parsed from the option string, position -1, no parent);
`transformIdentifier` (transformIdentifier.ts L111-117) detects
`!node.parent || ts.positionIsSynthesized(node.pos)` and emits a bare
`luau.Identifier(node.text)` with NO symbol lookups. For `"React.createElement"` the
result is property access `React.createElement` (already indexable, convertToIndexable
is a no-op).

ROTOR MAPPING: new `internal/transformer/jsx.go` `transformJsx`. Already in place:
`transformEntityName` (imports.go:232, incl. ValidateIdentifier which is synthetic-safe —
statementlist.go:153 uses only `node.Text()`), synthetic-identifier early-out in
`TransformIdentifier` (identifier.go:30-33, comment even names getJsxFactoryEntity),
`CreateMapPointer` (pointer.go:25), `convertToIndexableExpression` (access.go usage).
Factory entity: `s.EmitResolver().GetJsxFactoryEntity(node)` (state.go:445; tsgo
checker/emitresolver.go:53-57). Assert → panic with the upstream message (byte-parity
files never hit it).

### 2.2 transformJsxTagName (transformJsxTagName.ts L9-38)

```text
transformJsxTagNameExpression(state, node /*JsxTagNameExpression*/):
    if isIdentifier(node):
        firstChar = node.text[0]
        if firstChar === firstChar.toLowerCase():       // QUIRK §7.1: '_' passes!
            return luau.string(node.text)               // host component: <frame/> -> "frame"
        // else fall through to transformExpression below
    if isPropertyAccessExpression(node):                 // <NS.Item/>; also <a.b.c/>
        if isPrivateIdentifier(node.name): DiagnosticService.add(errors.noPrivateIdentifier(node.name))
        return luau.property(convertToIndexable(transformExpression(state, node.expression)), node.name.text)
    else if isJsxNamespacedName(node):                   // <a:b/>
        return luau.string(getTextOfJsxNamespacedName(node))   // "a:b" (namespace ":" name)
    else:
        return transformExpression(state, node)          // Identifier (component) / ThisExpression

transformJsxTagName(state, tagName):
    [expression, prereqs] = state.capture(() => transformJsxTagNameExpression(state, tagName))
    tagNameExp = expression
    if !prereqs.isEmpty():
        state.prereqList(prereqs)
        tagNameExp = state.pushToVarIfComplex(tagNameExp, "tagName")    // temp name "tagName"
    return tagNameExp
```

Notes:

- The lowercase test is on the RAW first char via JS `toLowerCase()`. `"_" === "_".toLowerCase()`
  → true, so `<_Comp/>` emits the STRING `"_Comp"` even though the checker bound it to a
  function component. Oracle-proven (§3 case L). Port with the same semantics: take the
  first rune, compare `string(r) == strings.ToLower(string(r))`.
- `getTextOfJsxNamespacedName` = `namespace.text + ":" + name.text` (tsgo equivalent:
  `n.AsJsxNamespacedName().Namespace.Text() + ":" + n.AsJsxNamespacedName().Name().Text()`,
  cf. tsgo/ast/utilities.go:2108-2109).
- ThisExpression tag (`<this.Comp/>` outer `this`) flows through PropertyAccess branch's
  transformExpression.

ROTOR MAPPING: `transformJsxTagName` in jsx.go. `s.Capture` (state.go:277),
`s.PushToVarIfComplex(exp, "tagName")` (state.go:320), `DiagNoPrivateIdentifier`
(diagnostics.go:98). tsgo: `ast.IsIdentifier`, `ast.IsPropertyAccessExpression`,
`ast.IsPrivateIdentifier(node.Name())`, `ast.IsJsxNamespacedName`.

### 2.3 transformJsxAttributes (transformJsxAttributes.ts)

Top loop (L70-103) over `attributes.properties` (order preserved):

```text
for attribute in attributes.properties:
    if isJsxAttribute(attribute): transformJsxAttribute(state, attribute, attributesPtr)
    else:  // JsxSpreadAttribute  `<frame {...x}/>`
        expType = state.typeChecker.getNonOptionalType(state.getType(attribute.expression))  // CHECKER
        symbol = getFirstDefinedSymbol(state, expType)                                       // CHECKER
        if symbol && macroManager.isMacroOnlyClass(symbol):
            DiagnosticService.add(errors.noMacroObjectSpread(attribute))
        expression = transformExpression(state, attribute.expression)

        if attribute === attributes.properties[0] && isDefinitelyType(expType, isObjectType):
            // FIRST property overall AND definitely an object: clone fast-path
            attributesPtr.value = state.pushToVar(luau.call(luau.globals.table.clone, [expression]), attributesPtr.name)
            state.prereq(CallStatement(luau.call(luau.globals.setmetatable, [attributesPtr.value, luau.nil()])))
            //   ^ "Explicitly remove metatable because things like classes can be spread"
            continue

        disableMapInline(state, attributesPtr)        // map literal so far -> local _attributes = {...}
        state.prereq(createJsxAttributeLoop(state, attributesPtr.value, expression, attribute.expression))
```

`transformJsxAttribute` (L50-68):

```text
initializer = attribute.initializer                 // string literal | JsxExpression | JsxElement | undefined
if initializer && isJsxExpression(initializer): initializer = initializer.expression
                                                    // NOTE: {} empty expr -> undefined -> true (QUIRK §7.3)
[init, initPrereqs] = initializer ? state.capture(() => transformExpression(state, initializer))
                                  : [luau.bool(true), emptyList]      // implicit true: <el Visible />
if !initPrereqs.isEmpty():
    disableMapInline(state, attributesPtr)          // BEFORE pushing the prereqs (oracle §3 case C)
    state.prereqList(initPrereqs)
text = isIdentifier(attribute.name) ? attribute.name.text : getTextOfJsxNamespacedName(attribute.name)
assignToMapPointer(state, attributesPtr, luau.string(text), init)
```

`createJsxAttributeLoop` (L11-48) — the generic spread merge:

```text
definitelyObject = isDefinitelyType(state.getType(tsExpression), isObjectType)   // CHECKER
if !definitelyObject: expression = state.pushToVarIfComplex(expression, "attribute")
statement = ForStatement{ ids: [tempId("k"), tempId("v")], expression,
                          body: [ Assignment{ attributesPtrValue[k] = v } ] }
   // renders: for _k, _v in <expression> do  _attributes[_k] = _v  end   (Luau generalized iteration)
if !definitelyObject:
    statement = IfStatement{ condition: createTruthinessChecks(state, expression, tsExpression),
                             statements: [statement], elseBody: [] }
return statement
```

Pointer semantics (util/pointer.ts): `assignToMapPointer` appends a `MapField` while the
pointer is still an inline map, else prereqs `ptr.value[left] = right`. `disableMapInline`
= `ptr.value = state.pushToVar(ptr.value /*map literal*/, ptr.name)` → `local _attributes = { ...fields so far... }`.

ROTOR MAPPING: jsx.go `transformJsxAttributes` / `transformJsxAttribute` /
`createJsxAttributeLoop`. Existing: `AssignToMapPointer`/`DisableMapInline` (pointer.go:37/52),
`s.Checker.GetNonOptionalType`, `GetFirstDefinedSymbol` (types.go:472), `IsDefinitelyType` +
`IsObjectType` (types.go:106/247), `s.Macros.IsMacroOnlyClass` (macromanager.go:441),
`DiagNoMacroObjectSpread` (diagnostics.go:279), `CreateTruthinessChecks(s, exp, node, t)`
(truthiness.go:31 — pass `s.GetType(tsExpression)` as t), `luau.TempID("k")/("v")`
(create.go:61), `luau.GlobalProperty("table", "clone")`, setmetatable global. tsgo:
`ast.IsJsxAttribute`, `attributes.AsJsxAttributes().Properties.Nodes`,
`attr.AsJsxAttribute().Initializer`, `attr.AsJsxSpreadAttribute().Expression`,
attribute name via `attr.AsJsxAttribute().Name()` (Identifier or JsxNamespacedName).

### 2.4 transformJsxChildren (transformJsxChildren.ts L11-39)

```text
lastJsxChildIndex = findLastIndex(children, c => !isJsxText(c) || !c.containsOnlyTriviaWhiteSpaces)
for i in [0, lastJsxChildIndex):              // EXCLUSIVE of the last significant child
    if isJsxExpression(children[i]) && children[i].dotDotDotToken:
        DiagnosticService.add(errors.noPrecedingJsxSpreadElement(children[i]))

return ensureTransformOrder(state,
    children.filter(c => !isJsxText(c) || !c.containsOnlyTriviaWhiteSpaces)   // drop whitespace-only text
            .filter(c => !isJsxExpression(c) || c.expression !== undefined), // drop `{}` empty exprs
    (state, node) => {
        if isJsxText(node):
            text = fixupWhitespaceAndDecodeEntities(node.text) ?? ""
            return luau.string(text.replace(/\\/g, "\\\\"))     // QUIRK §7.2: double every backslash
        return transformExpression(state, node)                  // JsxElement/SelfClosing/Fragment/JsxExpression
    })
```

- `findLastIndex` (Shared/util/findLastIndex.ts): scan end→start, first hit's index, else -1.
- `ensureTransformOrder` is the standard prereq-ordering walk (already ported,
  ensureorder.go:22 `ensureTransformOrderWith`): every child is captured; children BEFORE
  the last prereq-carrying child that are not simple-primitive/temp/const-identifier get
  `pushToVar(..., "exp")` → `local _exp = getEl()` (oracle §3 case D).
- The diagnostic loop runs over the ORIGINAL children (pre-filter) but stops BEFORE
  `lastJsxChildIndex`, so a trailing `{...arr}` (ignoring trailing whitespace-only text) is
  legal; any earlier one errors. Children equal to `-1` (all whitespace) → loop doesn't run.
- JsxText that reaches the transformer always yields a non-empty fixup in practice
  (whitespace-only-with-newline text has `containsOnlyTriviaWhiteSpaces` and is filtered;
  single-line whitespace-only text returns the whitespace itself). The `?? ""` is a
  safety net — port it (Go helper returns "" naturally, §5.3).

ROTOR MAPPING: jsx.go `transformJsxChildren` using `ensureTransformOrderWith`. JsxText:
`child.AsJsxText().Text` + `ContainsOnlyTriviaWhiteSpaces` (tsgo ast_generated.go:6986-6998);
JsxExpression: `child.AsJsxExpression().DotDotDotToken != nil` / `.Expression == nil`.
`DiagNoPrecedingJsxSpreadElement` already exists (diagnostics.go:329). findLastIndex:
inline a small helper (rotor has none generic).

### 2.5 transformJsxExpression (transformJsxExpression.ts L6-15)

```text
if node.expression:
    expression = transformExpression(state, node.expression)
    if node.dotDotDotToken: return luau.call(luau.globals.unpack, [expression])   // {...arr} -> unpack(arr)
    return expression
return luau.none()       // bare {} as a standalone expression (children filter already dropped it)
```

ROTOR MAPPING: jsx.go `transformJsxExpression`; dispatch case `ast.KindJsxExpression`.
`luau.Global("unpack")` / `luau.NewNone()`.

### 2.6 transformJsxFragment (transformJsxFragment.ts L9-34)

```text
jsxFactoryEntity = state.resolver.getJsxFactoryEntity(node); assert(...)
createElementExpression = convertToIndexable(transformEntityName(state, jsxFactoryEntity))
jsxFragmentFactoryEntity = state.resolver.getJsxFragmentFactoryEntity(node)        // CHECKER
                           ?? ts.parseIsolatedEntityName("Fragment", ESNext)       // QUIRK §7.4
assert(jsxFragmentFactoryEntity)  // "Unable to find valid jsxFragmentFactoryEntity"
args = [transformEntityName(state, jsxFragmentFactoryEntity)]   // NOT convertToIndexable — it's an argument
transformedChildren = transformJsxChildren(state, node.children)
if transformedChildren.length > 0: args.push(luau.nil())        // fragments ALWAYS nil before children
args.push(...transformedChildren)
return luau.call(createElementExpression, args)
```

Shapes: `<></>` → `React.createElement(React.Fragment)`;
`<>{children}</>` → `React.createElement(React.Fragment, nil, children)`.

ROTOR MAPPING: jsx.go `transformJsxFragment`; dispatch `ast.KindJsxFragment`.
`s.EmitResolver().GetJsxFragmentFactoryEntity(node)` (tsgo emitresolver.go:59-63).
Fallback: `parser.ParseIsolatedEntityName("Fragment")` (tsgo/parser/parser.go:281) — but
that does NOT mark synthetic; mirror checker.markAsSynthetic (jsx.go:1444-1448: walk the
nodes setting `Loc = core.NewTextRange(-1, -1)`) or simpler, hand-build a synthetic
`luau.Identifier("Fragment")`-producing path (the entity is always a single identifier).
Pick the parse+mark route for upstream fidelity.

### 2.7 fixupWhitespaceAndDecodeEntities (util/fixupWhitespaceAndDecodeEntities.ts)

Verbatim copy of TS's own jsx transformer algorithm: per-line, drop all-whitespace lines,
trimRight first line / trim middles / trimLeft last line, decode HTML entities per line,
join surviving lines with a single space; returns undefined if every line is whitespace.
Entities: `&#123;` decimal, `&#xAB;` hex (regex `&((#((\d+)|x([\da-fA-F]+)))|(\w+));`,
encoded via utf16EncodeAsString), plus the 252-entry named table (quot amp apos lt gt
nbsp ... diams) — values are Unicode codepoints (file L89-345).

**tsgo already has a Go implementation**: `tsgo/transformers/jsxtransforms/jsx.go:810-850`
(`fixupWhitespaceAndDecodeEntities`, returns "" instead of undefined — exactly matches
upstream's `?? ""` consumption) and `decodeEntities`/`decodeEntity` (L864-940+). Both are
UNEXPORTED — copy them (and the entity table) into rotor rather than overlay-export.
Known micro-divergences from the TS original, acceptable (unreachable for parity files):
tsgo `WriteRune` vs TS `utf16EncodeAsString` differ only for lone surrogates
(`&#xD800;` → U+FFFD in Go vs an unpaired surrogate in JS) and tsgo's `decodeEntities`
ampersand re-scan handles `&&amp;;`-style nesting slightly differently. Flag in code.

ROTOR MAPPING: new `internal/transformer/jsxtext.go` housing the copied fixup +
decodeEntities + entity table, plus the roblox-ts-specific backslash doubling at the call
site (§7.2): `luau.Str(strings.ReplaceAll(fixed, "\\", "\\\\"))`. Rationale for the
doubling: rotor's renderStringLiteral (internal/luau/render/expressions.go:105-119), like
upstream luau-ast, emits string values RAW (no escaping) — oracle §3 case E shows rbxtsc
writes `"back\\slash"` for source text `back\slash`.

---

## 3. Oracle-verified worked examples

All Luau below is byte-verbatim rbxtsc 3.0.0 output (`--type model`) from the throwaway
project (jsx react / React.createElement / React.Fragment, `@rbxts/react@17.3.7-ts.1`).
Cases D/E and the entity case used a TextHolder component whose props were locally typed
`children?: any` AND a throwaway-only widening of `ReactNode` to include string — both are
type-level only (emit identical); needed because @rbxts/react's ReactNode rejects text
children (§1.3).

File preamble (every case):

```lua
-- Compiled with roblox-ts v3.0.0
local TS = require(script.Parent.include.RuntimeLib)
local React = TS.import(script, script.Parent, "node_modules", "@rbxts", "react")
```

**1. Self-closing host element, props incl. implicit-true**

```tsx
export const a = <frame BackgroundTransparency={0.5} Visible />;
```

```lua
local a = React.createElement("frame", {
	BackgroundTransparency = 0.5,
	Visible = true,
})
```

**2. Element with props AND children (no nil separator)** — note outer parens come from
the TS parenthesized expression, not from JSX:

```tsx
export const b = (
	<screengui ResetOnSpawn={false}>
		<frame Visible={true} />
		<textlabel Text="hi" />
	</screengui>
);
```

```lua
local b = (React.createElement("screengui", {
	ResetOnSpawn = false,
}, React.createElement("frame", {
	Visible = true,
}), React.createElement("textlabel", {
	Text = "hi",
})))
```

**3. Fragment with children**

```tsx
export const c = (<><frame /><frame /></>);
```

```lua
local c = (React.createElement(React.Fragment, nil, React.createElement("frame"), React.createElement("frame")))
```

**4. Mixed children: text, `&&`, ternary, Map expression** (TextHolder: `children?: any`)

```tsx
export const d = (
	<TextHolder>
		hello world
		{cond && <frame />}
		{cond ? <frame /> : <textlabel />}
		{items}
	</TextHolder>
);
```

```lua
local d = (React.createElement(TextHolder, nil, "hello world", cond and React.createElement("frame"), if cond then React.createElement("frame") else React.createElement("textlabel"), items))
```

**5. Spread attributes — definitely-object** (`extra: { Visible: boolean }`)

```tsx
export const e = <frame BackgroundTransparency={1} {...extra} Visible={false} />;
export const e2 = <frame {...extra} Visible={false} />;
```

```lua
local _attributes = {
	BackgroundTransparency = 1,
}
for _k, _v in extra do
	_attributes[_k] = _v
end
_attributes.Visible = false
local e = React.createElement("frame", _attributes)
local _attributes_1 = table.clone(extra)
setmetatable(_attributes_1, nil)
_attributes_1.Visible = false
local e2 = React.createElement("frame", _attributes_1)
```

(e: spread is NOT properties[0] → loop path. e2: spread IS properties[0] and definitely
object → `table.clone` + `setmetatable(..., nil)`; later attrs become assignments because
the pointer is a temp.)

**6. `key` is a plain prop (React world, no Roact Key semantics)**

```tsx
export const f = (
	<frame key="container">
		<Item key="one" text="1" />
		<Item key="two" text="2" />
	</frame>
);
```

```lua
local f = (React.createElement("frame", {
	key = "container",
}, React.createElement(Item, {
	key = "one",
	text = "1",
}), React.createElement(Item, {
	key = "two",
	text = "2",
})))
```

**7. Event/Change tables are ordinary props**

```tsx
export const g = (
	<textbutton
		Event={{ MouseButton1Click: () => print("click") }}
		Change={{ AbsoluteSize: () => print("size") }}
	/>
);
```

```lua
local g = (React.createElement("textbutton", {
	Event = {
		MouseButton1Click = function()
			return print("click")
		end,
	},
	Change = {
		AbsoluteSize = function()
			return print("size")
		end,
	},
}))
```

**8 (I). Dotted component tag**

```tsx
export const h = <NS.Item text="3" />;          // also <Nested.Deep.Comp />
```

```lua
local h = React.createElement(NS.Item, {
	text = "3",
})
-- and: local i = React.createElement(Nested.Deep.Comp)
```

**9. JSX text fixup + entities** — `&amp;` → `&`, `&nbsp;` → raw UTF-8 U+00A0
(bytes `C2 A0` in the .luau, hexdump-verified), lines joined with " ":

```tsx
export const i = (
	<TextHolder>
		one &amp; two&nbsp;three
		line2
	</TextHolder>
);
```

```lua
local i = (React.createElement(TextHolder, nil, "one & two three line2"))
```

**10. Spread child (last) → unpack**

```tsx
export const j = <frame>{...arr}</frame>;
```

```lua
local j = React.createElement("frame", nil, unpack(arr))
```

**A/B. Spread of possibly-undefined object** (`maybe: {Visible:boolean} | undefined`) —
not definitely-object, so even at properties[0] it takes the guarded-loop path:

```lua
local _attributes = {}
if maybe then
	for _k, _v in maybe do
		_attributes[_k] = _v
	end
end
local a = React.createElement("frame", _attributes)
local _attributes_1 = {
	BackgroundTransparency = 1,
}
if maybe then
	for _k, _v in maybe do
		_attributes_1[_k] = _v
	end
end
local b = React.createElement("frame", _attributes_1)
```

**C. Attribute initializer with prereqs disables the inline map** (`flags.pop()!`):

```lua
local _attributes_2 = {}
-- ▼ Array.pop ▼
local _length = #flags
local _result = flags[_length]
flags[_length] = nil
-- ▲ Array.pop ▲
_attributes_2.Visible = _result
-- ▼ Array.pop ▼
local _length_1 = #flags
local _result_1 = flags[_length_1]
flags[_length_1] = nil
-- ▲ Array.pop ▲
_attributes_2.Active = _result_1
local c = React.createElement("frame", _attributes_2)
```

**D. Child ordering temps (ensureTransformOrder)** — `<frame>{getEl()}{els.pop()!}</frame>`:

```lua
local _exp = getEl()
-- ▼ Array.pop ▼
local _length_2 = #els
local _result_2 = els[_length_2]
els[_length_2] = nil
-- ▲ Array.pop ▲
local d = React.createElement("frame", nil, _exp, _result_2)
```

**E. Backslash doubling in JsxText** — source `<TextHolder>back\slash</TextHolder>`:

```lua
local e = React.createElement(TextHolder, nil, "back\\slash")
```

**F/G. Empty fragment; `{}` child dropped**

```lua
local f = React.createElement(React.Fragment)
local g = React.createElement("frame")        -- <frame>{}</frame>
```

**H. `.map` children — the array is ONE child**

```tsx
export const h = <frame>{list.map(s => <textlabel key={s} Text={s} />)}</frame>;
```

```lua
-- ▼ ReadonlyArray.map ▼
local _newValue = table.create(#list)
local _callback = function(s)
	return React.createElement("textlabel", {
		key = s,
		Text = s,
	})
end
for _k, _v in list do
	_newValue[_k] = _callback(_v, _k - 1, list)
end
-- ▲ ReadonlyArray.map ▲
local h = React.createElement("frame", nil, _newValue)
```

**J. Attrs + children together** — `<frame Visible={true}>{getEl()}</frame>`:

```lua
local j = React.createElement("frame", {
	Visible = true,
}, getEl())
```

**K. The providers.tsx shape** — `<>{children}</>`:

```lua
local k = React.createElement(React.Fragment, nil, children)
```

**L. QUIRK probe: underscore-leading component emits a STRING tag**

```tsx
function _Comp() { return <frame />; }
export const a = <_Comp />;
```

```lua
local function _Comp()
	return React.createElement("frame")
end
local a = React.createElement("_Comp")
```

**M. Diagnostic probe** — `<frame>{...arr}<frame /></frame>` fails the compile with
`src/_scratch4.tsx:11:27 - error TS roblox-ts: JSX spread expression must come last in children!`
(span on the `{...arr}` JsxExpression).

---

## 4. Diagnostics (byte-exact)

Only ONE diagnostic is JSX-specific (diagnostics.ts L198-199); two shared ones are reachable
from JSX paths. All three ALREADY EXIST in rotor:

| upstream id | text | rotor |
|---|---|---|
| `noPrecedingJsxSpreadElement` (error) | `JSX spread expression must come last in children!` | diagnostics.go:329 ✓ |
| `noMacroObjectSpread` (error) | `Macro classes cannot be used in an object spread!` + suggestion `Did you mean to use an array spread? `[ ...exp ]`` | diagnostics.go:279 ✓ |
| `noPrivateIdentifier` (error) | `Private identifiers are not supported!` | diagnostics.go:98 ✓ |

`validateCompilerOptions` (Project/functions/validateCompilerOptions.ts L33-116) has **no
jsx requirements whatsoever** — rbxtsc accepts any/no jsx config; the checker handles
factory resolution and errors (e.g. TS2747 children-type, TS17016 fragment-factory) come
from TypeScript itself. rotor needs no project-level jsx validation.

---

## 5. tsgo mapping

### 5.1 Node kinds (tsgo/ast/kind_generated.go)

`KindJsxText` (22), `KindJsxTextAllWhiteSpaces` (23, scanner token only — the AST node is
always JsxText with a bool), `KindJsxElement` (316), `KindJsxSelfClosingElement` (317),
`KindJsxOpeningElement` (318), `KindJsxClosingElement` (319), `KindJsxFragment` (320),
`KindJsxOpeningFragment` (321), `KindJsxClosingFragment` (322), `KindJsxAttribute` (323),
`KindJsxAttributes` (324), `KindJsxSpreadAttribute` (325), `KindJsxExpression` (326),
`KindJsxNamespacedName` (327).

### 5.2 Structs / field access (tsgo/ast/ast_generated.go)

| node | fields needed |
|---|---|
| `JsxElement` (6524) | `OpeningElement`, `Children *JsxChildList`, (`ClosingElement` unused) |
| `JsxOpeningElement` (6652) | `TagName`, `Attributes` (TypeArguments unused) |
| `JsxSelfClosingElement` (6695) | `TagName`, `Attributes` |
| `JsxFragment` (6738) | `Children` |
| `JsxAttributes` (6567) | `Properties *JsxAttributeList` → `.Nodes` |
| `JsxAttribute` (6823) | `Name()` (Identifier or JsxNamespacedName), `Initializer` (optional; StringLiteral / JsxExpression / JsxElement / ...) |
| `JsxSpreadAttribute` (6869) | `Expression` |
| `JsxExpression` (6946) | `DotDotDotToken` (optional), `Expression` (optional) |
| `JsxText` (6986) | `Text`, `ContainsOnlyTriviaWhiteSpaces` |
| `JsxNamespacedName` (6607) | `Namespace`, `Name()` |

Guards: `ast.IsJsxAttribute`, `ast.IsJsxExpression`, `ast.IsJsxText`,
`ast.IsJsxNamespacedName`, `ast.IsPropertyAccessExpression`, `ast.IsPrivateIdentifier`.

### 5.3 Checker / resolver APIs

- `EmitResolver.GetJsxFactoryEntity(location)` / `GetJsxFragmentFactoryEntity(location)`
  (tsgo/checker/emitresolver.go:53-63, checker-mutex-wrapped) — exactly upstream's
  `state.resolver.getJsxFactoryEntity/getJsxFragmentFactoryEntity`. Internals
  (checker/jsx.go:1405-1434): factory honors per-file `/** @jsx */` pragma then
  `compilerOptions.JsxFactory` then defaults to QualifiedName `React.createElement`
  (jsx.go:1383-1385); fragment honors `/** @jsxfrag */` then
  `compilerOptions.JsxFragmentFactory` then returns **nil** (the "Fragment" fallback is
  roblox-ts's, §2.6). Entities parsed from options are SYNTHETIC: `markAsSynthetic` sets
  `Loc = (-1,-1)` recursively (jsx.go:1444-1448), and they have nil `Parent` — both
  conditions of rotor's `TransformIdentifier` early-out (identifier.go:32) hold.
- `Checker.GetNonOptionalType`, `GetTypeAtLocation` — already wrapped.
- **Import retention**: tsgo's checking pass calls `markJsxAliasReferenced`
  (checker.go:28200-28244, dispatched from `markLinkedReferences` 27981/28028-28031) on
  every JsxOpeningLikeElement/JsxOpeningFragment: resolves the factory's first identifier
  (`React`) as a value use and `markAliasSymbolAsReferenced` — so
  `EmitResolver.IsReferencedAliasDeclaration` (imports.go:53) keeps
  `import React from "@rbxts/react"` alive with NO new rotor work. Precondition (already
  satisfied, digest §7.3 of phase2): rotor runs the per-file semantic pass
  (`preEmitDiagnostics`) before transforming. Verify with the first JSX fixture: line 3
  of every oracle file is `local React = TS.import(script, script.Parent, "node_modules", "@rbxts", "react")`.

### 5.4 Options & sanitizer — NO CHANGES NEEDED (empirical)

`core.CompilerOptions` keeps `Jsx JsxEmit` / `JsxFactory` / `JsxFragmentFactory` /
`JsxImportSource` (compileroptions.go:54-57; enum 527-535: react=3). TS7 did NOT remove
them: `verifyCompilerOptions` (tsgo/compiler/program.go:1071-1095) still validates
jsxFactory (must parse as entity name; incompatible only with `react-jsx`/`react-jsxdev`
and `reactNamespace`) and jsxFragmentFactory (requires jsxFactory). randomness's trio
(`react` + `React.createElement` + `React.Fragment`) passes untouched through
`SanitizeTSConfig` (internal/compile/sanitize.go — touches only downlevelIteration,
baseUrl, moduleResolution, types). **Proven by the CompileProject probe**: zero TS
diagnostics, only rotor not-yet-supported. `.tsx` files are already enumerated
(internal/compile/project.go:31 includes ".tsx"; the source-file loop at project.go:317
accepts ".tsx").

### 5.5 Reusable-but-unexported tsgo helpers

`tsgo/transformers/jsxtransforms/jsx.go`: `fixupWhitespaceAndDecodeEntities` (810-850),
`addLineOfJsxText` (785-793), `decodeEntities` (864-910), `decodeEntity` (912+) and its
entity table. Unexported → COPY into `internal/transformer/jsxtext.go` (do not overlay-
export; the vendored-mirror policy reserves overlays for behavior, not visibility).

---

## 6. Rotor implementation plan sketch

### 6.1 Code layout

- `internal/transformer/jsx.go` — `transformJsxElement`, `transformJsxSelfClosingElement`,
  `transformJsxFragment`, `transformJsxExpression`, `transformJsx`, `transformJsxTagName`
  (+ inner `transformJsxTagNameExpression`), `transformJsxAttributes` (+
  `transformJsxAttribute`, `createJsxAttributeLoop`), `transformJsxChildren`,
  `getTextOfJsxNamespacedName` helper, `findLastIndex` helper.
- `internal/transformer/jsxtext.go` — copied `fixupWhitespaceAndDecodeEntities`,
  `decodeEntities`, entity table (provenance comments → tsgo jsx.go:810+ and upstream
  util/fixupWhitespaceAndDecodeEntities.ts).
- `internal/transformer/dispatch.go` — add to `TransformExpression`:
  `case ast.KindJsxElement / KindJsxExpression / KindJsxFragment / KindJsxSelfClosingElement`
  (mirrors transformExpression.ts L96-99).
- Unit tests `jsx_test.go` + fixture-driven differential tests.

Everything else already exists (verified): pointers, Capture/Prereq/PushToVar(IfComplex),
ensureTransformOrderWith, CreateTruthinessChecks, IsDefinitelyType/IsObjectType,
GetFirstDefinedSymbol, IsMacroOnlyClass, transformEntityName, ValidateIdentifier,
synthetic-identifier handling, all three diagnostics, EmitResolver accessor, luau
TempID/GlobalProperty/Str/NewNone/maps.

### 6.2 Fixture project additions (DO before enabling fixtures; not yet installed)

`testdata/diff/project` currently has only `@rbxts/compiler-types` + `@rbxts/types` and no
jsx config. Spec:

1. `package.json`: add `"@rbxts/react": "17.3.7-ts.1"` (EXACT pin, matching randomness's
   resolved version) to devDependencies-adjacent deps; regenerate `package-lock.json` via
   the project's npm (`npm install` — package-lock.json flow, NOT pnpm).
2. `tsconfig.json` compilerOptions: add
   `"jsx": "react", "jsxFactory": "React.createElement", "jsxFragmentFactory": "React.Fragment"`.
   Zero effect on existing goldens (verified concept: jsx options do not alter non-tsx
   emit; regenerate goldens anyway via `tools/oracle/oracle.ps1` and diff — should be
   no-ops).
3. New fixtures, e.g. `src/32_jsx.tsx` (+ `33_jsx_spread.tsx` etc.), `import React from
   "@rbxts/react"`, covering §3 cases 1-3, 5-8, 10, A-D, F-H, J-L. **Avoid raw text
   children** (type-illegal under @rbxts/react, §1.3) unless a fixture-local
   `children?: any` component is used AND the case passes the real rbxtsc (cases 4/9/E
   above needed a d.ts widening — those exact shapes therefore CANNOT be oracle fixtures;
   keep them covered by Go unit tests on the emitter instead, or restructure with
   `{"text"}` string-literal expression children which ARE legal and exercise the same
   call shape, though NOT the JsxText fixup path).
4. `internal/diff/manifest.go`: append the new fixture basenames.
5. `tools/oracle/oracle.ps1` unchanged (npm install + npx rbxtsc + copy out → golden).
   Node lives under mise; bash sees it (`node 26.1.0`, `npm 11.13.0`), PowerShell needs
   the documented Path re-export.

### 6.3 JsxText coverage strategy

The JsxText pipeline (fixup + entities + backslash doubling) is unreachable in oracle-able
fixtures (§6.2.3). Cover it with table-driven Go tests in `jsxtext_test.go` asserting
against §3 cases 4/9/E outputs (recorded above verbatim) plus the TS-source examples in
fixupWhitespaceAndDecodeEntities.ts comments. Mark in code: "oracle-verified 2026-06-07
via type-widened throwaway; see phase3c digest §3".

### 6.4 Risks

1. ~~tsgo can't check @rbxts/react~~ — ELIMINATED (probe: zero TS diagnostics).
2. React import retention via markJsxAliasReferenced — high confidence (§5.3), verify on
   first fixture; failure mode is a missing `local React = TS.import(...)` line.
3. Inline-map rendering inside call args (multi-line `{...}` tables as middle args,
   §3 case 1/2/7) — rotor's renderer already does this for object literals; eyeball the
   first golden diff.
4. randomness acceptance beyond JSX: 8 of 41 files were blocked on non-JSX features, and
   the other @rbxts packages' d.ts (charm, remo, ui-labs...) may surface new tsgo
   typecheck deltas — out of scope for 3c but expect follow-up noise.
5. Luau `if cond then a else b` child expressions and `cond and <el/>` come from the
   already-ported conditional/binary transforms — no JSX-specific work, but fixture
   §3 case 4-analog (with `{...}` expression children) will catch regressions.
6. tempID naming parity: "attributes", "attribute", "tagName", "exp", "k", "v" — exact
   strings matter for `_attributes_1`-style dedup (§3 cases 5/A/B/C).

---

## 7. Quirks (port VERBATIM)

1. **Underscore/lowercase tag test** (transformJsxTagName.ts L11-16): intrinsic-vs-component
   is decided by `firstChar === firstChar.toLowerCase()` on the raw identifier text —
   NOT by the checker's intrinsic classification. `<_Comp/>` → `React.createElement("_Comp")`
   (string!) even though `_Comp` is a bound function component. Oracle-proven (§3 L).
   Non-ASCII: a leading `Ä` fails the equality (toLowerCase → `ä`) → component path; use
   first-RUNE + `strings.ToLower` comparison in Go for identical behavior.
2. **Backslash doubling in JsxText** (transformJsxChildren.ts L34):
   `text.replace(/\\/g, "\\\\")` AFTER fixup/entity decode, because luau-ast (and rotor's
   renderStringLiteral) emit string values raw. Oracle: `back\slash` → `"back\\slash"`.
   Applies ONLY to JsxText children — attribute string literals go through the normal
   string-literal transform.
3. **Empty-brace attribute initializer is `true`** (transformJsxAttributes.ts L51-58):
   `<frame Visible={}/>` — initializer is a JsxExpression with `expression === undefined`,
   which is replaced by `undefined`, which takes the implicit-true branch → `Visible = true`.
   (Contrast children: `{}` children are FILTERED OUT, §2.4.)
4. **Fragment factory fallback** (transformJsxFragment.ts L15-19): when the resolver
   returns nothing (no jsxFragmentFactory option, no pragma), upstream emits a bare
   synthetic `Fragment` identifier — producing `React.createElement(Fragment, ...)` that
   references a likely-nonexistent local. Deliberate upstream behavior (comment: the
   typechecker defaults to "Fragment", so emit follows). Unreachable for randomness
   (option set) but port the fallback.
5. **Spread clone fast-path conditions** (transformJsxAttributes.ts L85-97): requires the
   spread to be `attributes.properties[0]` (first property OVERALL) AND
   `isDefinitelyType(getNonOptionalType(type), isObjectType)`. Emits
   `local _attributes = table.clone(expr)` + prereq `setmetatable(_attributes, nil)`
   ("Explicitly remove metatable because things like classes can be spread"). Any other
   spread → disableMapInline + k/v `for` loop; non-definitely-object additionally gets
   `pushToVarIfComplex(expr, "attribute")` + truthiness `if` wrapper (§3 A/B).
6. **nil placeholder asymmetry** (transformJsx.ts L37-41 vs transformJsxFragment.ts L27-29):
   elements push `nil` only when there are children AND no attributes; fragments NEVER
   have attributes so they always push `nil` before children. Empty fragment → single-arg
   call.
7. **Child filtering order** (transformJsxChildren.ts L24-30): drop JsxText with
   `containsOnlyTriviaWhiteSpaces`, then drop JsxExpressions with no expression — and the
   spread-must-be-last scan runs on the UNFILTERED list but only for indices strictly
   below `lastJsxChildIndex` (last significant child), so `{...arr}` followed only by
   whitespace-only text is legal.
8. **`key` has no special handling** in 3.0 — it's an ordinary prop in the attributes map
   (§3 case 6). Do not port any Roact Key/children-map logic from 1.x memories.
9. **TagName prereq hoist** (transformJsxTagName.ts L31-36): only when the tag expression
   captured prereqs does it get `pushToVarIfComplex(..., "tagName")`; a plain
   `NS.Deep.Comp` chain stays inline (§3 case I).
10. **Attribute-prereq map disabling order** (transformJsxAttributes.ts L60-63):
    `disableMapInline` runs BEFORE `prereqList`, so the `local _attributes = {…so far…}`
    line precedes the initializer's prereq statements (§3 case C: `_attributes_2 = {}`
    first, then the pop expansion, then `.Visible = _result`).
11. **noMacroObjectSpread fires on JSX spreads too** (transformJsxAttributes.ts L77-81):
    spreading a macro-only class (ReadonlyMap etc.) in attributes is the same diagnostic
    as object-literal spreads.
12. **Namespaced names render with a colon**: `<a:b c:d="x"/>` → tag string `"a:b"`,
    prop key `"c:d"` (bracket-quoted by the renderer since not a valid identifier).
13. **JsxExpression spread child** uses global `unpack` (NOT `table.unpack`)
    (transformJsxExpression.ts L10, oracle §3 case 10).

---

## Appendix: probe inventory (all deleted)

- `%TEMP%\rotor-jsx-oracle` — fixture-project copy + @rbxts/react + jsx tsconfig +
  `_scratch.tsx/_scratch2.tsx/_scratch3.tsx/_scratch4.tsx`. NOTE: `robocopy /XD out`
  also excludes `node_modules/*/out` (roblox-ts, @roblox-ts/luau-ast, path-translator,
  rojo-resolver) — restore them or exclude only the top-level out when copying.
- `%TEMP%\rotor-probe-repo` — rotor repo copy + `internal/compile/jsxprobe_scratch_test.go`
  calling `CompileProject` on the oracle project.
- @rbxts/react d.ts widening used ONLY for text-children typechecks:
  `type ReactNode = ... | string | number | ReadonlyArray<ReactNode>` (index.d.ts:314).
