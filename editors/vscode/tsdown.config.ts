import { defineConfig } from "tsdown";

// One minified CommonJS file: VS Code loads it without a node_modules walk, and
// the .vsix ships no dependencies. VS Code 1.91 runs Node 20.
const configuration = defineConfig({
	clean: true,
	deps: { neverBundle: ["vscode"] },
	dts: false,
	entry: ["src/extension.ts"],
	format: "cjs",
	minify: true,
	outDir: "dist",
	platform: "node",
	sourcemap: false,
	target: "node20",
});

export default configuration;
