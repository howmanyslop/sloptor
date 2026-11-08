import { defineConfig } from "vitest/config";

// Only the modules that do not import `vscode` run here; the rest is wiring.
const configuration = defineConfig({
	test: {
		name: "vscode",
		include: ["src/**/*.test.ts"],
	},
});

export default configuration;
