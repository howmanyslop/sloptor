import { defineConfig, devices } from "@playwright/test";

const host = "127.0.0.1";
const port = 4321;

export default defineConfig({
	fullyParallel: false,
	projects: [
		{
			name: "chromium",
			use: { ...devices["Desktop Chrome"] },
		},
	],
	reporter: "list",
	testDir: "./tests/browser",
	testMatch: "**/*.test.ts",
	use: {
		baseURL: `http://${host}:${port}/sloptor`,
		trace: "retain-on-failure",
	},
	webServer: {
		command: `node --run build && node --run preview -- --host ${host} --port ${port} --ignore-lock`,
		reuseExistingServer: false,
		timeout: 120_000,
		url: `http://${host}:${port}/sloptor/`,
	},
});
