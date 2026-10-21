package compile

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

func TestCompileFileRejectsReferencedFunctionAfterReturn(test *testing.T) {
	projectDirectory := writeProject(test, "@scope/unreachable-function", "")
	const source = `export function ClosureAfterReturn(value: number): () => number {
	const later = () => read();
	return later;
	function read(): number { return value; }
}
`
	if writeError := os.WriteFile(filepath.Join(projectDirectory, "src", "main.ts"), []byte(source), 0o644); writeError != nil {
		test.Fatal(writeError)
	}
	text, diagnostics, compileError := CompileFile(projectDirectory, "src/main.ts")
	if compileError != nil {
		test.Fatalf("CompileFile: %v (diagnostics: %v)", compileError, diagnostics)
	}
	const want = "Function \"read\" is unreachable after a final statement.\nDeclare the function before the first statement that uses it."
	if len(diagnostics) != 1 || diagnostics[0] != want {
		test.Fatalf("diagnostics = %v, want %q (output: %q)", diagnostics, want, text)
	}
	if text != "" {
		test.Fatalf("output = %q, want no output after diagnostic", text)
	}
}

func TestCompileFileElidesUnreferencedFunctionAfterReturn(test *testing.T) {
	projectDirectory := writeProject(test, "@scope/unreachable-function", "")
	const source = `export function NoReference(): number {
	return 1;
	function unused(): number { return 2; }
}
`
	if writeError := os.WriteFile(filepath.Join(projectDirectory, "src", "main.ts"), []byte(source), 0o644); writeError != nil {
		test.Fatal(writeError)
	}
	text, diagnostics, compileError := CompileFile(projectDirectory, "src/main.ts")
	if compileError != nil || len(diagnostics) != 0 {
		test.Fatalf("CompileFile: %v (diagnostics: %v)", compileError, diagnostics)
	}
	const want = `-- Compiled with sloptor v2.9.0-beta.1
local function NoReference()
	return 1
end
return {
	NoReference = NoReference,
}
`
	if text != want {
		test.Fatalf("output = %q, want %q", text, want)
	}
}

func TestCompileFilePreservesReachableHoistedFunction(test *testing.T) {
	projectDirectory := writeProject(test, "@scope/unreachable-function", "")
	const source = `export function Reachable(value: number): () => number {
	const later = () => read();
	function read(): number { return value; }
	return later;
}
`
	if writeError := os.WriteFile(filepath.Join(projectDirectory, "src", "main.ts"), []byte(source), 0o644); writeError != nil {
		test.Fatal(writeError)
	}
	text, diagnostics, compileError := CompileFile(projectDirectory, "src/main.ts")
	if compileError != nil || len(diagnostics) != 0 {
		test.Fatalf("CompileFile: %v (diagnostics: %v)", compileError, diagnostics)
	}
	const want = `-- Compiled with sloptor v2.9.0-beta.1
local function Reachable(value)
	local read
	local later = function()
		return read()
	end
	function read()
		return value
	end
	return later
end
return {
	Reachable = Reachable,
}
`
	if text != want {
		test.Fatalf("output = %q, want %q", text, want)
	}
}

func TestCompileFileRejectsReferencedFunctionAfterLoopExit(test *testing.T) {
	for _, finalStatement := range []string{"break", "continue"} {
		test.Run(finalStatement, func(test *testing.T) {
			projectDirectory := writeProject(test, "@scope/unreachable-function", "")
			source := fmt.Sprintf(`export function Loop(value: number): void {
	while (value > 0) {
		const later = () => read();
		%s;
		function read(): number { return value; }
	}
}
`, finalStatement)
			if writeError := os.WriteFile(filepath.Join(projectDirectory, "src", "main.ts"), []byte(source), 0o644); writeError != nil {
				test.Fatal(writeError)
			}
			text, diagnostics, compileError := CompileFile(projectDirectory, "src/main.ts")
			if compileError != nil {
				test.Fatalf("CompileFile: %v (diagnostics: %v)", compileError, diagnostics)
			}
			const want = "Function \"read\" is unreachable after a final statement.\nDeclare the function before the first statement that uses it."
			if len(diagnostics) != 1 || diagnostics[0] != want {
				test.Fatalf("diagnostics = %v, want %q (output: %q)", diagnostics, want, text)
			}
			if text != "" {
				test.Fatalf("output = %q, want no output after diagnostic", text)
			}
		})
	}
}
