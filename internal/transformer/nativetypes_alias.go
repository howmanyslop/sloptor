package transformer

import (
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/tspath"
)

// Named TS types are emitted once as file-level Luau type aliases, so a
// recursive interface stays recursive instead of collapsing to `{ [any]: any }`
// and a referenced interface keeps its name. The registry is the shared one
// (luautypes.go); this file holds what the `//!native` spelling decides about
// it: which symbols are aliased at all (nativeSpelling.ShouldAlias, below),
// and where a statement is emitted.
//
// Only interfaces and non-generic type aliases of object types are aliased; a
// generic type alias is expanded inline because relating an alias's type
// arguments to the checker's instantiated reference is not worth the risk.
// Library types (lib.d.ts, @rbxts/compiler-types, @rbxts/types) are never
// aliased: they are spelled structurally, or by name for the Roblox allowlist.

// nativeReservedTypeAliasNames are names a type alias must not take, because
// Luau already reads them as something else.
var nativeReservedTypeAliasNames = map[string]struct{}{
	"number": {}, "string": {}, "boolean": {}, "any": {}, "nil": {}, "never": {},
	"unknown": {}, "vector": {}, "Vector3": {}, "buffer": {}, "thread": {},
	"table": {}, "typeof": {},
}

func isNativeVectorTypeName(name string) bool {
	return name == "vector" || name == "Vector3"
}

// isNativeAliasableSymbol is the `//!native` alias policy: a non-class
// interface without call signatures (a callable interface is a function type,
// not a table), or a non-generic type alias of an object type; never a
// library type, a LuaTuple (a pack is only spellable as a function return, so
// `type Pair = (A, B)` would not parse), an enum or a class.
func isNativeAliasableSymbol(state *State, symbol *ast.Symbol, declaration *ast.Node) bool {
	if declaration == nil || isNativeLibrarySymbol(state, symbol) {
		return false
	}
	declared := state.Checker.GetDeclaredTypeOfSymbol(symbol)
	if declared == nil || IsLuaTupleType(state).Check(declared) {
		return false
	}
	switch {
	case symbol.Flags&ast.SymbolFlagsInterface != 0 && symbol.Flags&ast.SymbolFlagsClass == 0:
		return getNativeAliasableDeclaration(symbol, ast.IsInterfaceDeclaration) &&
			len(state.Checker.GetCallSignatures(declared)) == 0
	case symbol.Flags&ast.SymbolFlagsTypeAlias != 0:
		if !getNativeAliasableDeclaration(symbol, ast.IsTypeAliasDeclaration) || !ast.IsTypeAliasDeclaration(declaration) {
			return false
		}
		// A generic type alias is expanded inline.
		return len(declaration.TypeParameters()) == 0 && declared.Flags()&checker.TypeFlagsObject != 0
	}
	return false
}

// getNativeAliasableDeclaration reports whether the symbol has a declaration of
// the expected kind whose type parameters Luau can all spell.
func getNativeAliasableDeclaration(symbol *ast.Symbol, isKind func(*ast.Node) bool) bool {
	for _, declaration := range symbol.Declarations {
		if !isKind(declaration) {
			continue
		}
		for _, parameter := range declaration.TypeParameters() {
			name := parameter.Name()
			if name == nil || !luau.IsValidIdentifier(name.Text()) {
				return false
			}
		}
		return true
	}
	return false
}

// nativeDeclarationAlias returns the `type` statement to emit AT a top-level
// interface or type alias declaration of this file, or an empty list. The alias
// is registered here when nothing referenced it earlier, so it sits with the
// TS declaration (after its leading comments) rather than at the top of the
// file.
//
// Luau aliases are visible before their declaration line within the same block,
// so a use above the declaration keeps working. A merged symbol emits one alias,
// at its FIRST declaration: the checker's declared type already carries every
// merged member. Declarations anywhere but at the top level of this file (a
// namespace, function, or block; an import; an ambient/library type) stay on the
// hoist-to-header path, where they cannot fall out of scope.
func (state *State) nativeDeclarationAlias(node *ast.Node) *luau.List[luau.Statement] {
	statements := luau.NewList[luau.Statement]()
	if !state.IsNative || state.SourceFile == nil {
		return statements
	}
	name := node.Name()
	if name == nil {
		return statements
	}
	symbol := state.Checker.GetSymbolAtLocation(name)
	if symbol == nil || state.luauFirstDeclaration(symbol) != node {
		return statements
	}
	entry := state.typeAliasFor(symbol)
	if entry == nil || entry.Statement == nil || entry.Placement != atDeclaration || entry.placed {
		return statements
	}
	entry.placed = true
	statements.Push(entry.Statement)
	return statements
}

