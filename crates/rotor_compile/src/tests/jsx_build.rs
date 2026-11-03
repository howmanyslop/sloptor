//! `jsx_build_test.go`

use super::{Temp, large_stack, write_project};
use crate::compile::compile_file;

/// `writeJsxComponentProject`
fn write_jsx_component_project(name: &str) -> Temp {
	let temp = write_project("@scope/jsx-component-binding", "");
	let config = std::fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	temp.write(
		"tsconfig.json",
		config.replacen(
			"\"compilerOptions\": {",
			"\"compilerOptions\": {\n\t\t\"jsx\": \"react\",\n\t\t\"jsxFactory\": \"React.createElement\",",
			1,
		),
	);
	temp.write(
		"src/jsx.d.ts",
		"declare namespace JSX { interface Element {} }\ndeclare const React: { createElement(component: unknown): JSX.Element };\n",
	);
	temp.write(
		"src/view.tsx",
		format!(
			"export function Example({{ component: {name} }}: {{ readonly component: () => JSX.Element }}): JSX.Element {{ return <{name} />; }}\n"
		),
	);
	temp
}

fn compile(temp: &Temp) -> (String, Vec<String>) {
	match large_stack(|| compile_file(&temp.dir(), "src/view.tsx")) {
		Ok((text, diagnostics)) => (String::from_utf8_lossy(&text).into_owned(), diagnostics),
		Err(failure) => panic!("CompileFile: {failure} (diagnostics: {:?})", failure.messages()),
	}
}

#[test]
fn compile_file_jsx_uses_component_binding() {
	for name in ["_View", "View"] {
		let (got, diagnostics) = compile(&write_jsx_component_project(name));
		assert!(diagnostics.is_empty(), "{name}: diagnostics: {diagnostics:?}");
		assert!(
			got.contains(&format!("React.createElement({name})")),
			"{name}: JSX did not use the component binding:\n{got}"
		);
	}
}

#[test]
fn compile_file_jsx_rejects_dollar_component_binding() {
	let (_, diagnostics) = compile(&write_jsx_component_project("$View"));
	assert!(
		diagnostics.len() == 1 && diagnostics[0].contains("Invalid Luau identifier!"),
		"diagnostics: {diagnostics:?}, want invalid identifier diagnostic"
	);
}
