import type { OxfmtOptions } from "@isentinel/eslint-config";
import type { UnknownRecord } from "type-fest";

const DEFAULT_CONFIGURATION = {
	arrowParens: "always",
	bracketSameLine: false,
	bracketSpacing: true,
	embeddedLanguageFormatting: "auto",
	endOfLine: "lf",
	htmlWhitespaceSensitivity: "css",
	ignorePatterns: [
		"**/*-lock.json",
		"**/*.js",
		"**/*.{md,toml,js,yaml,yml,toml}",
		"**/.opencode/goals",
		"**/do-not-sync-ever/**",
		"**/generated/**",
		"**/ses_*.json",
		".mise/**",
		".tsbuildinfo*",
		"tests/fixtures/**/{invalid-*,*-invalid}/**/*.json",
		"**/{reference,testdata}/**/src/tests/**",
		"**/testdata/**",
		"{reference,third_party}/**",
		"rbxts-tsconfig.schema.json",
	],
	insertFinalNewline: true,
	jsdoc: false,
	jsxSingleQuote: false,
	objectWrap: "preserve",
	overrides: [
		{
			files: ["**/*.jsonc"],
			options: { trailingComma: "all" },
		},
		{
			files: ["biome.jsonc", ".oxlintrc.json"],
			options: { trailingComma: "none" },
		},
		{
			files: [".oxfmtrc.json"],
			options: { trailingComma: "all" },
		},
	],
	printWidth: 120,
	proseWrap: "preserve",
	quoteProps: "as-needed",
	semi: true,
	singleAttributePerLine: false,
	singleQuote: false,
	sortImports: {
		customGroups: [
			{
				elementNamePattern: [
					"react",
					"react-dom",
					"react-**",
					"next",
					"next/**",
					"vue",
					"vue-**",
					"@vue/**",
					"svelte",
					"svelte/**",
					"@sveltejs/**",
					"solid-js",
					"@solidjs/**",
				],
				groupName: "framework",
				modifiers: ["value"],
				selector: "external",
			},
			{
				elementNamePattern: [
					"vitest",
					"@vitest/**",
					"jest",
					"@jest/**",
					"@testing-library/**",
					"msw",
					"msw/**",
					"playwright",
					"@playwright/**",
					"cypress",
					"@cypress/**",
				],
				groupName: "testing",
				modifiers: ["value"],
				selector: "external",
			},
		],
		groups: [
			"side_effect",
			"side_effect_style",
			{ newlinesBetween: true },

			"value-builtin",
			"framework",
			"testing",
			"value-external",
			{ newlinesBetween: true },

			["value-internal", "value-subpath"],
			{ newlinesBetween: true },

			["value-parent", "value-sibling", "value-index"],
			{ newlinesBetween: true },

			"style",
			{ newlinesBetween: true },

			["type-builtin", "type-external"],
			{ newlinesBetween: true },

			["type-internal", "type-subpath"],
			{ newlinesBetween: true },

			["type-parent", "type-sibling", "type-index"],
			"type-import",
			{ newlinesBetween: true },

			"unknown",
		],
		ignoreCase: true,
		internalPattern: ["$", "~/"],
		newlinesBetween: false,
		order: "asc",
		partitionByComment: false,
		partitionByNewline: false,
		sortSideEffects: false,
	},
	sortPackageJson: false,
	sortTailwindcss: true,
	tabWidth: 4,
	trailingComma: "all",
	useTabs: true,
} satisfies OxfmtOptions;

// biome-ignore lint/style/noRestrictedTypes: Except will not work here.
function deleteSchema<TObject extends UnknownRecord>(object: TObject): Omit<TObject, "$schema"> {
	if ("$schema" in object) {
		// oxlint-disable-next-line no-unused-vars -- we want to remove $schema from the object.
		const { $schema, ...rest } = object;
		return rest;
	}

	return object;
}

export async function getOxfmtConfigurationAsync(): Promise<OxfmtOptions> {
	try {
		const { up } = await import("empathic/find");
		const filePath = up(".oxfmtrc.json");
		if (filePath === undefined) return DEFAULT_CONFIGURATION;

		const [{ readFile }, { parseJSONC }] = await Promise.all([import("node:fs/promises"), import("confbox")]);
		const jsonc = parseJSONC(await readFile(filePath, "utf8"));

		const { isOxfmtConfiguration } = await import("./oxfmt-types.ts");
		return deleteSchema(isOxfmtConfiguration.allows(jsonc) ? jsonc : DEFAULT_CONFIGURATION);
	} catch {
		return DEFAULT_CONFIGURATION;
	}
}
