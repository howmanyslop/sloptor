package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ----------------------------------------------------------------------------
// rotor extension: Luau's native `vector` carries Vector3's math methods
// (propertycallmacros.go `vector` row) — `a.sub(b)` compiles to `a - b`, never
// the meaningless `a:sub(b)` (a vector value has no methods; indexing one at
// runtime errors). These tests pin the emit and the registration tolerance
// for types packages that predeclare the methods' absence (see
// macromanager.go optionalPropertyCallClasses).
// ----------------------------------------------------------------------------

// vectorMathTypes is the `declare interface vector` block that newer @rbxts/types
// releases ship in include/macro_math.d.ts — verbatim, because the registration
// loop must recognize the real declaration shape (method signatures merged into
// the ambient `interface vector` of roblox.d.ts).
const vectorMathTypes = `
declare interface vector {
	/** macro for vector + vector */
	add(this: vector, v: vector): vector;
	/** macro for vector - vector */
	sub(this: vector, v: vector): vector;
	/** macro for vector * vector | number */
	mul(this: vector, other: vector | number): vector;
	/** macro for vector / vector | number */
	div(this: vector, other: vector | number): vector;
	/** macro for vector // vector | number */
	idiv(this: vector, other: vector | number): vector;
}
`

// stripCompileHeader returns text without its first line, asserting that line
// is the version banner (`-- Compiled with sloptor vX.Y.Z`) so expected-output
// literals never churn on release bumps.
func stripCompileHeader(t *testing.T, text string) string {
	t.Helper()
	name, rest, ok := strings.Cut(text, "\n")
	if !ok {
		t.Fatalf("compiled output has no header line:\n%s", text)
	}
	if !strings.HasPrefix(name, "-- Compiled with sloptor v") {
		t.Fatalf("compiled output header = %q, want version banner", name)
	}
	return rest
}

func writeProjectFile(t *testing.T, dir, name, contents string) {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}

// TestVectorMathMethodsCompileToOperators: with the vector macro math API
// declared (the @rbxts/types shape), every math method compiles to its Luau
// operator — the reported bug was `minuend.sub(subtrahend)` emitting
// `minuend:sub(subtrahend)`. The subtract fixture is the original report:
// its operand temps (`local _minuend = minuend`) are runCallMacro's mutation
// pinning (parameters are mutable symbols — isSymbolMutable.ts), identical to
// the report's own Vector3-cast output. The const-based ops fixture mirrors
// testdata/diff/golden/25_mathmacros.luau's Vector3 emit shape byte for byte.
func TestVectorMathMethodsCompileToOperators(t *testing.T) {
	dir := buildAuditProject(t)

	macroMathPath := filepath.Join(dir, "node_modules", "@rbxts", "types", "include", "macro_math.d.ts")
	data, err := os.ReadFile(macroMathPath)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(macroMathPath, append(data, []byte(vectorMathTypes)...), 0o644); err != nil {
		t.Fatal(err)
	}

	writeProjectFile(t, dir, filepath.Join("src", "main.ts"), `function subtract(minuend: vector, subtrahend: vector): vector {
	let { x, y, z } = minuend.sub(subtrahend);

	if (z < 0) {
		z += 2 ** 22;
		y -= 1;
	}

	if (y < 0) {
		y += 2 ** 20;
		x -= 1;
	}

	if (x < 0) x += 2 ** 22;
	return vector.create(x, y, z);
}

const a = vector.create(1, 2, 3);
const b = vector.create(4, 5, 6);
const n = 2;
print(a.add(b), a.sub(b), a.mul(b), a.div(b), a.idiv(b));
print(a.mul(n), a.div(n), a.idiv(n));
print(a.add(b).mul(b));
print(a.mul(b.add(b)));
print(subtract(a, b));
`)

	text, diags, err := CompileFile(dir, "src/main.ts")
	if err != nil {
		t.Fatalf("CompileFile: %v (diags: %v)", err, diags)
	}
	if len(diags) != 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}

	want := `local function subtract(minuend, subtrahend)
	local _minuend = minuend
	local _subtrahend = subtrahend
	local _binding = _minuend - _subtrahend
	local x = _binding.x
	local y = _binding.y
	local z = _binding.z
	if z < 0 then
		z += 2 ^ 22
		y -= 1
	end
	if y < 0 then
		y += 2 ^ 20
		x -= 1
	end
	if x < 0 then
		x += 2 ^ 22
	end
	return vector.create(x, y, z)
end
local a = vector.create(1, 2, 3)
local b = vector.create(4, 5, 6)
local n = 2
print(a + b, a - b, a * b, a / b, a // b)
print(a * n, a / n, a // n)
print((a + b) * b)
local _arg0 = b + b
print(a * _arg0)
print(subtract(a, b))
return nil
`
	if got := stripCompileHeader(t, text); got != want {
		t.Errorf("compiled output differs:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestVectorMathMethodsMissingFromTypesAreTolerated: the macro math API for
// `vector` is absent from older @rbxts/types releases (the pinned fixtures
// included), and such projects must keep compiling — a method their types do
// not declare can never be called, so its absence is not the audit's
// silent-regression class. The Vector3 call in the fixture also proves the
// strict rows still macro-ize alongside the tolerated one.
func TestVectorMathMethodsMissingFromTypesAreTolerated(t *testing.T) {
	dir := buildAuditProject(t)

	writeProjectFile(t, dir, filepath.Join("src", "main.ts"), `const v = vector.create(1, 2, 3);
print(v.x, v.y, v.z);
const a = new Vector3(1, 2, 3);
const b = new Vector3(4, 5, 6);
print(a.add(b), a.sub(b));
`)

	text, diags, err := CompileFile(dir, "src/main.ts")
	if err != nil {
		t.Fatalf("CompileFile: %v (diags: %v)", err, diags)
	}
	if len(diags) != 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}

	want := `local v = vector.create(1, 2, 3)
print(v.x, v.y, v.z)
local a = Vector3.new(1, 2, 3)
local b = Vector3.new(4, 5, 6)
print(a + b, a - b)
return nil
`
	if got := stripCompileHeader(t, text); got != want {
		t.Errorf("compiled output differs:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestVectorMathMethodsFromProjectDeclaration: a project-side augmentation
// (the escape hatch for users whose @rbxts/types predates the vector macro
// math API) merges into the ambient `interface vector` and macro-izes the
// same way the package declaration does.
func TestVectorMathMethodsFromProjectDeclaration(t *testing.T) {
	dir := buildAuditProject(t)

	writeProjectFile(t, dir, filepath.Join("src", "vector.d.ts"), vectorMathTypes)
	writeProjectFile(t, dir, filepath.Join("src", "main.ts"), `declare const a: vector;
declare const b: vector;
print(a.sub(b), a.add(b));
`)

	text, diags, err := CompileFile(dir, "src/main.ts")
	if err != nil {
		t.Fatalf("CompileFile: %v (diags: %v)", err, diags)
	}
	if len(diags) != 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}
	const want = `print(a - b, a + b)
return nil
`
	if got := stripCompileHeader(t, text); got != want {
		t.Errorf("compiled output differs:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
