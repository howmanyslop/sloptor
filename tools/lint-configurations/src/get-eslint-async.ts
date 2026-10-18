import nodePath from "node:path";
import { GLOB_PACKAGE_JSON, isentinel } from "@isentinel/eslint-config";
import { configs as astroConfigs } from "eslint-plugin-astro";

import { baseIgnores, projectType } from "./constants.ts";
import { getOxfmtConfigurationAsync } from "./get-oxfmt-configuration-async.ts";

const BANNED_DEPENDENCIES = {
	allowed: ["@typescript/native-preview"] satisfies ReadonlyArray<string>,
	modules: [] satisfies ReadonlyArray<string>,
	presets: ["microutilities", "native", "preferred"] satisfies ReadonlyArray<string>,
};

type LintConfiguration = Awaited<ReturnType<typeof isentinel>>;

export async function getEslintAsync(tsconfigPath: string): Promise<LintConfiguration> {
	const oxfmtOptions = await getOxfmtConfigurationAsync();

	return isentinel(
		{
			e18e: {
				modernization: true,
				moduleReplacements: true,
				nodeMajor: 24,
				overrides: {
					"e18e/ban-dependencies": ["error", BANNED_DEPENDENCIES],
				},
				performanceImprovements: true,
			},
			formatters: {
				css: true,
				graphql: true,
				html: true,
				json: true,
				lua: false,
				markdown: false,
				oxfmtOptions,
				yaml: true,
			},
			gitignore: true,
			ignores: [...baseIgnores, ".rumdl_cache/**"] satisfies ReadonlyArray<string>,
			jsdoc: false,
			jsonc: true,
			markdown: false,
			oxlint: true,
			oxlintWarnDeadRules: true,
			pnpm: true,
			roblox: false,
			rules: {
				"sonar/no-redundant-optional": "off",
				"ts/prefer-destructuring": "off",
				"unicorn/no-non-function-verb-prefix": "off",
			},
			spellCheck: false,
			test: {
				vitest: {
					typecheck: true,
				},
			},
			toml: {
				overrides: {
					"toml/array-bracket-spacing": "off",
					"toml/array-element-newline": "off",
					"toml/indent": ["error", "tab"],
					"toml/padding-line-between-pairs": "off",
					"toml/padding-line-between-tables": "off",
				},
			},
			type: projectType,
			typescript: {
				outOfProjectFiles: ["*.js"],
				parserOptions: { tsconfigRootDir: nodePath.dirname(tsconfigPath) },
				tsconfigPath,
			},
			yaml: {
				overrides: {
					"yaml/indent": "error",
					"yaml/quotes": ["error", { prefer: "double" }],
				},
			},
		},
		...astroConfigs.recommended,
		{
			name: "rotor/package-json",
			files: ["**/package.json", "!package.json"],
			rules: {
				"package-json/require-attribution": "off",
				"package-json/require-bugs": "off",
				"package-json/require-description": "off",
				"package-json/require-engines": "off",
				"package-json/require-exports": "off",
				"package-json/require-files": "off",
				"package-json/require-homepage": "off",
				"package-json/require-keywords": "off",
				"package-json/require-license": "off",
				"package-json/require-repository": "off",
				"package-json/require-sideEffects": "off",
				"package-json/require-types": "off",
				"package-json/require-version": "off",
			},
		},
		{
			name: "rotor/dependencies",
			files: [GLOB_PACKAGE_JSON],
			rules: {
				"e18e/ban-dependencies": ["error", BANNED_DEPENDENCIES],
			},
		},
	);
}