// nativeHoistedTypeStatements lists the `type` statements registered while
// annotations were built that no declaration placed, in registration order,
// for prependHeader to put after the header. Unlike the luauTypes hoist the
// statements carry no export marker and no loss comments: `//!native` output
// is annotations only. With luauTypes on as well, luauTypes places every
// alias (prependHeader asks luauHoistedTypeStatements instead) and the
// native builder's registry is never emitted.
func (state *State) nativeHoistedTypeStatements() *luau.List[luau.Statement] {
	statements := luau.NewList[luau.Statement]()
	if state.IsLuauTyped || state.luauTypeBuilder == nil {
		return statements
	}
	for _, entry := range state.luauTypeBuilder.Aliases() {
		if entry.placed || entry.Statement == nil {
			continue
		}
		entry.placed = true
		statements.Push(entry.Statement)
	}
	return statements
}

// isNativeLibrarySymbol reports whether a declaration of symbol lives in a
// library file. The `//!native` aliases never alias such a symbol; the
// luauTypes registry mirrors it as a hoisted alias instead (see
// luautypes_alias.go), so isNativeLibraryFile answers "is this a library
// file", not "must this stay unaliased".
func isNativeLibrarySymbol(state *State, symbol *ast.Symbol) bool {
	for _, declaration := range symbol.Declarations {
		sourceFile := ast.GetSourceFileOfNode(declaration)
		if sourceFile == nil {
			continue
		}
		if isNativeLibraryFile(state, sourceFile) {
			return true
		}
	}
	return false
}

func isNativeLibraryFile(state *State, file *ast.SourceFile) bool {
	name := strings.ReplaceAll(file.FileName(), "\\", "/")
	if strings.Contains(name, "node_modules/@rbxts/") || strings.Contains(name, "node_modules/typescript/") {
		return true
	}
	if state.Program != nil {
		return state.Program.IsLibFile(file)
	}
	return strings.HasPrefix(tspath.GetBaseFileName(name), "lib.")
}

// nativeRobloxTypeAllowlist is the set of Roblox datatypes that name a real
// type in Luau, plus `Instance` (subclasses are recognised through their base
// chain).
var nativeRobloxTypeAllowlist = map[string]struct{}{
	"Axes": {}, "BrickColor": {}, "CFrame": {}, "Color3": {}, "ColorSequence": {},
	"ColorSequenceKeypoint": {}, "DateTime": {}, "Faces": {}, "FloatCurveKey": {},
	"Font": {}, "Instance": {}, "NumberRange": {}, "NumberSequence": {},
	"NumberSequenceKeypoint": {}, "OverlapParams": {}, "PathWaypoint": {},
	"PhysicalProperties": {}, "Random": {}, "Ray": {}, "RaycastParams": {},
	"RaycastResult": {}, "RBXScriptConnection": {}, "RBXScriptSignal": {},
	"Rect": {}, "Region3": {}, "Region3int16": {}, "TweenInfo": {}, "UDim": {},
	"UDim2": {}, "Vector2": {}, "Vector2int16": {}, "Vector3": {}, "Vector3int16": {},
	"vector": {},
}

// getNativeRobloxTypeName is the Luau name of an allowlisted Roblox type: a listed
// datatype, an Instance subclass, or "" for anything else (colour sequences and
// friends are represented by plain tables at runtime).
func getNativeRobloxTypeName(state *State, checkerType *checker.Type) string {
	if !IsRobloxType(state).Check(checkerType) {
		return ""
	}
	symbol := checkerType.Symbol()
	if symbol == nil || !luau.IsValidIdentifier(symbol.Name) {
		return ""
	}
	if _, ok := nativeRobloxTypeAllowlist[symbol.Name]; ok {
		return symbol.Name
	}
	for _, base := range getRecursiveBaseTypes(state, checkerType) {
		baseSymbol := base.Symbol()
		if baseSymbol != nil && baseSymbol.Name == "Instance" && IsRobloxType(state).Check(base) {
			return symbol.Name
		}
	}
	return ""
}
