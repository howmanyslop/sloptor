import { RuleConfigSeverity } from "@commitlint/types";

import type { UserConfig } from "@commitlint/types";

const configuration: UserConfig = {
	extends: ["@commitlint/config-conventional"],
	rules: {
		"header-max-length": [RuleConfigSeverity.Error, "always", 72],
		"scope-case": [RuleConfigSeverity.Error, "always", "lower-case"],
		"subject-empty": [RuleConfigSeverity.Error, "never"],
		"subject-full-stop": [RuleConfigSeverity.Error, "never", "."],
		"type-case": [RuleConfigSeverity.Error, "always", "lower-case"],
		"type-enum": [
			RuleConfigSeverity.Error,
			"always",
			["feat", "fix", "refactor", "docs", "style", "test", "chore", "perf", "ci", "build"],
		],
	},
};

export default configuration;
