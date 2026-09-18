import fs from "node:fs";
import path from "node:path";

function fail(message) {
	throw new Error(message);
}

function options(argv) {
	const result = new Map();
	for (let index = 0; index < argv.length; index += 2) {
		const flag = argv[index];
		const value = argv[index + 1];
		if (!flag?.startsWith("--") || value === undefined)
			fail(
				"usage: --root <fixture-directory> --typescript <sidecar-typescript-directory> [--files <positive-integer>]",
			);
		result.set(flag.slice(2), value);
	}
	return result;
}

function write(root, relative, contents) {
	const file = path.join(root, relative);
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, typeof contents === "string" ? contents : `${JSON.stringify(contents, null, 2)}\n`);
}

const args = options(process.argv.slice(2));
const requestedRoot = args.get("root");
const requestedTypeScript = args.get("typescript");
const sourceFiles = Number(args.get("files") ?? 180);
if (!Number.isInteger(sourceFiles) || sourceFiles < 1) fail("--files must be a positive integer");
if (!requestedRoot || !requestedTypeScript) fail("--root and --typescript are required");
const root = path.resolve(requestedRoot);
const typescript = path.resolve(requestedTypeScript);
if (root === path.parse(root).root) fail("--root must not be a filesystem root");
const typeScriptPackage = JSON.parse(fs.readFileSync(path.join(typescript, "package.json"), "utf8"));
if (typeScriptPackage.version !== "6.0.3") fail(`expected sidecar TypeScript 6.0.3, got ${typeScriptPackage.version}`);
fs.mkdirSync(path.dirname(root), { recursive: true });
fs.mkdirSync(root);

write(root, "package.json", { name: "synthetic-worker-transform", private: true });
write(root, "node_modules/@rbxts/globals/package.json", { name: "@rbxts/globals", types: "index.d.ts" });
write(
	root,
	"node_modules/@rbxts/globals/index.d.ts",
	[
		"interface Array<T> { readonly length: number; [index: number]: T; }",
		"interface Boolean {}",
		"interface CallableFunction extends Function {}",
		"interface Function {}",
		"interface IArguments { readonly length: number; readonly callee: Function; }",
		"interface NewableFunction extends Function {}",
		"interface Number {}",
		"interface Object {}",
		"interface RegExp {}",
		"interface String {}",
		"",
	].join("\n"),
);
write(root, "node_modules/@synthetic/contracts/package.json", {
	name: "@synthetic/contracts",
	version: "1.0.0",
	types: "./index.d.ts",
	exports: { ".": { types: "./index.d.ts", default: "./index.js" } },
});
write(
	root,
	"node_modules/@synthetic/contracts/index.d.ts",
	"export interface WorkItem { readonly id: number; readonly scale: number; }\n",
);
write(root, "node_modules/@synthetic/contracts/index.js", "module.exports = {};\n");
fs.mkdirSync(path.join(root, "node_modules"), { recursive: true });
fs.symlinkSync(typescript, path.join(root, "node_modules", "typescript"), "dir");
fs.mkdirSync(path.join(root, "out"));
write(root, "include/RuntimeLib.lua", "return {}\n");
write(root, "default.project.json", {
	name: "synthetic-worker-fixture",
	tree: {
		$path: "out",
		include: { $path: "include" },
		node_modules: {
			$className: "Folder",
			"@rbxts": { $path: "node_modules/@rbxts" },
		},
	},
});

write(
	root,
	"checker-transformer.js",
	`const ts = require('typescript');
let factoryGeneration = 0;
module.exports = program => {
  const generation = process.env.SYNTHETIC_FACTORY_CACHE_PROBE === '1' ? ++factoryGeneration : 1;
  const checker = program.getTypeChecker();
  return context => source => {
    const visit = node => {
      if (ts.isVariableDeclaration(node) && node.initializer && ts.isNumericLiteral(node.initializer)) {
        const type = checker.getTypeAtLocation(node.name);
        if (checker.typeToString(type).length > 0) {
          return ts.factory.updateVariableDeclaration(
            node,
            node.name,
            node.exclamationToken,
            node.type,
            ts.factory.createBinaryExpression(node.initializer, ts.SyntaxKind.PlusToken, ts.factory.createNumericLiteral(generation)),
          );
        }
      }
      return ts.visitEachChild(node, visit, context);
    };
    return ts.visitNode(source, visit);
  };
};
`,
);
write(root, "tsconfig.json", {
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
		sourceMap: false,
		strict: true,
		target: "ESNext",
		typeRoots: ["node_modules/@rbxts"],
		types: ["globals"],
		plugins: [{ transform: "./checker-transformer.js" }],
	},
	rbxts: { type: "game", rojo: "default.project.json" },
	include: ["src"],
});
for (let index = 0; index < sourceFiles; index++) {
	write(
		root,
		`src/unit-${index}.ts`,
		[
			"import type { WorkItem } from '@synthetic/contracts';",
			`const seed${index}: number = ${index};`,
			`const item${index}: WorkItem = { id: seed${index}, scale: ${(index % 7) + 1} };`,
			`export const value${index}: number = item${index}.id * item${index}.scale;`,
			"",
		].join("\n"),
	);
}
write(root, "synthetic-worker-fixture.json", {
	schemaVersion: 1,
	fixture: "synthetic-worker-persistence",
	sourceFiles,
	transform: "checker query plus generation-marked numeric-literal AST rewrite",
	typeScript: typeScriptPackage.version,
	packageMetadata: "exports-backed synthetic declaration package",
});
process.stdout.write(`${JSON.stringify({ root, sourceFiles, typeScript: typeScriptPackage.version })}\n`);
