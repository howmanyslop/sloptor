import { availableParallelism } from "node:os";
import { argv } from "node:process";
import { defineConfig } from "vitest/config";

const cpuCount = availableParallelism();
const workerCount = Math.max(2, Math.min(cpuCount - 1, 12));

const NARROWING_FLAGS = new Set(["--changed", "--project", "--related", "--shard", "--testNamePattern", "-t"]);
const TEST_FILE_ARGUMENT = /\.(?:fuzz|test)\.[cm]?tsx?$/u;
const TEST_ROOTS = ["editors/"];

function isFocusedRun(cliArguments: ReadonlyArray<string>): boolean {
	return cliArguments.some((argument) => {
		if (argument.startsWith("-")) return NARROWING_FLAGS.has(argument.split("=", 1)[0] ?? argument);

		return TEST_ROOTS.some((root) => argument.startsWith(root)) || TEST_FILE_ARGUMENT.test(argument);
	});
}

const enabled = !isFocusedRun(argv.slice(2));

const configuration = defineConfig({
	resolve: { tsconfigPaths: true },
	test: {
		name: "vscode",
		coverage: {
			clean: true,
			enabled,
			exclude: [
				"src/color/color-provider.ts",
				"src/compiler-terminal.ts",
				"src/compiler.ts",
				"src/extension.ts",
				"src/language-client.ts",
				"src/output.ts",
			],
			include: ["src/**/*.ts"],
			provider: "v8",
			reporter: ["text", "html", "text-summary", "json", "json-summary"],
			reportOnFailure: true,
			reportsDirectory: "./coverage",
			thresholds: { 100: true },
		},
		environment: "node",
		fileParallelism: true,
		fsModuleCache: true,
		globals: true,
		include: ["src/**/*.test.ts"],
		isolate: false,
		maxWorkers: workerCount,
		passWithNoTests: true,
		pool: "forks",
		testTimeout: 30_000,
	},
});

export default configuration;
