// oxlint-disable small-rules/prevent-abbreviations -- not here, lol.
import nodePath from "node:path";
import { GLOB_DTS, GLOB_SRC, GLOB_SRC_EXT, GLOB_TESTS } from "@isentinel/eslint-config";
import { isentinel } from "@isentinel/eslint-config/oxlint";
import { ALL_REACT_DOCTOR_RULES } from "oxlint-plugin-react-doctor";

import { baseIgnores, projectType } from "./constants.ts";

import type { OxlintConfig, OxlintFactoryOptions } from "@isentinel/eslint-config/oxlint";

const CONFIGURATION_FILES = `**/*.config.${GLOB_SRC_EXT}`;
const SCRIPT_FILES = `scripts/${GLOB_SRC}`;
const REACT_DOCTOR_PLUGIN = { name: "react-doctor", specifier: "oxlint-plugin-react-doctor" };

const MANUAL_BANNED = "Manual React memoization is banned. Let React Compiler derive it.";

const REACT_COMPILER_RESTRICTED_SYNTAX = [
	{
		message: MANUAL_BANNED,
		selector: "ImportDeclaration[source.value='react'] > ImportSpecifier[imported.name='memo']",
	},
	{
		message: MANUAL_BANNED,
		selector: "ImportDeclaration[source.value='react'] > ImportSpecifier[imported.name='useCallback']",
	},
	{
		message: MANUAL_BANNED,
		selector: "ImportDeclaration[source.value='react'] > ImportSpecifier[imported.name='useMemo']",
	},
	{
		message: MANUAL_BANNED,
		selector: "CallExpression[callee.object.name='React'][callee.property.name='memo']",
	},
	{
		message: MANUAL_BANNED,
		selector: "CallExpression[callee.object.name='React'][callee.property.name='useCallback']",
	},
	{
		message: MANUAL_BANNED,
		selector: "CallExpression[callee.object.name='React'][callee.property.name='useMemo']",
	},
	{
		message: "Function defaults are not stabilized by React Compiler. Hoist the function instead.",
		selector:
			":matches(FunctionDeclaration, FunctionExpression)[id.name=/^(?:[A-Z]|use[A-Z])/] > ObjectPattern.params > Property > AssignmentPattern > :matches(ArrowFunctionExpression, FunctionExpression).right",
	},
	{
		message: "Function defaults are not stabilized by React Compiler. Hoist the function instead.",
		selector:
			"VariableDeclarator[id.name=/^(?:[A-Z]|use[A-Z])/] > :matches(ArrowFunctionExpression, FunctionExpression).init > ObjectPattern.params > Property > AssignmentPattern > :matches(ArrowFunctionExpression, FunctionExpression).right",
	},
] as const;

const REACT_EFFECT_RESTRICTED_SYNTAX = [
	{
		message: "useEffect is banned. Use the `no-use-effect` skill for more information.",
		selector: "CallExpression[callee.name='useEffect']",
	},
	{
		message: "useLayoutEffect is banned. Use the `no-use-effect` skill for more information.",
		selector: "CallExpression[callee.name='useLayoutEffect']",
	},
] as const;

// Rule families for libraries no workspace uses. Every enabled JS plugin rule
// costs time on every file, whether or not the library is imported.
const UNUSED_REACT_DOCTOR_FAMILIES = [
	"expo-",
	"ink-",
	"jotai-",
	"mobx-",
	"motion-",
	"nextjs-",
	"preact-",
	"r3f-",
	"radix-",
	"react-router-",
	"redux-",
	"remotion-",
	"rn-",
	"styled-components-",
	"three-",
	"valtio-",
	"zod-",
	"zustand-",
];

const reactDoctorRules = Object.fromEntries(
	Object.entries(ALL_REACT_DOCTOR_RULES).map(([key, value]) => {
		const name = key.slice("react-doctor/".length);
		if (UNUSED_REACT_DOCTOR_FAMILIES.some((family) => name.startsWith(family)) || name.startsWith("jsx-no-new-")) {
			return [key, "off" as const];
		}

		return [key, value];
	}),
);

