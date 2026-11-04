// Generates crates/rotor_lsp/src/params.rs from the tsgo lsproto package.
// Source: tsgo/lsp/lsproto/lsp_generated.go.
// Run with `node tools/lsp-params/main.ts`, then `mise run rust:fmt`.
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const METHOD_CONSTANT = /^[ \t]*(?<constant>Method\w+)\s+Method = "(?<value>[^"]+)"/gmu;
const CASE_PARAMS = /case (?<constant>Method\w+):\n\t\treturn (?:unmarshalEmpty|unmarshalPtrTo\[(?<type>\w+)\])/gu;
const CASE_START = /\n\t\tcase `"/u;
const NEXT_CASE = /\n\t\t(?:case|default)/u;
const REQUIRED = /missing &\^= missing/u;
const NULL_CHECK = /errNull\(/u;

interface Field {
	name: string;
	nullable: boolean;
	required: boolean;
}

interface Case {
	kind: string;
	method: string;
}

function compareStrings(left: string, right: string): number {
	if (left < right) return -1;
	return left > right ? 1 : 0;
}

function quote(names: Array<string>): string {
	return names.map((name) => `"${name}"`).join(", ");
}

const root = path.resolve(import.meta.dirname, "../..");
const source = readFileSync(`${root}/tsgo/lsp/lsproto/lsp_generated.go`, "utf8");

const methods = new Map<string, string>();
for (const match of source.matchAll(METHOD_CONSTANT)) {
	const { constant, value } = match.groups ?? {};
	if (constant === undefined || value === undefined) throw new Error("unreachable: unnamed match");
	methods.set(constant, value);
}

const switchStart = source.indexOf("func unmarshalParams(method Method, data []byte) (any, error) {");
const switchBody = source.slice(switchStart, source.indexOf("\n}\n", switchStart));
const cases: Array<Case> = [];
for (const match of switchBody.matchAll(CASE_PARAMS)) {
	const { constant, type } = match.groups ?? {};
	if (constant === undefined) throw new Error("unreachable: unnamed match");
	const method = methods.get(constant);
	if (method === undefined) throw new Error(`no value for ${constant}`);
	cases.push({ kind: type ?? "empty", method });
}

function readFields(type: string): Array<Field> | undefined {
	const start = source.indexOf(`func (s *${type}) UnmarshalJSONFrom(dec *json.Decoder) error {`);
	if (start === -1) return undefined;
	const body = source.slice(start, source.indexOf("\n}\n", start));
	const result: Array<Field> = [];
	const parts = body.split(CASE_START).slice(1);
	for (const part of parts) {
		const name = part.slice(0, part.indexOf('"'));
		const end = part.search(NEXT_CASE);
		const text = end === -1 ? part : part.slice(0, end);
		result.push({
			name,
			nullable: !NULL_CHECK.test(text),
			required: REQUIRED.test(text),
		});
	}
	return result;
}

const lines: Array<string> = [
	"//! Which request params `tsgo/lsp/lsproto` fails to decode (`unmarshalParams`), so a request",
	"//! for a known method with such params is answered with `CodeInvalidParams`, as Go's server does.",
	"//!",
	"//! Generated from `tsgo/lsp/lsproto/lsp_generated.go`. Only the params' top level is checked:",
	"//! an object, its required properties present and no property null that Go rejects as null.",
	"//! A params object malformed deeper than that is answered as an unknown method.",
	"",
	"use serde_json::Value;",
	"",
	"/// How a method's params decode.",
	"enum Shape {",
	"\t/// `unmarshalEmpty`: any params fail.",
	"\tEmpty,",
	"\t/// `unmarshalPtrTo` of a struct with no decoder of its own: an object, or null.",
	"\tPlain,",
	"\t/// `unmarshalPtrTo`: an object with these required properties, and these that may not be null.",
	"\tObject {",
	"\t\trequired: &'static [&'static str],",
	"\t\tnot_null: &'static [&'static str],",
	"\t},",
	"}",
	"",
	"const METHODS: &[(&str, Shape)] = &[",
];
const sortedCases = cases.toSorted((left, right) => compareStrings(left.method, right.method));
for (const { kind, method } of sortedCases) {
	if (kind === "empty") {
		lines.push(`\t("${method}", Shape::Empty),`);
		continue;
	}
	// A union alias decodes through its own type; it is not checked here.
	if (new RegExp(`^type ${kind} = `, "mu").test(source)) continue;
	const list = readFields(kind);
	if (list === undefined) {
		lines.push(`\t// ${kind}: no decoder of its own`, `\t("${method}", Shape::Plain),`);
		continue;
	}
	const requiredNames: Array<string> = [];
	const notNullNames: Array<string> = [];
	for (const field of list) {
		if (field.required) requiredNames.push(field.name);
		else if (!field.nullable) notNullNames.push(field.name);
	}
	lines.push(
		`\t// ${kind}`,
		`\t("${method}", Shape::Object {`,
		`\t\trequired: &[${quote(requiredNames)}],`,
		`\t\tnot_null: &[${quote(notNullNames)}],`,
		"\t}),",
	);
}
lines.push(
	"];",
	"",
	"/// Whether Go's `unmarshalParams` rejects `params` (present in the message, `null` included)",
	"/// for `method`. An unknown method takes any JSON.",
	"#[must_use]",
	"pub fn rejects(method: &str, params: &Value) -> bool {",
	"\tlet Ok(index) = METHODS.binary_search_by(|(name, _)| (*name).cmp(method)) else {",
	"\t\treturn false;",
	"\t};",
	"\tmatch &METHODS[index].1 {",
	"\t\tShape::Empty => true,",
	"\t\tShape::Plain => !(params.is_object() || params.is_null()),",
	"\t\tShape::Object { required, not_null } => {",
	"\t\t\tlet Some(object) = params.as_object() else {",
	"\t\t\t\treturn true;",
	"\t\t\t};",
	"\t\t\trequired.iter().any(|name| !object.contains_key(*name))",
	"\t\t\t\t|| not_null.iter().any(|name| object.get(*name).is_some_and(Value::is_null))",
	"\t\t}",
	"\t}",
	"}",
	"",
);
writeFileSync(`${root}/crates/rotor_lsp/src/params.rs`, lines.join("\n"));
console.log(`${cases.length} methods`);
