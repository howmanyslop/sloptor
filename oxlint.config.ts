// oxlint-disable small-rules/prevent-abbreviations -- ok.
import { argv } from "node:process";
import { defineConfig } from "oxlint";
import { ALL_REACT_DOCTOR_RULES } from "oxlint-plugin-react-doctor";

import type { ExternalPluginEntry } from "oxlint";

const reactDoctorRules = Object.fromEntries(
	Object.entries(ALL_REACT_DOCTOR_RULES).map(([key, value]) => {
		if (key.includes("nextjs-") || key.includes("preact-")) return [key, "off"];
		return [key, value];
	}),
);

const configuration = defineConfig({
	$schema: "node_modules/oxlint/configuration_schema.json",
	categories: {
		correctness: "error",
		nursery: "error",
		pedantic: "error",
		perf: "error",
		restriction: "error",
		style: "error",
		suspicious: "error",
	},
	env: {
		builtin: true,
		node: true,
	},
	ignorePatterns: [
		"**/{dist,do-not-sync-ever,node_modules}/**",
		"**/*.js",
		"scripts/dupes-viewer.html",
		"scripts/clis/**/*.ts",
	],
	jsPlugins: [
		"./plugins/small-rules.js",
		{ name: "react-doctor", specifier: "oxlint-plugin-react-doctor" },
		{ name: "antfu", specifier: "eslint-plugin-antfu" },
		{ name: "unused-imports", specifier: "eslint-plugin-unused-imports" },
		{ name: "sonar", specifier: "eslint-plugin-sonarjs" },
		{ name: "regexp-js", specifier: "eslint-plugin-regexp" },
	],
	options: {
		denyWarnings: true,
		maxWarnings: 0,
		reportUnusedDisableDirectives: "error",
		respectEslintDisableDirectives: true,
		typeAware: true,
		typeCheck: !argv.includes("--lsp"),
	},
	overrides: [
		{
			env: {
				astro: true,
				browser: true,
				node: true,
			},
			files: ["documentation/**/*"],
		},
		{
			// rule-newness.ts spawns git to classify rules; the binary is a fixed system dependency.
			files: ["documentation/src/data/rule-newness.ts"],
			rules: {
				"no-console": "off",
				"sonar/no-os-command-from-path": "off",
			},
		},
		{
			files: ["documentation/**/*.astro"],
			plugins: [],
			rules: {
				"import/unambiguous": "off",
				"small-rules/no-unused-imports": "off",
				"small-rules/prevent-abbreviations": "off",
				"sonar/unused-import": "off",
				"unused-imports/no-unused-imports": "off",
			},
		},
		{
			files: ["documentation/src/types/satteri-optional-peers.d.ts"],
			rules: {
				"import/unambiguous": "off",
				"small-rules/no-unused-imports": "off",
				"typescript/no-redundant-type-constituents": "off",
			},
		},
		{
			files: ["scripts/**/*.{ts,tsx}", "*.config.{ts,mjs,tsx}"],
			rules: { "no-console": "off" },
		},
		{
			files: ["documentation/**/*.astro", "scripts/**/*.{ts,tsx}", "*.config.{ts,mjs,tsx}"],
			rules: { "node/no-top-level-await": "off" },
		},
		{
			files: ["tests/**/*.test.{tsx,ts}"],
			plugins: ["vitest"],
			rules: {
				"max-lines": "off",
				"max-lines-per-function": "off",
				"no-console": "error",
				"no-non-null-assertion": "off",
				"small-rules/prefer-expect-assertions": [
					"error",
					{
						additionalAssertionFunctions: ["expectRecord", "expectArray", "expectPresent"],
						additionalExpectCallNames: ["expectRecord", "expectArray", "expectPresent"],
					},
				],
				"small-rules/prevent-abbreviations": "off",
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
				"vitest/prefer-expect-assertions": "error",
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
			files: ["tests/fixtures/**/*.{ts,tsx}", "*.config.{cjs,mjs,js,cts,mts,ts}", "knip.ts"],
			rules: { "import/no-default-export": "off" },
		},
		{
			files: ["vitest*.config.ts"],
			rules: { "unicorn/no-null": "off" },
		},
		{
			files: ["tests/fixtures/**/*.{ts,tsx}"],
			rules: { "typescript/ban-ts-comment": "off" },
		},
		{
			files: ["**/*.d.ts"],
			rules: { "import/unambiguous": "off" },
		},
		{
			files: ["documentation/src/**/*.{ts,tsx}"],
			plugins: ["react", "react-perf", "jsx-a11y"],
			rules: {
				"react-perf/jsx-no-new-function-as-prop": "off",
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
				"small-rules/ban-react-fc": "error",
				"small-rules/memoized-effect-dependencies": "error",
				"small-rules/no-static-react-create-element": ["error", { environment: "standard" }],
				"small-rules/prefer-hoisted-jsx-elements": [
					"error",
					{
						additionalHoistableComponents: [],
						additionalStaticFactories: [],
						environment: "standard",
					},
				],
				"small-rules/prefer-hoisted-jsx-object-properties": "error",
				"small-rules/require-named-effect-functions": [
					"error",
					{
						environment: "standard",
						hooks: [
							{ allowAsync: false, name: "useEffect" },
							{ allowAsync: false, name: "useLayoutEffect" },
							{ allowAsync: false, name: "useInsertionEffect" },
						],
					},
				],
				"small-rules/require-react-display-names": ["error", { environment: "standard" }],
			},
		},
		{
			files: ["src/**/*.{ts,tsx}"],
			rules: { "small-rules/no-variadic-spread": "error" },
		},
	],
	plugins: ["eslint", "import", "jsdoc", "node", "oxc", "promise", "typescript", "unicorn"],
	rules: {
		...reactDoctorRules,
		"arrow-body-style": ["error", "as-needed"],
		"capitalized-comments": "off",
		complexity: "off",
		curly: ["error", "multi-line"],
		"default-case": "off",
		"eslint/max-lines-per-function": [
			"error",
			{
				IIFEs: false,
				max: 120,
				skipBlankLines: false,
				skipComments: false,
			},
		],
		"func-names": ["error", "as-needed", { generators: "as-needed" }],
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
		"id-length": [
			"error",
			{
				exceptionPatterns: ["^_"],
				exceptions: ["_", "x", "y", "z", "a", "b", "$"],
				max: 45,
			},
		],
		"import/exports-last": "off",
		"import/group-exports": "off",
		"import/max-dependencies": "off",
		"import/no-cycle": [
			"error",
			{
				allowUnsafeDynamicCyclicDependency: true,
				ignoreExternal: false,
				ignoreTypes: true,
				maxDepth: 4294967295,
			},
		],
		"import/no-default-export": "off",
		"import/no-named-export": "off",
		"import/no-nodejs-modules": "off",
		"import/no-relative-parent-imports": "off",
		"import/no-unassigned-import": [
			"error",
			{
				allow: ["**/*.css", "**/*.scss", "**/*.less", "**/*.sass", "@total-typescript/ts-reset"],
			},
		],
		"import/prefer-default-export": "off",
		"init-declarations": "off",
		"jsdoc/require-param-type": "off",
		"jsdoc/require-property-type": "off",
		"jsdoc/require-returns-type": "off",
		"max-lines": "off",
		"max-lines-per-function": "off",
		"max-params": [
			"error",
			{
				countVoidThis: false,
				max: 7,
			},
		],
		"max-statements": "off",
		"new-cap": "off",
		"no-array-constructor": "error",
		"no-bitwise": "off",
		"no-continue": "off",
		"no-duplicate-imports": "off",
		"no-else-return": [
			"error",
			{
				allowElseIf: false,
			},
		],
		"no-magic-numbers": "off",
		"no-nested-ternary": "off",
		"no-new-array": "off",
		"no-optional-chaining": "off",
		"no-plusplus": "off",
		"no-ternary": "off",
		"no-undefined": "off",
		"no-underscore-dangle": [
			"off",
			{
				allow: ["______IGNORE_ME______", "_options", "_tag", "_url"],
			},
		],
		"no-unused-vars": [
			"warn",
			{
				argsIgnorePattern: "^_",
				caughtErrorsIgnorePattern: "^_",
				vars: "all",
				varsIgnorePattern: "^logger$|^_",
			},
		],
		"no-use-before-define": [
			"error",
			{
				functions: false,
			},
		],
		// Doesn't work award
		"no-useless-undefined": "off",
		"no-void": ["error", { allowAsStatement: true }],
		"no-warning-comments": "warn",
		"node/callback-return": "off",
		"node/no-sync": "off",
		"oxc/no-async-await": "off",
		"oxc/no-const-enum": "off",
		"oxc/no-rest-spread-properties": "off",
		"prefer-arrow-callback": "off",
		"prefer-named-capture-group": "off",
		"promise/prefer-await-to-callbacks": "off",
		"react-doctor/jsx-curly-brace-presence": "off",
		"react-doctor/react-in-jsx-scope": "off",
		"regexp-js/confusing-quantifier": "warn",
		"regexp-js/control-character-escape": "error",
		"regexp-js/match-any": "error",
		"regexp-js/negation": "error",
		"regexp-js/no-contradiction-with-assertion": "error",
		"regexp-js/no-dupe-characters-character-class": "error",
		"regexp-js/no-dupe-disjunctions": "error",
		"regexp-js/no-empty-alternative": "warn",
		"regexp-js/no-empty-capturing-group": "error",
		"regexp-js/no-empty-character-class": "error",
		"regexp-js/no-empty-group": "error",
		"regexp-js/no-empty-lookarounds-assertion": "error",
		"regexp-js/no-empty-string-literal": "error",
		"regexp-js/no-escape-backspace": "error",
		"regexp-js/no-extra-lookaround-assertions": "error",
		"regexp-js/no-invalid-regexp": "error",
		"regexp-js/no-invisible-character": "error",
		"regexp-js/no-lazy-ends": "warn",
		"regexp-js/no-legacy-features": "error",
		"regexp-js/no-misleading-capturing-group": "error",
		"regexp-js/no-misleading-unicode-character": "error",
		"regexp-js/no-missing-g-flag": "error",
		"regexp-js/no-non-standard-flag": "error",
		"regexp-js/no-obscure-range": "error",
		"regexp-js/no-optional-assertion": "error",
		"regexp-js/no-potentially-useless-backreference": "warn",
		"regexp-js/no-super-linear-backtracking": "error",
		"regexp-js/no-trivially-nested-assertion": "error",
		"regexp-js/no-trivially-nested-quantifier": "error",
		"regexp-js/no-unused-capturing-group": "error",
		"regexp-js/no-useless-assertions": "error",
		"regexp-js/no-useless-backreference": "error",
		"regexp-js/no-useless-character-class": "error",
		"regexp-js/no-useless-dollar-replacements": "error",
		"regexp-js/no-useless-escape": "error",
		"regexp-js/no-useless-flag": "warn",
		"regexp-js/no-useless-lazy": "error",
		"regexp-js/no-useless-non-capturing-group": "error",
		"regexp-js/no-useless-quantifier": "error",
		"regexp-js/no-useless-range": "error",
		"regexp-js/no-useless-set-operand": "error",
		"regexp-js/no-useless-string-literal": "error",
		"regexp-js/no-useless-two-nums-quantifier": "error",
		"regexp-js/no-zero-quantifier": "error",
		"regexp-js/optimal-lookaround-quantifier": "warn",
		"regexp-js/optimal-quantifier-concatenation": "error",
		"regexp-js/prefer-character-class": "error",
		"regexp-js/prefer-d": "error",
		"regexp-js/prefer-plus-quantifier": "error",
		"regexp-js/prefer-predefined-assertion": "error",
		"regexp-js/prefer-question-quantifier": "error",
		"regexp-js/prefer-range": "error",
		"regexp-js/prefer-set-operation": "error",
		"regexp-js/prefer-star-quantifier": "error",
		"regexp-js/prefer-unicode-codepoint-escapes": "error",
		"regexp-js/prefer-w": "error",
		"regexp-js/simplify-set-operations": "error",
		"regexp-js/sort-flags": "error",
		"regexp-js/strict": "error",
		"regexp-js/use-ignore-case": "error",
		"require-await": "off",
		"small-rules/array-type-generic": "error",
		"small-rules/ban-instances": "off",
		"small-rules/ban-react-fc": "off",
		"small-rules/ban-types": [
			"error",
			{
				bannedTypes: {
					Omit: "Except",
				},
			},
		],
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
		"small-rules/directive-disable-enable-pair": "error",
		"small-rules/directive-no-aggregating-enable": "error",
		"small-rules/directive-no-duplicate-disable": "error",
		"small-rules/directive-no-restricted-disable": "error",
		"small-rules/directive-no-unlimited-disable": "error",
		"small-rules/directive-no-unused-enable": "error",
		"small-rules/directive-no-use": "error",
		"small-rules/directive-require-description": "error",
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
		"small-rules/memoized-effect-dependencies": "off",
		"small-rules/no-adjust-state-on-prop-change": "off",
		"small-rules/no-array-constructor-elements": [
			"error",
			{
				environment: "standard",
				requireExplicitGenericOnNewArray: true,
			},
		],
		"small-rules/no-array-constructor-index-assignment": "error",
		"small-rules/no-array-size-assignment": ["error", { allowAutofix: false }],
		"small-rules/no-async-constructor": "error",
		"small-rules/no-async-in-system": [
			"error",
			{
				additionalSystemTypeNames: [],
				callbackParameterTypes: [],
				synchronousCallbacks: [],
			},
		],
		"small-rules/no-cascading-set-state": "off",
		"small-rules/no-chain-state-updates": "off",
		"small-rules/no-color3-constructor": "off",
		"small-rules/no-commented-code": "error",
		"small-rules/no-constant-condition-with-break": [
			"error",
			{
				loopExitCalls: ["break", "return", "throw"],
			},
		],
		"small-rules/no-dead-store": "error",
		"small-rules/no-derived-state": "off",
		"small-rules/no-error": "off",
		"small-rules/no-event-handler": "off",
		"small-rules/no-events-in-events-callback": "off",
		"small-rules/no-external-store-subscription": "off",
		"small-rules/no-filter-map-chain": "off",
		"small-rules/no-floating-point-equality": "error",
		"small-rules/no-giant-component": "off",
		"small-rules/no-god-components": "off",
		"small-rules/no-ianitor-in-function-body": "off",
		"small-rules/no-ianitor-success-access": "off",
		"small-rules/no-identity-map": "off",
		"small-rules/no-increment-decrement": ["error", { allowAutofix: true }],
		"small-rules/no-initialize-state": "off",
		"small-rules/no-inline-property-on-memo-component": "off",
		"small-rules/no-instance-methods-without-this": "off",
		"small-rules/no-loop-iterable-mutation": "error",
		"small-rules/no-native-properties-spread": "off",
		"small-rules/no-new-instance-in-use-memo": "off",
		"small-rules/no-pass-data-to-parent": "off",
		"small-rules/no-pass-live-state-to-parent": "off",
		"small-rules/no-print": "off",
		"small-rules/no-redundant-aspect-ratio-constraint": "off",
		"small-rules/no-render-helper-functions": "off",
		"small-rules/no-reset-all-state-on-prop-change": "off",
		"small-rules/no-restricted-property-assignment": "off",
		"small-rules/no-spec-file-extension": "error",
		"small-rules/no-static-react-create-element": "error",
		"small-rules/no-table-create-map": "off",
		"small-rules/no-task-wait": "off",
		"small-rules/no-trivial-assertions": "off",
		"small-rules/no-underscore-react-props": "off",
		"small-rules/no-unused-imports": "error",
		"small-rules/no-unused-use-memo": "off",
		"small-rules/no-use-memo-simple-expression": "off",
		"small-rules/no-use-of-empty-return-value": "error",
		"small-rules/no-useless-constants": "error",
		"small-rules/no-useless-default": "off",
		"small-rules/no-useless-use-effect": "off",
		"small-rules/no-useless-use-memo": "off",
		"small-rules/no-useless-use-spring": "off",
		"small-rules/no-variadic-spread": "off",
		"small-rules/no-warn": "off",
		"small-rules/only-type-imports": "off",
		"small-rules/prefer-class-properties": "error",
		"small-rules/prefer-constant-dispatch": "off",
		"small-rules/prefer-context-stack": "off",
		"small-rules/prefer-direct-hook-imports": "off",
		"small-rules/prefer-early-return": "error",
		"small-rules/prefer-expect-assertions": "off",
		"small-rules/prefer-hoisted-jsx-elements": "off",
		"small-rules/prefer-hoisted-jsx-object-properties": "off",
		"small-rules/prefer-idiv": "off",
		"small-rules/prefer-local-portal-component": "off",
		"small-rules/prefer-math-min-max": "off",
		"small-rules/prefer-modding-inspect": "off",
		"small-rules/prefer-module-scope-constants": "off",
		"small-rules/prefer-padding-components": "off",
		"small-rules/prefer-pascal-case-enums": "error",
		"small-rules/prefer-sequence-overloads": "off",
		"small-rules/prefer-single-world-query": "off",
		"small-rules/prefer-singular-enums": "error",
		"small-rules/prefer-ternary-conditional-rendering": "off",
		"small-rules/prefer-udim2-shorthand": "off",
		"small-rules/prefer-use-reducer": "off",
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
		"small-rules/react-hooks-strict-return": "off",
		"small-rules/require-async-suffix": "error",
		"small-rules/require-module-level-instantiation": "off",
		"small-rules/require-named-effect-functions": "off",
		"small-rules/require-paired-calls": "error",
		"small-rules/require-react-component-keys": "off",
		"small-rules/require-react-display-names": "off",
		"small-rules/require-switch-case-braces": [
			"error",
			{
				metric: "lines",
			},
		],
		"small-rules/require-throw-error-capture": [
			"error",
			{
				allow: [{ from: "package", name: "ValidationError", package: "@cliffy/command" }],
			},
		],
		"small-rules/require-unicode-regex": "error",
		"small-rules/rerender-memo-with-default-value": "off",
		"small-rules/strict-component-boundaries": ["error", { allow: [] }],
		"small-rules/use-exhaustive-dependencies": "off",
		"small-rules/use-hook-at-top-level": "off",
		"sonar/anchor-precedence": "error",
		"sonar/argument-type": "error",
		"sonar/arguments-order": "error",
		"sonar/arguments-usage": "off",
		"sonar/array-callback-without-return": "error",
		"sonar/array-constructor": "off",
		"sonar/arrow-function-convention": "off",
		"sonar/assertions-in-tests": "error",
		"sonar/aws-apigateway-public-api": "error",
		"sonar/aws-ec2-rds-dms-public": "error",
		"sonar/aws-ec2-unencrypted-ebs-volume": "error",
		"sonar/aws-efs-unencrypted": "error",
		"sonar/aws-iam-all-privileges": "error",
		"sonar/aws-iam-all-resources-accessible": "off",
		"sonar/aws-iam-privilege-escalation": "error",
		"sonar/aws-iam-public-access": "error",
		"sonar/aws-opensearchservice-domain": "error",
		"sonar/aws-rds-unencrypted-databases": "error",
		"sonar/aws-restricted-ip-admin-access": "error",
		"sonar/aws-s3-bucket-granted-access": "error",
		"sonar/aws-s3-bucket-insecure-http": "error",
		"sonar/aws-s3-bucket-public-access": "error",
		"sonar/aws-s3-bucket-versioning": "error",
		"sonar/aws-sagemaker-unencrypted-notebook": "error",
		"sonar/aws-sns-unencrypted-topics": "error",
		"sonar/aws-sqs-unencrypted-queue": "error",
		"sonar/bitwise-operators": "error",
		"sonar/block-scoped-var": "error",
		"sonar/bool-param-default": "off",
		"sonar/call-argument-line": "error",
		"sonar/chai-determinate-assertion": "error",
		"sonar/class-name": "error",
		"sonar/class-prototype": "off",
		"sonar/code-eval": "error",
		"sonar/cognitive-complexity": "error",
		"sonar/comma-or-logical-or-case": "error",
		// Handled by biome
		"sonar/comment-regex": "off",
		"sonar/concise-regex": "error",
		"sonar/conditional-indentation": "off",
		"sonar/confidential-information-logging": "error",
		"sonar/constructor-for-side-effects": "error",
		"sonar/content-length": "error",
		"sonar/content-security-policy": "error",
		"sonar/cookie-no-httponly": "error",
		"sonar/cors": "error",
		"sonar/csrf": "error",
		"sonar/cyclomatic-complexity": "off",
		"sonar/declarations-in-global-scope": "off",
		"sonar/deprecation": "error",
		"sonar/destructuring-assignment-syntax": "off",
		"sonar/different-types-comparison": "error",
		"sonar/disabled-auto-escaping": "error",
		"sonar/disabled-resource-integrity": "error",
		"sonar/disabled-timeout": "error",
		"sonar/dompurify-unsafe-config": "error",
		"sonar/duplicates-in-character-class": "error",
		"sonar/dynamically-constructed-templates": "error",
		"sonar/elseif-without-else": "off",
		"sonar/empty-string-repetition": "error",
		"sonar/encryption-secure-mode": "error",
		"sonar/existing-groups": "error",
		"sonar/expression-complexity": "off",
		"sonar/file-header": "off",
		"sonar/file-name-differ-from-class": "off",
		"sonar/file-permissions": "error",
		"sonar/file-uploads": "error",
		"sonar/fixme-tag": "error",
		"sonar/for-in": "off",
		"sonar/for-loop-increment-sign": "error",
		"sonar/frame-ancestors": "error",
		"sonar/function-inside-loop": "error",
		"sonar/function-name": "off",
		"sonar/function-return-type": "error",
		"sonar/future-reserved-words": "error",
		"sonar/generator-without-yield": "error",
		"sonar/hardcoded-secret-signatures": "error",
		"sonar/hashing": "error",
		"sonar/hidden-files": "error",
		"sonar/in-operator-type-error": "error",
		"sonar/inconsistent-function-call": "error",
		"sonar/index-of-compare-to-positive-number": "error",
		"sonar/insecure-cookie": "error",
		"sonar/insecure-jwt-token": "error",
		"sonar/inverted-assertion-arguments": "error",
		"sonar/jsx-no-leaked-render": "error",
		"sonar/label-position": "error",
		"sonar/link-with-target-blank": "error",
		"sonar/max-lines": "off",
		"sonar/max-lines-per-function": "off",
		"sonar/max-switch-cases": "error",
		"sonar/max-union-size": "off",
		"sonar/misplaced-loop-counter": "error",
		"sonar/nested-control-flow": "off",
		"sonar/new-operator-misuse": "error",
		"sonar/no-all-duplicated-branches": "error",
		"sonar/no-alphabetical-sort": "error",
		"sonar/no-angular-bypass-sanitization": "error",
		"sonar/no-array-delete": "error",
		"sonar/no-associative-arrays": "error",
		"sonar/no-async-constructor": "error",
		"sonar/no-built-in-override": "off",
		"sonar/no-case-label-in-switch": "error",
		"sonar/no-clear-text-protocols": "error",
		"sonar/no-code-after-done": "error",
		"sonar/no-collapsible-if": "off",
		"sonar/no-collection-size-mischeck": "error",
		"sonar/no-commented-code": "error",
		"sonar/no-control-regex": "error",
		"sonar/no-dead-store": "error",
		"sonar/no-delete-var": "error",
		"sonar/no-duplicate-in-composite": "error",
		"sonar/no-duplicate-string": "off",
		"sonar/no-duplicated-branches": "error",
		"sonar/no-element-overwrite": "error",
		"sonar/no-empty-after-reluctant": "error",
		"sonar/no-empty-alternatives": "error",
		"sonar/no-empty-character-class": "off",
		"sonar/no-empty-collection": "error",
		"sonar/no-empty-group": "error",
		"sonar/no-empty-test-file": "off",
		"sonar/no-equals-in-for-termination": "error",
		"sonar/no-exclusive-tests": "error",
		"sonar/no-extra-arguments": "error",
		"sonar/no-fallthrough": "error",
		"sonar/no-for-in-iterable": "off",
		"sonar/no-function-declaration-in-block": "off",
		"sonar/no-global-this": "error",
		"sonar/no-globals-shadowing": "error",
		"sonar/no-gratuitous-expressions": "error",
		"sonar/no-hardcoded-ip": "error",
		"sonar/no-hardcoded-passwords": "error",
		"sonar/no-hardcoded-secrets": "error",
		"sonar/no-hook-setter-in-body": "error",
		"sonar/no-identical-conditions": "error",
		"sonar/no-identical-expressions": "error",
		"sonar/no-identical-functions": "error",
		"sonar/no-ignored-exceptions": "error",
		"sonar/no-ignored-return": "error",
		"sonar/no-implicit-dependencies": "off",
		"sonar/no-implicit-global": "error",
		"sonar/no-in-misuse": "error",
		"sonar/no-incomplete-assertions": "error",
		"sonar/no-inconsistent-returns": "off",
		"sonar/no-incorrect-string-concat": "off",
		"sonar/no-internal-api-use": "error",
		"sonar/no-intrusive-permissions": "error",
		"sonar/no-invalid-regexp": "off",
		"sonar/no-invariant-returns": "error",
		"sonar/no-inverted-boolean-check": "error",
		"sonar/no-ip-forward": "error",
		"sonar/no-labels": "error",
		"sonar/no-literal-call": "error",
		"sonar/no-mime-sniff": "error",
		"sonar/no-misleading-array-reverse": "error",
		"sonar/no-misleading-character-class": "error",
		"sonar/no-mixed-content": "error",
		"sonar/no-nested-assignment": "error",
		"sonar/no-nested-conditional": "error",
		"sonar/no-nested-functions": "error",
		"sonar/no-nested-incdec": "off",
		"sonar/no-nested-switch": "off",
		"sonar/no-nested-template-literals": "error",
		"sonar/no-os-command-from-path": "error",
		"sonar/no-parameter-reassignment": "error",
		"sonar/no-primitive-wrappers": "error",
		"sonar/no-redundant-assignments": "error",
		"sonar/no-redundant-boolean": "error",
		"sonar/no-redundant-jump": "error",
		"sonar/no-redundant-optional": "off",
		"sonar/no-redundant-parentheses": "off",
		"sonar/no-reference-error": "off",
		"sonar/no-referrer-policy": "error",
		"sonar/no-regex-spaces": "error",
		"sonar/no-require-or-define": "off",
		"sonar/no-return-type-any": "off",
		"sonar/no-same-argument-assert": "error",
		"sonar/no-same-line-conditional": "error",
		"sonar/no-selector-parameter": "error",
		"sonar/no-session-cookies-on-static-assets": "error",
		"sonar/no-skipped-tests": "error",
		"sonar/no-small-switch": "error",
		"sonar/no-sonar-comments": "off",
		"sonar/no-tab": "off",
		"sonar/no-table-as-layout": "error",
		"sonar/no-try-promise": "error",
		"sonar/no-undefined-argument": "error",
		"sonar/no-undefined-assignment": "off",
		"sonar/no-unenclosed-multiline-block": "error",
		"sonar/no-uniq-key": "error",
		"sonar/no-unsafe-unzip": "off",
		"sonar/no-unthrown-error": "error",
		"sonar/no-unused-collection": "error",
		"sonar/no-unused-function-argument": "off",
		"sonar/no-unused-vars": "error",
		"sonar/no-use-of-empty-return-value": "error",
		"sonar/no-useless-catch": "error",
		"sonar/no-useless-increment": "error",
		"sonar/no-useless-intersection": "error",
		"sonar/no-useless-react-setstate": "error",
		"sonar/no-variable-usage-before-declaration": "off",
		"sonar/no-weak-cipher": "error",
		"sonar/no-weak-keys": "error",
		"sonar/no-wildcard-import": "off",
		"sonar/non-existent-operator": "error",
		"sonar/non-number-in-arithmetic-expression": "off",
		"sonar/null-dereference": "error",
		"sonar/object-alt-content": "error",
		"sonar/operation-returning-nan": "off",
		"sonar/os-command": "error",
		"sonar/post-message": "error",
		"sonar/prefer-default-last": "error",
		"sonar/prefer-immediate-return": "off",
		"sonar/prefer-object-literal": "off",
		"sonar/prefer-promise-shorthand": "error",
		"sonar/prefer-read-only-props": "error",
		"sonar/prefer-regexp-exec": "error",
		"sonar/prefer-single-boolean-return": "error",
		"sonar/prefer-type-guard": "error",
		"sonar/prefer-while": "error",
		"sonar/production-debug": "error",
		"sonar/pseudo-random": "error",
		"sonar/public-static-readonly": "error",
		"sonar/publicly-writable-directories": "error",
		"sonar/reduce-initial-value": "error",
		"sonar/redundant-type-aliases": "error",
		"sonar/regex-complexity": "error",
		"sonar/review-blockchain-mnemonic": "error",
		"sonar/session-regeneration": "error",
		"sonar/shorthand-property-grouping": "off",
		"sonar/single-char-in-character-classes": "off",
		"sonar/single-character-alternation": "error",
		"sonar/slow-regex": "error",
		"sonar/sql-queries": "error",
		"sonar/stable-tests": "error",
		"sonar/stateful-regex": "error",
		"sonar/strict-transport-security": "error",
		"sonar/strings-comparison": "off",
		"sonar/table-header": "error",
		"sonar/table-header-reference": "error",
		"sonar/test-check-exception": "error",
		"sonar/todo-tag": "error",
		"sonar/too-many-break-or-continue-in-loop": "off",
		"sonar/unicode-aware-regex": "off",
		"sonar/unused-import": "error",
		"sonar/unused-named-groups": "error",
		"sonar/unverified-certificate": "error",
		"sonar/unverified-hostname": "error",
		"sonar/updated-const-var": "error",
		"sonar/updated-loop-counter": "error",
		"sonar/use-type-alias": "error",
		"sonar/useless-string-operation": "off",
		"sonar/values-not-convertible-to-numbers": "off",
		"sonar/variable-name": "off",
		"sonar/void-use": "error",
		"sonar/weak-ssl": "error",
		"sonar/web-sql-database": "off",
		"sonar/x-powered-by": "error",
		"sonar/xml-parser-xxe": "error",
		"sort-imports": [
			"warn",
			{
				allowSeparatedGroups: true,
				ignoreCase: true,
				ignoreDeclarationSort: true,
				ignoreMemberSort: false,
			},
		],
		"sort-keys": [
			"error",
			"asc",
			{
				allowLineSeparatedGroups: true,
				caseSensitive: true,
				minKeys: 2,
				natural: true,
			},
		],
		"typescript/array-type": [
			"error",
			{
				default: "generic",
				readonly: "generic",
			},
		],
		"typescript/no-unnecessary-condition": [
			"error",
			{
				allowConstantLoopConditions: "always",
				checkTypePredicates: false,
			},
		],
		// Types can be wrong, especially when dealing with packages that use `.d.ts` instead.
		// Highly recommended to disable this on a case-by-case basis.
		"typescript/prefer-literal-enum-member": [
			"error",
			{
				allowBitwiseExpressions: true,
			},
		],
		"typescript/prefer-readonly-parameter-types": "off",
		"typescript/switch-exhaustiveness-check": [
			"error",
			{
				allowDefaultCaseForExhaustiveSwitch: true,
				considerDefaultExhaustiveForUnions: true,
				requireDefaultForNonUnion: false,
			},
		],
		"unicorn/filename-case": ["error", { case: "kebabCase" }],
		"unicorn/no-array-callback-reference": "off",
		"unicorn/no-nested-ternary": "off",
		"unicorn/no-new-array": "error",
		"unicorn/no-process-exit": "off",
		"unicorn/number-literal-case": "off",
		"unicorn/numeric-separators-style": "off",
		"unicorn/prefer-event-target": "off",
		"unicorn/prefer-math-trunc": "off",
		"unicorn/switch-case-braces": "off",
		"unused-imports/no-unused-imports": "error",
		"unused-imports/no-unused-vars": "off",
		"vue/no-dupe-keys": "off",
	},
	settings: {
		jsdoc: {
			augmentsExtendsReplacesDocs: false,
			exemptDestructuredRootsFromChecks: false,
			ignoreInternal: false,
			ignorePrivate: false,
			ignoreReplacesDocs: true,
			implementsReplacesDocs: false,
			overrideReplacesDocs: true,
			tagNamePreference: {},
		},
		"jsx-a11y": {
			attributes: {},
			components: {},
		},
		react: {
			componentWrapperFunctions: [],
			formComponents: [],
			linkComponents: [],
			version: "19.2.7",
		},
		vitest: {
			typecheck: true,
		},
	},
});

// oxlint-disable no-inner-declarations unicorn/consistent-function-scoping -- testing
if (import.meta.main) {
	const START_NEW_LINE = /^/gmu;
	function indentEveryLine(value: string, amount = 1): string {
		return value.replace(START_NEW_LINE, "\t".repeat(amount));
	}

	interface JsonSchema {
		readonly $ref?: string | undefined;
		readonly additionalProperties?: boolean | JsonSchema | undefined;
		readonly default?: unknown;
		readonly description?: string | undefined;
		readonly enum?: ReadonlyArray<unknown> | undefined;
		readonly items?: JsonSchema | undefined;
		readonly minItems?: number | undefined;
		readonly minimum?: number | undefined;
		readonly oneOf?: ReadonlyArray<JsonSchema> | undefined;
		readonly properties?: Record<string, JsonSchema> | undefined;
		readonly required?: ReadonlyArray<string> | undefined;
		readonly type?: string | undefined;
		readonly uniqueItems?: boolean | undefined;
	}

	interface SchemaToJsResult {
		readonly configEntry: string;
		readonly hasOptions: boolean;
	}

	function isRecord(object: unknown): object is Record<string, unknown> {
		return typeof object === "object" && object !== null && !Array.isArray(object);
	}

	function formatDefaultValue(value: unknown): string {
		if (value === null) return "null";
		if (typeof value === "string") return JSON.stringify(value);
		if (typeof value === "number" || typeof value === "boolean") return String(value);
		if (Array.isArray(value)) {
			if (value.length === 0) return "[]";
			return `[${value.map(formatDefaultValue).join(", ")}]`;
		}
		if (!isRecord(value)) return "undefined";

		const entries = Object.entries(value);
		if (entries.length === 0) return "{}";

		const lines = entries.map(([entryKey, entryValue]) => `${entryKey}: ${formatDefaultValue(entryValue)}`);
		return `{ ${lines.join(", ")} }`;
	}

	function defaultExpression(schema: JsonSchema): string {
		if (schema.default !== undefined) return formatDefaultValue(schema.default);

		if (schema.oneOf !== undefined && schema.oneOf.length > 0) {
			const [firstAlternative] = schema.oneOf;
			if (firstAlternative?.type === "array") return "[]";
			if (firstAlternative?.type === "object") return "{}";
			return '""';
		}

		switch (schema.type) {
			case "string":
				return '""';

			case "boolean":
				return "false";

			case "number":
			case "integer":
				return "0";

			case "array":
				return "[]";

			case "object":
				return "{}";

			default:
				return "undefined";
		}
	}

	function schemaToJsConfig(ruleName: string, schema: ReadonlyArray<JsonSchema>): SchemaToJsResult {
		if (!Array.isArray(schema) || schema.length === 0) {
			return { configEntry: `"${ruleName}": "error",`, hasOptions: false };
		}

		const [topLevel] = schema;
		if (topLevel === undefined) {
			return { configEntry: `"${ruleName}": "error",`, hasOptions: false };
		}

		if (topLevel.type === "array" && topLevel.items?.type !== "object") {
			return { configEntry: `"${ruleName}": "error",`, hasOptions: false };
		}

		if (topLevel.type !== "object" && topLevel.oneOf === undefined) {
			return { configEntry: `"${ruleName}": "error",`, hasOptions: false };
		}

		const properties = topLevel.properties ?? {};
		if (Object.keys(properties).length === 0) return { configEntry: `"${ruleName}": "error",`, hasOptions: false };

		const lines: Array<string> = [`"${ruleName}": ["error", {`];
		for (const [key, property] of Object.entries(properties)) {
			lines.push(`\t${key}: ${defaultExpression(property)},`);
		}
		lines.push("}],");

		return { configEntry: lines.join("\n"), hasOptions: true };
	}

	function getJsPluginName(packageName: string): string | undefined {
		const jsPlugins: Array<ExternalPluginEntry> = configuration.jsPlugins;
		for (const jsPlugin of jsPlugins) {
			if (typeof jsPlugin === "object" && jsPlugin.specifier === packageName) {
				return jsPlugin.name;
			}
			if (jsPlugin === packageName) return undefined;
		}
		return undefined;
	}

	async function validateSmallRulesAsync(): Promise<void> {
		const smallRules = await import("./src/index.ts");
		const name = getJsPluginName("@pobammer-ts/small-rules") ?? smallRules.default.meta?.name;
		if (name === undefined) {
			const error = new Error("Could not find the name of the small rules plugin.");
			Error.captureStackTrace(error, validateSmallRulesAsync);
			throw error;
		}

		console.log(`Validating ${name}...`);
		const existingRules = new Set<string>();
		for (const ruleKey of Object.keys(configuration.rules)) {
			if (!ruleKey.startsWith(name)) continue;
			existingRules.add(ruleKey);
		}

		const stringBuilder = new Array<string>();

		for (const [ruleKey, rule] of Object.entries(smallRules.default.rules)) {
			const key = `${name}/${ruleKey}`;
			if (!existingRules.has(key)) {
				const schema = rule.meta?.schema;
				// oxlint-disable-next-line typescript/no-base-to-string -- idc
				if (schema === undefined || String(schema).length === 0) {
					console.log(`Missing rule ${key} has no schema.`);
					stringBuilder.push(`\t"${key}": "error",`);
					continue;
				}

				console.log(`Rule ${key} has a schema.`);
				// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- idc
				const jsConfig = schemaToJsConfig(key, schema as ReadonlyArray<JsonSchema>);
				if (!jsConfig.hasOptions) continue;

				stringBuilder.push(indentEveryLine(jsConfig.configEntry, 1));
			}
		}

		console.log(stringBuilder.join("\n"));
	}

	await validateSmallRulesAsync();
}
// oxlint-enable no-inner-declarations -- testing

export default configuration;
