//! `unreachable_function_test.go`

use super::{large_stack, write_project};
use crate::compile::compile_file;

const UNREACHABLE: &str = "Function \"read\" is unreachable after a final statement.\nDeclare the function before the first statement that uses it.";

/// Compiles `source` as `src/main.ts` of a fresh project.
fn compile(source: &str) -> (String, Vec<String>) {
	let temp = write_project("@scope/unreachable-function", "");
	temp.write("src/main.ts", source);
	match large_stack(|| compile_file(&temp.dir(), "src/main.ts")) {
		Ok((text, diagnostics)) => (String::from_utf8_lossy(&text).into_owned(), diagnostics),
		Err(failure) => panic!("CompileFile: {failure} (diagnostics: {:?})", failure.messages()),
	}
}

#[test]
fn compile_file_rejects_referenced_function_after_return() {
	let (text, diagnostics) = compile(
		"export function ClosureAfterReturn(value: number): () => number {
	const later = () => read();
	return later;
	function read(): number { return value; }
}
",
	);
	assert_eq!(diagnostics, [UNREACHABLE], "output: {text:?}");
	assert_eq!(text, "", "want no output after diagnostic");
}

#[test]
fn compile_file_elides_unreferenced_function_after_return() {
	let (text, diagnostics) = compile(
		"export function NoReference(): number {
	return 1;
	function unused(): number { return 2; }
}
",
	);
	assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
	let want = "-- Compiled with sloptor v3.0.0-beta.2
local function NoReference()
	return 1
end
return {
	NoReference = NoReference,
}
";
	assert_eq!(text, want);
}

#[test]
fn compile_file_preserves_reachable_hoisted_function() {
	let (text, diagnostics) = compile(
		"export function Reachable(value: number): () => number {
	const later = () => read();
	function read(): number { return value; }
	return later;
}
",
	);
	assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
	let want = "-- Compiled with sloptor v3.0.0-beta.2
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
";
	assert_eq!(text, want);
}

#[test]
fn compile_file_rejects_referenced_function_after_loop_exit() {
	for final_statement in ["break", "continue"] {
		let (text, diagnostics) = compile(&format!(
			"export function Loop(value: number): void {{
	while (value > 0) {{
		const later = () => read();
		{final_statement};
		function read(): number {{ return value; }}
	}}
}}
"
		));
		assert_eq!(diagnostics, [UNREACHABLE], "{final_statement}: output: {text:?}");
		assert_eq!(text, "", "{final_statement}: want no output after diagnostic");
	}
}
