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
		clearMocks: true,
		environment: "jsdom",
		include: ["tests/unit/**/*.test.{ts,tsx}"],
		name: "documentation",
		restoreMocks: true,
		setupFiles: ["./tests/unit/setup.ts"],
	},
});
