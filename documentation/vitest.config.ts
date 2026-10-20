import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
	base: "/sloptor/",
	resolve: {
		alias: {
			$components: fileURLToPath(new URL("src/components", import.meta.url)),
			$utilities: fileURLToPath(new URL("src/utilities", import.meta.url)),
		},
	},
	test: {
		name: "documentation",
		clearMocks: true,
		environment: "jsdom",
		include: ["tests/unit/**/*.test.{ts,tsx}"],
		restoreMocks: true,
		setupFiles: ["./tests/unit/setup.ts"],
	},
});
