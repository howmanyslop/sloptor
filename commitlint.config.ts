import type { UserConfig } from "@commitlint/types";

const configuration = {
	extends: ["@commitlint/config-conventional"],
	rules: {
		"body-max-line-length": [0, "always", Number.POSITIVE_INFINITY],
		"footer-max-line-length": [0, "always", Number.POSITIVE_INFINITY],
	},
} as const satisfies UserConfig;

export default configuration;
