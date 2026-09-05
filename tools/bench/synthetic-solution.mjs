import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const PROJECTS = 14;
const FILES_PER_PROJECT = 300;
const TYPE_PACKAGES = 24;

function fail(message) {
	throw new Error(message);
}

function parseArgs(argv) {
	const options = new Map();
	for (let index = 0; index < argv.length; index += 2) {
		const flag = argv[index];
		const value = argv[index + 1];
		if (!flag?.startsWith("--") || value === undefined)
			fail(
				"usage: node tools/bench/synthetic-solution.mjs --root <fixture-directory> --typescript <typescript-directory>",
			);
		options.set(flag.slice(2), value);
	}
	return options;
}

function write(root, relative, value) {
	const destination = path.join(root, relative);
	fs.mkdirSync(path.dirname(destination), { recursive: true });
	fs.writeFileSync(destination, typeof value === "string" ? value : `${JSON.stringify(value, null, 2)}\n`);
}

const options = parseArgs(process.argv.slice(2));
const requestedRoot = options.get("root");
const requestedTypeScript = options.get("typescript");
if (!requestedRoot || !requestedTypeScript) fail("both --root and --typescript are required");

const root = path.resolve(requestedRoot);
const typescript = path.resolve(requestedTypeScript);
if (root === path.parse(root).root) fail("--root must not be a filesystem root");
if (!fs.statSync(typescript).isDirectory()) fail(`TypeScript directory is unavailable: ${typescript}`);
fs.mkdirSync(path.dirname(root), { recursive: true });
fs.mkdirSync(root);

write(root, "package.json", { name: "synthetic-solution", private: true });
write(root, "transformer.js", "module.exports = () => context => source => source;\n");
write(root, "shared.project.json", {
	name: "synthetic-shared-tree",
	tree: {
		$className: "DataModel",
		ReplicatedStorage: { $path: "projects" },
	},
});
write(root, "node_modules/@rbxts/globals/package.json", { name: "@rbxts/globals", types: "index.d.ts" });
write(
	root,
	"node_modules/@rbxts/globals/index.d.ts",
	[
		"interface Array<T> { readonly length: number; }",
		"interface Boolean {}",
		"interface CallableFunction extends Function {}",
		"interface Function {}",
		"interface IArguments { readonly length: number; readonly callee: Function; }",
		"interface NewableFunction extends Function {}",
		"interface Number {}",
		"interface Object {}",
		"interface RegExp {}",
		"interface String {}",
		"declare const print: (...values: unknown[]) => void;",
		"",
	].join("\n"),
);
fs.symlinkSync(typescript, path.join(root, "node_modules", "typescript"), "dir");

for (let index = 0; index < TYPE_PACKAGES; index++) {
	const name = `@synthetic/types-${index}`;
	write(root, `node_modules/@synthetic/types-${index}/package.json`, {
		name,
		version: "1.0.0",
		types: "./index.d.ts",
		exports: { ".": { types: "./index.d.ts", default: "./index.js" } },
		typesVersions: { "*": { "*": ["index.d.ts"] } },
	});
	write(
		root,
		`node_modules/@synthetic/types-${index}/index.d.ts`,
		`export interface Shape${index} { readonly value: number; readonly label?: string; }\n`,
	);
	write(root, `node_modules/@synthetic/types-${index}/index.js`, "module.exports = {};\n");
}

const references = [];
for (let projectIndex = 0; projectIndex < PROJECTS; projectIndex++) {
	const project = `projects/project-${projectIndex}`;
	references.push({ path: project });
	write(root, `${project}/package.json`, { name: `synthetic-project-${projectIndex}`, private: true });
	write(root, `${project}/include/RuntimeLib.lua`, "return {}\n");
	write(root, `${project}/tsconfig.json`, {
		compilerOptions: {
			allowSyntheticDefaultImports: true,
			composite: true,
			declaration: true,
			declarationMap: false,
			incremental: true,
			module: "Preserve",
			moduleResolution: "Bundler",
			moduleDetection: "force",
			noLib: true,
			outDir: "out",
			rootDir: "src",
			strict: true,
			target: "ESNext",
			typeRoots: ["../../node_modules/@rbxts"],
			types: ["globals"],
			plugins: [{ transform: "../../transformer.js" }],
		},
		rbxts: { type: "game", rojo: "../../shared.project.json" },
		include: ["src"],
	});
	for (let fileIndex = 0; fileIndex < FILES_PER_PROJECT; fileIndex++) {
		const typeIndex = fileIndex % TYPE_PACKAGES;
		write(
			root,
			`${project}/src/value-${fileIndex}.ts`,
			[
				`import type { Shape${typeIndex} } from '@synthetic/types-${typeIndex}';`,
				`export const value${fileIndex}: Shape${typeIndex} = { value: ${fileIndex} };`,
				"",
			].join("\n"),
		);
	}
}
write(root, "tsconfig.json", { files: [], references });

const recipe = {
	schemaVersion: 1,
	fixture: "synthetic-solution-cache-comparison",
	projects: PROJECTS,
	filesPerProject: FILES_PER_PROJECT,
	sourceFiles: PROJECTS * FILES_PER_PROJECT,
	typePackages: TYPE_PACKAGES,
	sharedRojoConfig: "shared.project.json",
	packageMetadataScenario: "Each project resolves the same 24 packages with exports and typesVersions metadata.",
	oneFileEdit: "projects/project-0/src/value-0.ts",
};
write(root, "synthetic-fixture.json", recipe);
write(root, "synthetic-fixture.sha256", `${createHash("sha256").update(JSON.stringify(recipe)).digest("hex")}\n`);
process.stdout.write(`${JSON.stringify(recipe)}\n`);
