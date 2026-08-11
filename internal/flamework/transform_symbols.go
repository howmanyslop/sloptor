package flamework

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
)

func buildSymbolIDIntrinsic(state *TransformState, call *ast.Node, target *checker.Type) (*ast.Node, error) {
	if typeArguments := call.TypeArgumentList(); typeArguments != nil && len(typeArguments.Nodes) > 0 {
		id, err := nodeUID(state, typeArguments.Nodes[0])
		return state.factory.NewStringLiteral(id, ast.TokenFlagsNone), err
	}
	id, err := typeUID(state, target, call)
	return state.factory.NewStringLiteral(id, ast.TokenFlagsNone), err
}

func buildDeclarationUIDIntrinsic(state *TransformState, trace *ast.Node) (*ast.Node, error) {
	declaration := ast.FindAncestor(trace, func(node *ast.Node) bool {
		return ast.IsDeclaration(node) && ast.GetNameOfDeclaration(node) != nil
	})
	if declaration == nil {
		return nil, invalidMacro(trace, "This function must be under a variable declaration.")
	}
	id, err := nodeUID(state, declaration)
	return state.factory.NewStringLiteral(id, ast.TokenFlagsNone), err
}

func nodeUID(state *TransformState, node *ast.Node) (string, error) {
	if ast.IsTypeReferenceNode(node) {
		return nodeUID(state, node.AsTypeReferenceNode().TypeName.AsNode())
	}
	if ast.IsTypeQueryNode(node) {
		return nodeUID(state, node.AsTypeQueryNode().ExprName.AsNode())
	}
	if declaration := declarationFromNode(state, node); declaration != nil {
		return declarationUID(state, declaration)
	}
	target := state.checker.GetTypeAtLocation(node)
	return typeUID(state, target, node)
}

func typeUID(state *TransformState, target *checker.Type, trace *ast.Node) (string, error) {
	if target.Symbol() != nil {
		return symbolUID(state, target.Symbol(), trace)
	}
	if target.IsStringLiteral() {
		return "$ps:" + stringLiteralValue(target), nil
	}
	if target.IsNumberLiteral() {
		return "$pn:" + target.AsLiteralType().String(), nil
	}
	if target.Flags()&checker.TypeFlagsIntrinsic != 0 {
		return "$p:" + target.AsIntrinsicType().IntrinsicName(), nil
	}
	if target.Flags()&(checker.TypeFlagsObject|checker.TypeFlagsIntersection|checker.TypeFlagsUnion) != 0 {
		return "$p:defined", nil
	}
	return "", invalidMacro(trace, "Could not find UID for type %q", state.checker.TypeToString(target))
}

func symbolUID(state *TransformState, symbol *ast.Symbol, trace *ast.Node) (string, error) {
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	declaration := symbol.ValueDeclaration
	if declaration == nil && len(symbol.Declarations) > 0 {
		declaration = symbol.Declarations[0]
	}
	if declaration == nil {
		return "", invalidMacro(trace, "Could not find UID for symbol %q", symbol.Name)
	}
	return declarationUID(state, declaration)
}

func declarationUID(state *TransformState, declaration *ast.Node) (string, error) {
	name := declarationFullName(declaration)
	if name == "" {
		return "", invalidMacro(declaration, "Could not determine declaration name")
	}
	file := ast.GetSourceFileOfNode(declaration)
	if !pathWithin(state.project.PathTranslator().RootDir, file.FileName()) {
		return packageDeclarationUID(state, declaration, name)
	}
	output := state.project.PathTranslator().GetOutputPath(file.FileName())
	relative, err := filepath.Rel(state.project.RootDirectory(), output)
	if err != nil {
		return "", fmt.Errorf("resolve Flamework declaration path: %w", err)
	}
	relative = strings.TrimSuffix(filepath.ToSlash(relative), filepath.Ext(relative))
	internalID := state.project.PackageName() + ":" + relative + "@" + name
	return state.project.Identifier(DeclarationIdentity{
		InternalID:      internalID,
		DeclarationName: name,
		LuaFileName:     luaFileName(file.FileName()),
	})
}

func packageDeclarationUID(state *TransformState, declaration *ast.Node, name string) (string, error) {
	fileName := ast.GetSourceFileOfNode(declaration).FileName()
	packageJSON := findPackageJSON(filepath.Dir(fileName))
	if packageJSON == "" {
		return "", invalidMacro(declaration, "Could not find package for declaration %q", name)
	}
	data, err := os.ReadFile(packageJSON)
	if err != nil {
		return "", fmt.Errorf("read Flamework declaration package: %w", err)
	}
	var identity struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(data, &identity); err != nil || identity.Name == "" {
		return "", invalidMacro(declaration, "Could not read package identity for declaration %q", name)
	}
	relative, err := filepath.Rel(filepath.Dir(packageJSON), fileName)
	if err != nil {
		return "", fmt.Errorf("resolve packaged Flamework declaration path: %w", err)
	}
	relative = strings.TrimSuffix(strings.TrimSuffix(filepath.ToSlash(relative), ".ts"), ".d")
	if strings.HasSuffix(relative, "/index") {
		relative = strings.TrimSuffix(relative, "/index") + "/init"
	}
	internalID := identity.Name + ":" + relative + "@" + name
	packagePrefix := ""
	buildPath := filepath.Join(filepath.Dir(packageJSON), "flamework.build")
	if _, statErr := os.Stat(buildPath); statErr == nil {
		info, loadErr := LoadBuildInfo(buildPath, FlameworkVersion)
		if loadErr != nil {
			return "", fmt.Errorf("load packaged Flamework declaration metadata: %w", loadErr)
		}
		packagePrefix, _ = info.Snapshot().IdentifierPrefix()
	}
	return state.project.Identifier(DeclarationIdentity{
		InternalID:      internalID,
		DeclarationName: name,
		LuaFileName:     luaFileName(fileName),
		PackagePrefix:   packagePrefix,
		IsPackage:       true,
	})
}

func pathWithin(root, candidate string) bool {
	relative, err := filepath.Rel(root, candidate)
	return err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator)) && !filepath.IsAbs(relative)
}

func declarationFromNode(state *TransformState, node *ast.Node) *ast.Node {
	if ast.IsDeclaration(node) && ast.GetNameOfDeclaration(node) != nil {
		return node
	}
	symbol := state.checker.GetSymbolAtLocation(node)
	if symbol == nil {
		return nil
	}
	if symbol.Flags&ast.SymbolFlagsAlias != 0 {
		symbol = state.checker.GetAliasedSymbol(symbol)
	}
	if symbol.ValueDeclaration != nil {
		return symbol.ValueDeclaration
	}
	if len(symbol.Declarations) > 0 {
		return symbol.Declarations[0]
	}
	return nil
}

func declarationFullName(declaration *ast.Node) string {
	parts := make([]string, 0, 2)
	for current := declaration; current != nil && !ast.IsSourceFile(current); current = current.Parent {
		if name := ast.GetNameOfDeclaration(current); name != nil && ast.IsIdentifier(name) {
			parts = append(parts, name.Text())
		}
	}
	for left, right := 0, len(parts)-1; left < right; left, right = left+1, right-1 {
		parts[left], parts[right] = parts[right], parts[left]
	}
	return strings.Join(parts, ".")
}
