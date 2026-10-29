package transformer

import (
	"slices"
	"strconv"
	"strings"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/tspath"
)

// Named TS types are emitted once as file-level Luau type aliases, so a
// recursive interface stays recursive instead of collapsing to `{ [any]: any }`
// and a referenced interface keeps its name. The alias is registered before its
// body is built, which is what lets a self reference resolve to the name.
//
// Only interfaces and non-generic type aliases of object types are aliased; a
// generic type alias is expanded inline because relating an alias's type
// arguments to the checker's instantiated reference is not worth the risk.
// Library types (lib.d.ts, @rbxts/compiler-types, @rbxts/types) are never
// aliased: they are spelled structurally, or by name for the Roblox allowlist.

// nativeAliasEntry is one registered `type` statement.
type nativeAliasEntry struct {
	Symbol    *ast.Symbol
	Statement *luau.TypeAlias
	Generics  []string
	// Kind is what the alias names at runtime: a table, or a function for an
	// alias of a callable type.
	Kind nativeTypeKind
}

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

// getNativeAliasReference returns the alias spelling for a named interface or a
// non-generic type alias of an object type, registering the alias on first use.
func getNativeAliasReference(state *State, checkerType *checker.Type, depth int, seen map[*checker.Type]bool) (string, nativeTypeKind, bool) {
	symbol := getNativeAliasSymbol(state, checkerType)
	if symbol == nil {
		return "", nativeKindUnspellable, false
	}
	entry := state.nativeAliasBySymbol[symbol]
	if entry == nil {
		entry = state.registerNativeAlias(symbol)
		if entry == nil {
			return "", nativeKindUnspellable, false
		}
	}
	arguments := getTypeArguments(state, checkerType)
	if len(entry.Generics) != len(arguments) {
		return "", nativeKindUnspellable, false
	}
	if len(arguments) == 0 {
		return entry.Statement.Name, entry.Kind, true
	}
	parts := make([]string, len(arguments))
	for i, argument := range arguments {
		text, _, _ := getNativeType(state, argument, nativePositionNested, depth+1, seen)
		if text == "" {
			text = nativeTypeAny
		}
		parts[i] = text
	}
	return entry.Statement.Name + "<" + strings.Join(parts, ", ") + ">", entry.Kind, true
}

// getNativeAliasSymbol reports the symbol a type can be aliased by, or nil.
func getNativeAliasSymbol(state *State, checkerType *checker.Type) *ast.Symbol {
	symbol := checkerType.Symbol()
	if symbol != nil && symbol.Flags&ast.SymbolFlagsInterface != 0 && symbol.Flags&ast.SymbolFlagsClass == 0 {
		if isNativeLibrarySymbol(state, symbol) || !getNativeAliasableDeclaration(symbol, ast.IsInterfaceDeclaration) {
			return nil
		}
		// A callable interface is a function type, not a table.
		if len(state.Checker.GetCallSignatures(checkerType)) > 0 || len(state.Checker.GetConstructSignatures(checkerType)) > 0 {
			return nil
		}
		return symbol
	}
	alias := checkerType.Alias()
	if alias == nil {
		return nil
	}
	aliasSymbol := alias.Symbol()
	if aliasSymbol == nil || aliasSymbol.Flags&ast.SymbolFlagsTypeAlias == 0 {
		return nil
	}
	if isNativeLibrarySymbol(state, aliasSymbol) || !getNativeAliasableDeclaration(aliasSymbol, ast.IsTypeAliasDeclaration) {
		return nil
	}
	if len(alias.TypeArguments()) > 0 || checkerType.Flags()&checker.TypeFlagsObject == 0 {
		// Generic type aliases are expanded inline.
		return nil
	}
	return aliasSymbol
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

// registerNativeAlias reserves the alias name, then builds its body with the
// alias's own generics in scope. It returns nil when the body cannot be spelled
// and the type should be expanded inline instead.
func (state *State) registerNativeAlias(symbol *ast.Symbol) *nativeAliasEntry {
	declaration := getNativeAliasDeclaration(symbol)
	if declaration == nil {
		return nil
	}
	generics := nativeDeclarationGenerics(declaration)
	name := state.getUniqueNativeAliasName(symbol.Name)
	statement := luau.NewTypeAlias(name, generics, "")
	entry := &nativeAliasEntry{Symbol: symbol, Statement: statement, Generics: generics, Kind: nativeKindTable}
	if !ast.IsInterfaceDeclaration(declaration) && len(state.Checker.GetCallSignatures(state.Checker.GetDeclaredTypeOfSymbol(symbol))) > 0 {
		entry.Kind = nativeKindFunction
	}
	state.nativeAliasBySymbol[symbol] = entry
	state.nativeAliasByName[name] = entry
	state.nativeAliases = append(state.nativeAliases, statement)

	// A barrier scope: the body sees the alias's own generics, never the
	// enclosing function's.
	state.PushNativeGenerics(generics, true)
	body := getNativeAliasBody(state, symbol, ast.IsInterfaceDeclaration(declaration))
	state.PopNativeGenerics()

	if body == "" {
		if !ast.IsInterfaceDeclaration(declaration) {
			// Expanded inline by the caller.
			delete(state.nativeAliasBySymbol, symbol)
			delete(state.nativeAliasByName, name)
			state.nativeAliases = slices.DeleteFunc(state.nativeAliases, func(a *luau.TypeAlias) bool { return a == statement })
			return nil
		}
		// A table that cannot be written out is still a table.
		body = nativeTypeAnyTable
	}
	statement.Type = body
	return entry
}

func getNativeAliasDeclaration(symbol *ast.Symbol) *ast.Node {
	for _, declaration := range symbol.Declarations {
		if ast.IsInterfaceDeclaration(declaration) || ast.IsTypeAliasDeclaration(declaration) {
			return declaration
		}
	}
	return nil
}

// getNativeAliasBody spells the DECLARED type, so members mention the interface's
// own type parameter names.
func getNativeAliasBody(state *State, symbol *ast.Symbol, isInterface bool) string {
	declared := state.Checker.GetDeclaredTypeOfSymbol(symbol)
	if declared == nil {
		return ""
	}
	if !isInterface && len(state.Checker.GetCallSignatures(declared)) > 0 {
		text, kind, _ := getNativeFunctionType(state, declared, false, 0, nil)
		if kind == nativeKindFunction && text != nativeTypeAnyFunction {
			return text
		}
		return ""
	}
	return getNativeTableTypeAt(state, declared, 0, nil)
}

// getUniqueNativeAliasName picks a free, spellable alias name, suffixing `_2`,
// `_3`, ... on a reserved word or a name already taken by another type.
func (state *State) getUniqueNativeAliasName(base string) string {
	name := base
	if !luau.IsValidIdentifier(name) {
		name = "Alias"
	}
	for suffix := 1; ; suffix += 1 {
		candidate := name
		if suffix > 1 {
			candidate = name + "_" + strconv.Itoa(suffix)
		}
		_, taken := state.nativeAliasByName[candidate]
		_, reserved := nativeReservedTypeAliasNames[candidate]
		if !taken && !reserved && luau.IsValidIdentifier(candidate) {
			return candidate
		}
	}
}

// isNativeLibrarySymbol reports whether a declaration of symbol lives in a
// library file rotor must not alias.
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
