//! `vector_math_test.go`: the `vector` math methods compile to operators.

use std::path::Path;

use super::{Temp, copy_dir, fixture_project_dir, large_stack};
use crate::compile::compile_file;

const VECTOR_MATH_TYPES: &str = "
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
";

/// `buildAuditProject` (`audit_test.go`): a package project over the diff fixture's
/// `compiler-types` and `types`.
pub fn build_audit_project() -> Temp {
	let temp = Temp::new();
	let fixture_rbxts = Path::new(&fixture_project_dir()).join("node_modules/@rbxts");
	for pkg in ["compiler-types", "types"] {
		copy_dir(
			&fixture_rbxts.join(pkg),
			&temp.join(&format!("node_modules/@rbxts/{pkg}")),
		);
	}
	temp.write("package.json", r#"{"name":"@audit/fixture"}"#);
	temp.write(
		"tsconfig.json",
		"{
	\"compilerOptions\": {
		\"allowSyntheticDefaultImports\": true,
		\"module\": \"commonjs\",
		\"moduleResolution\": \"Node\",
		\"noLib\": true,
		\"moduleDetection\": \"force\",
		\"strict\": true,
		\"target\": \"ESNext\",
		\"typeRoots\": [\"node_modules/@rbxts\"],
		\"rootDir\": \"src\",
		\"outDir\": \"out\"
	},
	\"include\": [\"src\"]
}",
	);
	temp.write("src/main.ts", "export {};\n");
	temp
}

/// `stripCompileHeader`
fn strip_compile_header(text: &str) -> &str {
	let Some((name, rest)) = text.split_once('\n') else {
		panic!("compiled output has no header line:\n{text}");
	};
	assert!(
		name.starts_with("-- Compiled with sloptor v"),
		"compiled output header = {name:?}, want version banner"
	);
	rest
}

fn compile(temp: &Temp) -> String {
	match large_stack(|| compile_file(&temp.dir(), "src/main.ts")) {
		Ok((text, diagnostics)) => {
			assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
			String::from_utf8_lossy(&text).into_owned()
		}
		Err(failure) => panic!("CompileFile: {failure} (diags: {:?})", failure.messages()),
	}
}

#[test]
fn vector_math_methods_compile_to_operators() {
	let temp = build_audit_project();
	let macro_math_path = temp.join("node_modules/@rbxts/types/include/macro_math.d.ts");
	let mut data = std::fs::read_to_string(&macro_math_path).expect("macro_math.d.ts reads");
	data.push_str(VECTOR_MATH_TYPES);
	std::fs::write(&macro_math_path, data).expect("macro_math.d.ts writes");
	temp.write(
		"src/main.ts",
		"function subtract(minuend: vector, subtrahend: vector): vector {
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
",
	);

	let want = "local function subtract(minuend, subtrahend)
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
";
	assert_eq!(strip_compile_header(&compile(&temp)), want, "compiled output differs");
}

#[test]
fn vector_math_methods_missing_from_types_are_tolerated() {
	let temp = build_audit_project();
	temp.write(
		"src/main.ts",
		"const v = vector.create(1, 2, 3);
print(v.x, v.y, v.z);
const a = new Vector3(1, 2, 3);
const b = new Vector3(4, 5, 6);
print(a.add(b), a.sub(b));
",
	);
	let want = "local v = vector.create(1, 2, 3)
print(v.x, v.y, v.z)
local a = Vector3.new(1, 2, 3)
local b = Vector3.new(4, 5, 6)
print(a + b, a - b)
return nil
";
	assert_eq!(strip_compile_header(&compile(&temp)), want, "compiled output differs");
}

#[test]
fn vector_math_methods_from_project_declaration() {
	let temp = build_audit_project();
	temp.write("src/vector.d.ts", VECTOR_MATH_TYPES);
	temp.write(
		"src/main.ts",
		"declare const a: vector;\ndeclare const b: vector;\nprint(a.sub(b), a.add(b));\n",
	);
	let want = "print(a - b, a + b)\nreturn nil\n";
	assert_eq!(strip_compile_header(&compile(&temp)), want, "compiled output differs");
}
