package flamework

import "rotor/tsgo/ast"

const flameworkPreludeModule = "@flamework/core/out/prelude"

func decoratorRuntimeImports(expressions, prerequisites []*ast.Node, imports []MacroImport) []MacroImport {
	for _, node := range append(append([]*ast.Node(nil), expressions...), prerequisites...) {
		if nodeContainsIdentifier(node, "t") {
			return append(imports, MacroImport{Module: flameworkPreludeModule, Export: "t", Local: "t"})
		}
	}
	return imports
}

func nodeContainsIdentifier(node *ast.Node, name string) bool {
	if node == nil {
		return false
	}
	if ast.IsIdentifier(node) && node.Text() == name {
		return true
	}
	for child := range node.IterChildren() {
		if nodeContainsIdentifier(child, name) {
			return true
		}
	}
	return false
}