type NoRestrictedSyntax = NonNullable<NonNullable<OxlintFactoryOptions["rules"]>["eslint-js/no-restricted-syntax"]>;

interface ConfigurationOptions {
	readonly argv?: ReadonlyArray<string> | undefined;
	readonly banUseEffect?: boolean | undefined;
	readonly rootDirectory: string;
	readonly tsconfigPath: string;
}

export async function getOxlintAsync({
	argv,
	banUseEffect = true,
	rootDirectory,
	tsconfigPath,
}: ConfigurationOptions): Promise<OxlintConfig> {
	const oxfmtOptions = await import("@rotor-rbx/linter-utilities/configuration").then(
		async ({ getOxfmtConfigurationAsync }) => getOxfmtConfigurationAsync(),
	);

	const noRestrictedSyntax: NoRestrictedSyntax = ["error", ...REACT_COMPILER_RESTRICTED_SYNTAX];
	if (banUseEffect) {
		for (const effectSyntax of REACT_EFFECT_RESTRICTED_SYNTAX) {
			noRestrictedSyntax.push(effectSyntax);
		}
	}

	return isentinel(
		{
			name: "sloptor",
			categories: {
				correctness: "error",
				nursery: "error",
				pedantic: "error",
				perf: "error",
				restriction: "error",
				style: "error",
				suspicious: "error",
			},
			componentExts: ["astro"],
			eslintPlugin: false,
			formatters: { oxfmtOptions },
			ignores: [...baseIgnores, "**/{dist,do-not-sync-ever,node_modules}/**"] satisfies ReadonlyArray<string>,
			options: {
				denyWarnings: true,
				maxWarnings: 0,
				reportUnusedDisableDirectives: "error",
				respectEslintDisableDirectives: false,
				typeAware: true,
				typeCheck: argv === undefined ? true : !argv.includes("--lsp"),
			},
			oxc: true,
			react: {
				reactCompiler: true,
				version: "19.3.1",
			},
			roblox: false,
			rules: {
				"better-max-params/better-max-params": [
					"error",
					{
						constructor: 7,
						func: 7,
					},
				],
				"capitalized-comments": "off",
				"comment-length/limit-multi-line-comments": [
					"error",
					{
						semanticComments: ["v8 ignore"],
					},
				],
				"comment-length/limit-single-line-comments": [
					"error",
					{
						maxLength: 82,
						semanticComments: [
							"Modifications",
							"Vendored from",
							"oxlint-disable",
							"oxlint-disable-next-line",
							"biome-ignore",
							"eslint-disable-next-line",
						],
						tabSize: 4,
					},
				],
				complexity: ["error", { max: 20 }],
				curly: "off",
				"default-case": "off",
				// this is literally not true -- it just worsens performance!
				"e18e/prefer-static-collator": "off",
				"flawless/arrow-return-style": "off",
				"flawless/max-lines-per-function": "off",
				"func-style": [
					"error",
					"declaration",
					{
						allowArrowFunctions: false,
						allowTypeAnnotation: true,
						overrides: {
							namedExports: "ignore",
						},
					},
				],
				"import/exports-last": "off",
				"import/extensions": "off",
				"import/group-exports": "off",
				"import/max-dependencies": "off",
				"import/no-default-export": "off",
				"import/no-named-export": "off",
				"import/no-relative-parent-imports": "off",
				"import/no-unassigned-import": [
					"error",
					{
						allow: [
							"**/*.css",
							"**/*.scss",
							"**/*.less",
							"**/*.sass",
							"@total-typescript/ts-reset",
							"$configure-arktype",
							"$polyfill",
							"@dotenvx/dotenvx",
							"@dotenvx/dotenvx/config",
						],
					},
				],
				"import/prefer-default-export": "off",
				"init-declarations": "off",
				"jsdoc-js/convert-to-jsdoc-comments": [
					"warn",
					{
						allowedPrefixes: [
							"@ts-",
							"istanbul ",
							"c8 ",
							"v8 ",
							"eslint",
							"jshint",
							"jslint",
							"globals",
							"exported",
							"jscs",
							"oxlint-",
							"prettier-",
							"biome-ignore ",
						],
					},
				],
				"jsdoc/check-tag-names": ["error", { definedTags: ["rejects"] }],
				"jsdoc/require-param": "off",
				"jsdoc/require-param-type": "off",
				"jsdoc/require-property-type": "off",
				"jsdoc/require-returns-type": "off",
				"max-lines": "off",
				"max-lines-per-function": "off",
				"max-params": "off",
				"max-statements": "off",
				"new-cap": "off",
				"no-alert": "off",
				"no-bitwise": "off",
				"no-console": "off",
				"no-continue": "off",
				"no-duplicate-imports": "off",
				"no-magic-numbers": "off",
				"no-nested-ternary": "off",
				"no-plusplus": "off",
				"no-restricted-syntax": noRestrictedSyntax,
				"no-ternary": "off",
				"no-undefined": "off",
				"no-underscore-dangle": "off",
				"node-js/prefer-global/buffer": "off",
				"node/callback-return": "off",
				"node/no-sync": "off",
				"node/no-top-level-await": "off",
				"object-shorthand": ["error"],
				"one-var": "off",
				"oxc/no-async-await": "off",
				"oxc/no-const-enum": "off",
				"oxc/no-optional-chaining": "off",
				"oxc/no-rest-spread-properties": "off",
				"perfectionist/sort-modules": "off",
				"prefer-destructuring": "error",
				"prefer-named-capture-group": "off",
				"react-perf/jsx-no-new-function-as-prop": "off",
				"react/jsx-boolean-value": "off",
				"react/jsx-curly-brace-presence": [
					"error",
					{
						children: "always",
					},
				],
				"react/jsx-filename-extension": [
					"error",
					{
						extensions: ["jsx", "tsx"],
						ignoreFilesWithoutCode: true,
					},
				],
				"react/jsx-max-depth": ["error", { max: 3 }],
				"react/react-in-jsx-scope": "off",
				"small-rules/array-type-generic": "error",
				"small-rules/ban-instances": "off",
				"small-rules/ban-react-fc": "error",
				// Biome's native noRestrictedTypes covers this and is far faster.
				"small-rules/ban-types": "off",
				"small-rules/consistent-compound-words": [
					"error",
					{
						allowList: {},
						checkProperties: false,
						checkShorthandProperties: false,
						checkVariables: false,
						extendDefaultReplacements: false,
						replacements: {},
					},
				],
				"small-rules/curly": "error",
				"small-rules/directive-disable-enable-pair": "error",
				"small-rules/directive-no-aggregating-enable": "error",
				"small-rules/directive-no-duplicate-disable": "error",
				"small-rules/directive-no-restricted-disable": "error",
				"small-rules/directive-no-unlimited-disable": "error",
				"small-rules/directive-no-unused-enable": "error",
				"small-rules/directive-no-use": "error",
				"small-rules/directive-require-description": [
					"error",
					{
						ignore: ["oxlint-enable"],
					},
				],
				"small-rules/enforce-ianitor-check-type": "off",
				"small-rules/isolated-functions": [
					"error",
					{
						comments: [],
						functions: [],
						overrideGlobals: {},
						selectors: [],
					},
				],
				"small-rules/memoized-effect-dependencies": ["error", { environment: "standard" }],
				"small-rules/no-adjust-state-on-prop-change": ["error", { environment: "standard" }],
				"small-rules/no-array-constructor-elements": [
					"error",
					{
						environment: "standard",
						requireExplicitGenericOnNewArray: true,
					},
				],
				"small-rules/no-array-constructor-index-assignment": "error",
				"small-rules/no-array-size-assignment": ["error", { allowAutofix: false }],
				"small-rules/no-async-in-system": "off",
				"small-rules/no-cascading-set-state": "error",
				"small-rules/no-chain-state-updates": ["error", { environment: "standard" }],
				"small-rules/no-chained-type-assertions": "error",
				"small-rules/no-cliffy-action-wrapper": "error",
				"small-rules/no-color3-constructor": "off",
				"small-rules/no-conditional-empty-object-spread": "error",
				"small-rules/no-constant-condition-with-break": [
					"error",
					{
						loopExitCalls: ["break", "return", "throw"],
					},
				],
				"small-rules/no-dead-store": "error",
				"small-rules/no-derived-state": ["error", { environment: "standard" }],
				"small-rules/no-discarded-rejection": "warn",
				"small-rules/no-error": "off",
				"small-rules/no-event-handler": ["error", { environment: "standard" }],
				"small-rules/no-events-in-events-callback": "off",
				"small-rules/no-external-store-subscription": ["error", { environment: "standard" }],
				"small-rules/no-filter-map-chain": "error",
				"small-rules/no-floating-point-equality": "error",
				"small-rules/no-giant-component": "error",
				"small-rules/no-god-components": "off",
				"small-rules/no-has-before-remove-in-jecs": "off",
				"small-rules/no-ianitor-in-function-body": "off",
				"small-rules/no-ianitor-success-access": "off",
				"small-rules/no-identity-map": "error",
				"small-rules/no-increment-decrement": ["error", { allowAutofix: true }],
				"small-rules/no-initialize-state": ["error", { environment: "standard" }],
				"small-rules/no-inline-property-on-memo-component": "error",
				"small-rules/no-inline-signature-types": [
					"error",
					{
						functions: "all",
						position: "anywhere",
						returnTypes: true,
					},
				],
				"small-rules/no-instance-methods-without-this": "off",
				"small-rules/no-jsx-spread-child": "error",
				"small-rules/no-known-value-widening": "error",
				"small-rules/no-module-mocking": "error",
				"small-rules/no-multiple-cliffy-commands": "error",
				"small-rules/no-native-properties-spread": "off",
				"small-rules/no-nested-hook-calls": "error",
				"small-rules/no-new-instance-in-use-memo": "error",
				"small-rules/no-object-parameters": "error",
				"small-rules/no-pass-data-to-parent": ["error", { environment: "standard" }],
				"small-rules/no-pass-live-state-to-parent": ["error", { environment: "standard" }],
				"small-rules/no-print": "off",
				"small-rules/no-reduce-accumulator-copy": "error",
				"small-rules/no-redundant-aspect-ratio-constraint": "off",
				"small-rules/no-reflect-apply": "error",
				"small-rules/no-reflect-get": "error",
				"small-rules/no-render-helper-functions": "error",
				"small-rules/no-reset-all-state-on-prop-change": ["error", { environment: "standard" }],
				"small-rules/no-restricted-property-assignment": "error",
				"small-rules/no-runtime-in-type-files": "error",
				"small-rules/no-runtime-typeof": ["error", { allowInTypeGuards: true }],
				"small-rules/no-shape-in-symbol-names": "error",
				"small-rules/no-spec-file-extension": "error",
				"small-rules/no-static-react-create-element": "error",
				"small-rules/no-table-create-map": "off",
				"small-rules/no-task-wait": "off",
				"small-rules/no-trivial-assertions": "error",
				"small-rules/no-underscore-react-props": "error",
				"small-rules/no-unknown-parameters": "error",
				"small-rules/no-unknown-returns": "error",
				"small-rules/no-unknown-type-aliases": "error",
				"small-rules/no-unsafe-dictionary-type": "error",
				"small-rules/no-unused-imports": "error",
				"small-rules/no-unused-use-memo": ["error", { environment: "standard" }],
				"small-rules/no-use-memo-simple-expression": "error",
				"small-rules/no-useless-constants": "error",
				"small-rules/no-useless-default": "error",
				"small-rules/no-useless-empty-string-comparison": [
					"error",
					{
						allowAutofix: true,
					},
				],
				"small-rules/no-useless-use-effect": ["error", { environment: "standard" }],
				"small-rules/no-useless-use-memo": ["error", { environment: "standard" }],
				"small-rules/no-useless-use-spring": "off",
				"small-rules/no-variadic-spread": "error",
				"small-rules/no-warn": "off",
				"small-rules/no-widen-then-assert": "error",
				"small-rules/only-type-imports": "off",
				"small-rules/prefer-constant-dispatch": "error",
				"small-rules/prefer-context-stack": "off",
				"small-rules/prefer-direct-hook-imports": ["error", { environment: "standard" }],
				"small-rules/prefer-early-return": "error",
				"small-rules/prefer-expect-assertions": "off",
				"small-rules/prefer-hoisted-jsx-elements": [
					"off",
					{
						environment: "standard",
					},
				],
				"small-rules/prefer-hoisted-jsx-object-properties": "off",
				"small-rules/prefer-idiv": "off",
				"small-rules/prefer-local-portal-component": "off",
				"small-rules/prefer-math-min-max": "off",
				"small-rules/prefer-membership-filter-in-jecs": "off",
				"small-rules/prefer-modding-inspect": "off",
				"small-rules/prefer-module-scope-constants": "off",
				"small-rules/prefer-native-collection-copy": "off",
				"small-rules/prefer-optional-parameter": "error",
				"small-rules/prefer-padding-components": "off",
				"small-rules/prefer-pascal-case-enums": "error",
				"small-rules/prefer-sequence-overloads": "off",
				"small-rules/prefer-single-world-query-in-jecs": "off",
				"small-rules/prefer-ternary-conditional-rendering": "error",
				"small-rules/prefer-udim2-shorthand": "off",
				"small-rules/prefer-use-reducer": "error",
				"small-rules/prevent-abbreviations": [
					"error",
					{
						allowPropertyAccess: [
							"char",
							"InstanceProps",
							"InferProps",
							"PropsWithoutRef",
							"ComponentProps",
							"screenProps",
							"getScreenProps",
							"PropsWithChildren",
							"args",
						],
						ignoreShorthands: ["InferProps", "InstanceProps", "PropsWithoutRef", "ComponentProps"],
						shorthands: {
							"*Props": "*Properties",
							"*props": "*properties",
							args: "parameters",
							btn: "button",
							char: "character",
							dt: "deltaTime",
							plr: "player",
							str: "string",
						},
					},
				],
				"small-rules/react-hooks-strict-return": "error",
				"small-rules/require-async-suffix": "error",
				"small-rules/require-module-level-instantiation": "off",
				"small-rules/require-named-effect-functions": "error",
				"small-rules/require-paired-calls": "error",
				"small-rules/require-react-component-keys": "off",
				"small-rules/require-react-display-names": ["error", { environment: "standard" }],
				"small-rules/require-readable-spacing": [
					"error",
					{
						allowAttachedUses: true,
						allowGuardClauseRuns: true,
						allowMultilineBindingRuns: true,
						overrides: [
							{ blankLine: "any", next: "return", previous: "expression" },
							{ blankLine: "any", next: "binding", previous: "enum", usesPrevious: true },
						],
					},
				],
				"small-rules/require-safety-comment-for-type-assertion": "error",
				"small-rules/require-switch-case-braces": [
					"error",
					{
						metric: "lines",
					},
				],
				"small-rules/require-throw-error-capture": [
					"off",
					{
						allow: [{ name: "ValidationError", from: "package", package: "@cliffy/command" }],
					},
				],
				"small-rules/require-tsx-for-hooks": "error",
				"small-rules/require-unicode-regex": "error",
				"small-rules/rerender-memo-with-default-value": "error",
				"small-rules/starts-with-verb": [
					"error",
					{
						allowList: [],
						denyList: [],
					},
				],
				"small-rules/strict-component-boundaries": ["error", { allow: [] }],
				"small-rules/use-exhaustive-dependencies": "error",
				"small-rules/use-hook-at-top-level": "error",
				// shittier
				"sonar/destructuring-assignment-syntax": "off",
				"sonar/no-nested-incdec": "off",
				"sort-imports": [
					"off",
					{
						allowSeparatedGroups: true,
						ignoreCase: true,
						ignoreDeclarationSort: true,
						ignoreMemberSort: false,
					},
				],
				"sort-keys": [
					"off",
					"asc",
					{
						allowLineSeparatedGroups: true,
						caseSensitive: true,
						minKeys: 2,
						natural: true,
					},
				],
				"style/jsx-curly-brace-presence": "off",
				"style/padding-line-between-statements": [
					"error",
					{
						blankLine: "never",
						next: "*",
						prev: "directive",
					},
				],
				"ts/array-type": "off",
				"ts/explicit-member-accessibility": ["error", {}],
				"ts/prefer-readonly-parameter-types": "off",
				"ts/switch-exhaustiveness-check": [
					"error",
					{
						allowDefaultCaseForExhaustiveSwitch: true,
						considerDefaultExhaustiveForUnions: true,
						requireDefaultForNonUnion: false,
					},
				],
				"typescript/explicit-member-accessibility": ["error", {}],
				"unicorn-js/name-replacements": "off",
				"unicorn-js/no-break-in-nested-loop": "off",
				"unicorn-js/no-keyword-prefix": "off",
				"unicorn-js/no-unreadable-new-expression": "off",
				"unicorn-js/prefer-global-number-constants": "off",
				"unicorn/catch-error-name": ["warn", { name: "error" }],
				"unicorn/no-array-callback-reference": "off",
				"unicorn/no-new-array": "error",
				"unicorn/no-process-exit": "off",
				// shit rule that breaks everything:
				"unicorn/no-useless-undefined": "off",
				"unicorn/numeric-separators-style": "off",
				"unicorn/prefer-event-target": "off",
				"unicorn/prefer-math-trunc": "off",
				"unicorn/switch-case-braces": "off",
				"unused-imports/no-unused-vars": "off",
				"vue/no-dupe-keys": "off",
			},
			settings: {
				react: { version: "19.2.8" },
				"small-rules": { tsgolintVersion: "7.0.2002" },
				vitest: { typecheck: true },
			},
			spellCheck: false,
			stylistic: true,
			test: {
				vitest: {
					files: GLOB_TESTS.filter((glob) => !glob.includes(".bench.")),
					typecheck: true,
				},
			},
			type: projectType,
			typescript: {
				outOfProjectFiles: ["*.js"],
				parserOptions: { tsconfigRootDir: nodePath.dirname(tsconfigPath) },
				tsconfigPath,
			},
		},
		{
			name: "sloptor/disable-slop-rules",
			files: [GLOB_SRC],
			rules: {
				"jsdoc/require-param": "off",
			},
		},
		{
			name: "sloptor/use-native-rules",
			files: [GLOB_SRC],
			rules: {
				"eslint-js/id-length": "off",
				"id-length": [
					"error",
					{
						exceptionPatterns: ["^_"],
						exceptions: ["_", "x", "y", "z", "a", "b", "$"],
						max: 45,
					},
				],
				"jsdoc/require-param": "off",
			},
		},
		{
			name: "sloptor/astro-env",
			env: {
				astro: true,
				browser: true,
			},
			files: [`documentation/src/${GLOB_SRC}`],
		},
		{
			name: "sloptor/react-doctor",
			files: [`documentation/${GLOB_SRC}`],
			jsPlugins: [REACT_DOCTOR_PLUGIN],
			rules: {
				...reactDoctorRules,
				// STOP DUPLICATING NATIVE RULES!!!
				"react-doctor/exhaustive-deps": "off",
				"react-doctor/forbid-component-props": "off",
				"react-doctor/jsx-boolean-value": "off",
				"react-doctor/jsx-curly-brace-presence": "off",
				"react-doctor/jsx-max-depth": "off",
				"react-doctor/jsx-no-new-array-as-prop": "off",
				"react-doctor/jsx-no-new-function-as-prop": "off",
				"react-doctor/jsx-no-new-object-as-prop": "off",
				"react-doctor/jsx-props-no-spreading": "off",
				"react-doctor/no-danger": "off",
				"react-doctor/no-multi-comp": "off",
				"react-doctor/no-unknown-property": "off",
				"react-doctor/only-export-components": "off",
				// not enabled because Astro
				"react-doctor/prefer-dynamic-import": "off",
				"react-doctor/react-in-jsx-scope": "off",
			},
			settings: {
				"react-doctor": { rootDirectory },
			},
		},
		{
			name: "sloptor/documentation",
			env: {
				astro: true,
				browser: true,
				node: true,
			},
			files: ["documentation"],
		},
		{
			name: "sloptor/astro-files",
			files: ["documentation/src/**/*.astro"],
			rules: {
				"import/unambiguous": "off",
				"small-rules/no-unused-imports": "off",
				"small-rules/prevent-abbreviations": "off",
				"sonar/unused-import": "off",
				"unicorn/prefer-module": "off",
				"unused-imports/no-unused-imports": "off",
			},
		},
		{
			name: "sloptor/vitest",
			files: GLOB_TESTS.filter((glob) => !glob.includes(".bench.")),
			plugins: ["vitest"],
			rules: {
				"flawless/no-conditional-in-test": "off",
				"flawless/prefer-expect-assertions-count": "off",
				"max-lines": "off",
				"max-lines-per-function": "off",
				"no-console": "error",
				"no-non-null-assertion": "off",
				"small-rules/no-filter-map-chain": "off",
				"small-rules/prefer-expect-assertions": [
					"error",
					{
						additionalAssertionFunctions: ["expectRecord", "expectArray", "expectPresent"],
						additionalExpectCallNames: ["expectRecord", "expectArray", "expectPresent"],
					},
				],
				"small-rules/prevent-abbreviations": "off",
				"typescript/no-non-null-assertion": "off",
				"unicorn-js/no-incorrect-template-string-interpolation": "off",
				"unicorn/no-null": "off",
				"vitest/consistent-each-for": "error",
				"vitest/consistent-test-filename": "error",
				"vitest/consistent-test-it": "error",
				"vitest/consistent-vitest-vi": "error",
				"vitest/expect-expect": "error",
				"vitest/hoisted-apis-on-top": "error",
				"vitest/max-expects": "off",
				"vitest/max-nested-describe": "error",
				"vitest/no-alias-methods": "error",
				"vitest/no-commented-out-tests": "error",
				"vitest/no-conditional-expect": "error",
				"vitest/no-conditional-in-test": "error",
				"vitest/no-conditional-tests": "error",
				"vitest/no-disabled-tests": "error",
				"vitest/no-duplicate-hooks": "error",
				"vitest/no-focused-tests": "error",
				"vitest/no-hooks": "error",
				"vitest/no-identical-title": "error",
				"vitest/no-import-node-test": "error",
				"vitest/no-importing-vitest-globals": "off",
				"vitest/no-interpolation-in-snapshots": "error",
				"vitest/no-large-snapshots": "error",
				"vitest/no-mocks-import": "error",
				"vitest/prefer-called-exactly-once-with": "error",
				"vitest/prefer-called-once": "error",
				"vitest/prefer-called-times": "error",
				"vitest/prefer-describe-function-title": "off",
				"vitest/prefer-expect-assertions": "off",
				"vitest/prefer-expect-type-of": "error",
				"vitest/prefer-import-in-mock": "error",
				"vitest/prefer-importing-vitest-globals": "error",
				"vitest/prefer-strict-boolean-matchers": "error",
				"vitest/prefer-to-be-falsy": "off",
				"vitest/prefer-to-be-object": "error",
				"vitest/prefer-to-be-truthy": "off",
				"vitest/prefer-to-contain": "error",
				"vitest/prefer-todo": "error",
				"vitest/require-awaited-expect-poll": "error",
				"vitest/require-hook": "off",
				"vitest/require-local-test-context-for-concurrent-snapshots": "error",
				"vitest/require-mock-type-parameters": "error",
				"vitest/require-test-timeout": "off",
				"vitest/require-top-level-describe": "off",
				"vitest/valid-expect": "error",
				"vitest/valid-title": "error",
				"vitest/warn-todo": "error",
			},
		},
		{
			name: "sloptor/unit-tests",
			excludeFiles: ["**/*.integration.test.{ts,tsx}"],
			files: GLOB_TESTS.filter((glob) => !glob.includes(".bench.")),
			rules: {
				"small-rules/no-real-temporary-directory": "error",
				"small-rules/no-test-timeout": "error",
			},
		},
		{
			name: "sloptor/allow-top-level-await",
			files: [SCRIPT_FILES, CONFIGURATION_FILES],
			rules: {
				"antfu/no-top-level-await": "off",
				"node/no-top-level-await": "off",
			},
		},
		{
			name: "sloptor/not-browser",
			files: [SCRIPT_FILES, CONFIGURATION_FILES],
			rules: {
				"import/no-nodejs-modules": "off",
				"no-console": "off",
			},
		},
		{
			name: "sloptor/configuration-files",
			files: [CONFIGURATION_FILES],
			rules: { "small-rules/prevent-abbreviations": "off" },
		},
		{
			name: "sloptor/react",
			files: [`documentation/${GLOB_SRC}`],
			plugins: ["react", "react-perf", "jsx-a11y"],
			rules: {
				"react-perf/jsx-no-new-array-as-prop": "off",
				"react-perf/jsx-no-new-function-as-prop": "off",
				"react-perf/jsx-no-new-object-as-prop": "off",
				"react-x/exhaustive-deps": "off",
				"react/forbid-component-props": [
					"off",
					{
						forbid: [
							{
								allowedForPatterns: ["^Select"],
								disallowedFor: [],
								propName: "className",
							},
							"style",
						],
					},
				],
				"react/jsx-curly-brace-presence": [
					"error",
					{
						children: "always",
					},
				],
				"react/jsx-filename-extension": [
					"error",
					{
						extensions: ["tsx"],
						ignoreFilesWithoutCode: true,
					},
				],
				"react/react-in-jsx-scope": "off",
				"small-rules/ban-react-fc": "error",
				"small-rules/memoized-effect-dependencies": ["error", { environment: "standard" }],
				"small-rules/no-static-react-create-element": ["error", { environment: "standard" }],
				"small-rules/prefer-hoisted-jsx-elements": [
					"off",
					{
						additionalHoistableComponents: [],
						additionalStaticFactories: [],
						environment: "standard",
					},
				],
				"small-rules/prefer-hoisted-jsx-object-properties": "off",
				"small-rules/require-named-effect-functions": [
					"error",
					{
						environment: "standard",
						hooks: [
							{ name: "useEffect", allowAsync: false },
							{ name: "useLayoutEffect", allowAsync: false },
							{ name: "useInsertionEffect", allowAsync: false },
							{ name: "useMountEffect", allowAsync: false },
						],
					},
				],
				"small-rules/require-react-display-names": ["error", { environment: "standard" }],
			},
		},
		{
			name: "sloptor/allow-null",
			files: [...GLOB_TESTS.filter((glob) => !glob.includes(".bench.")), "**/*.tsx"],
			rules: { "unicorn/no-null": "off" },
		},
		{
			name: "sloptor/import-slop",
			files: [GLOB_DTS],
			rules: { "import/unambiguous": "off" },
		},
		{
			name: "sloptor/block-useless-rule",
			files: ["documentation/src/integrations/contextual-menu.ts"],
			rules: { "sonar/file-name-differ-from-class": "off" },
		},
	);
}
