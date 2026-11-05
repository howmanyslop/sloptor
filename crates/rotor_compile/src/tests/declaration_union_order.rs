//! Not a Go test file: a regression fixture for the order of a union of two instantiations of
//! one anonymous mapped type in emitted declarations. rotor's typescript-go fork orders them by
//! their raw mappers, whose fresh type parameters follow creation order; TS 7.1 compares the
//! effective instantiations instead, which here reverses the members.

use super::output::{build, patch_out_dir};
use super::write_project;
use crate::ProjectOptions;

#[test]
fn mapped_type_union_members_keep_go_order() {
	let temp = write_project("@scope/mapped-type-union-order", "");
	patch_out_dir(&temp, r#", "declaration": true"#);
	temp.write(
		"src/schema.ts",
		"export interface Schema<T> {\n\tor<U>(other: Schema<U>): Schema<T | U>;\n}\nexport declare function object<T>(shape: T): Schema<{ [K in keyof T]?: T[K] }>;\n",
	);
	// `second`'s instantiation is created first, though `first` is declared first.
	temp.write(
		"src/main.ts",
		"import { object } from \"./schema\";\nconst first = { a: 1 };\nconst second = { b: \"\" };\nexport const either = object(second).or(object(first));\n",
	);
	build(&temp.dir(), &ProjectOptions::default());
	let declaration = std::fs::read_to_string(temp.join("out/main.d.ts")).expect("out/main.d.ts");
	// Captured from the Go build.
	assert_eq!(
		declaration,
		"export declare const either: import(\"./schema\").Schema<{\n    b?: string | undefined;\n} | {\n    a?: number | undefined;\n}>;\n"
	);
}
