package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCompileFileJsxUsesComponentBinding(t *testing.T) {
	for _, name := range []string{"_View", "View"} {
		t.Run(name, func(t *testing.T) {
			dir := writeJsxComponentProject(t, name)
			got, diagnostics, err := CompileFile(dir, "src/view.tsx")
			if err != nil || len(diagnostics) != 0 {
				t.Fatalf("CompileFile: %v (diagnostics: %v)", err, diagnostics)
			}
			if !strings.Contains(got, "React.createElement("+name+")") {
				t.Fatalf("JSX did not use the component binding:\n%s", got)
			}
		})
	}
}

func TestCompileFileJsxRejectsDollarComponentBinding(t *testing.T) {
	dir := writeJsxComponentProject(t, "$View")
	_, diagnostics, err := CompileFile(dir, "src/view.tsx")
	if err != nil || len(diagnostics) != 1 || !strings.Contains(diagnostics[0], "Invalid Luau identifier!") {
		t.Fatalf("CompileFile: %v (diagnostics: %v), want invalid identifier diagnostic", err, diagnostics)
	}
}

func writeJsxComponentProject(t *testing.T, name string) string {
	t.Helper()
	dir := writeProject(t, "@scope/jsx-component-binding", "")
	configPath := filepath.Join(dir, "tsconfig.json")
	config := strings.Replace(string(mustReadFile(t, configPath)), `"compilerOptions": {`, `"compilerOptions": {
		"jsx": "react",
		"jsxFactory": "React.createElement",`, 1)
	if err := os.WriteFile(configPath, []byte(config), 0o644); err != nil {
		t.Fatal(err)
	}
	writeFiles(t, dir, map[string]string{
		"src/jsx.d.ts": "declare namespace JSX { interface Element {} }\ndeclare const React: { createElement(component: unknown): JSX.Element };\n",
		"src/view.tsx": "export function Example({ component: " + name + " }: { readonly component: () => JSX.Element }): JSX.Element { return <" + name + " />; }\n",
	})
	return dir
}
