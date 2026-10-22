import { fileURLToPath } from "node:url";
import { satteri } from "@astrojs/markdown-satteri";
import mdx from "@astrojs/mdx";
import react from "@astrojs/react";
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import { Predicate } from "effect";
import { getTsconfig } from "get-tsconfig";

import createContextualMenuIntegration from "./src/integrations/contextual-menu";
import { syntaxDarkTheme, syntaxLightTheme } from "./src/utilities/syntax-themes";

import type { AstroIntegration } from "astro";

const SITE_BASE = "/sloptor";

function fromDocumentationRoot(path: string): string {
	return fileURLToPath(new URL(path, import.meta.url));
}

function ensureAstroIntegration<Integration extends AstroIntegration>(
	integration: Integration,
): AstroIntegration & Pick<Integration, "hooks" | "name"> {
	if (!Predicate.isObject(integration)) {
		throw new TypeError(
			`Expected Astro integration to be an object, received: ${Object.prototype.toString.call(integration)}`,
		);
	}

	const name = "name" in integration ? integration.name : undefined;
	if (name === undefined || name.length === 0) {
		throw new Error(`Expected Astro integration to have a non-empty string "name" property, received: ${name}`);
	}

	const hooks = "hooks" in integration ? integration.hooks : undefined;
	if (!Predicate.isObject(hooks)) {
		throw new TypeError(
			`Expected Astro integration "${name}" to have a "hooks" object, received: ${String(hooks)}`,
		);
	}

	return { name, hooks };
}

function removeTrailingWildcard(path: string): string {
	return path.endsWith("/*") ? path.slice(0, -2) : path;
}

function getAliases(): Record<string, string> {
	const tsconfig = getTsconfig(fromDocumentationRoot("."), "tsconfig.json");
	if (tsconfig === null) throw new Error("Could not load tsconfig.json.");

	const paths = tsconfig.config.compilerOptions?.paths;
	if (paths === undefined) return {};

	return Object.fromEntries(
		Object.entries(paths).map(([pattern, targets]) => {
			const [target] = targets;
			if (target === undefined) {
				throw new Error(`Expected a path target for "${pattern}".`);
			}

			return [removeTrailingWildcard(pattern), fromDocumentationRoot(removeTrailingWildcard(target))];
		}),
	);
}

export default defineConfig({
	base: SITE_BASE,
	integrations: [
		ensureAstroIntegration(
			starlight({
				components: {
					Head: "./src/components/overrides/head.astro",
					Header: "./src/components/overrides/header.astro",
					PageTitle: "./src/components/overrides/page-title.astro",
					Sidebar: "./src/components/overrides/sidebar.astro",
				},
				customCss: [
					"@fontsource-variable/ibm-plex-sans",
					"@fontsource-variable/newsreader/opsz.css",
					"@fontsource-variable/newsreader/opsz-italic.css",
					"@fontsource/ibm-plex-mono/400.css",
					"@fontsource/ibm-plex-mono/500.css",
					"./src/styles/custom.css",
				],
				description: "A native roblox-ts toolchain, written in Go",
				editLink: {
					baseUrl: "https://github.com/howmanyslop/sloptor/edit/main/documentation/",
				},
				expressiveCode: {
					styleOverrides: {
						borderColor: "var(--rule)",
						borderRadius: "0",
						borderWidth: "1px",
						codeFontFamily: "var(--font-mono)",
						frames: {
							editorActiveTabIndicatorTopColor: "var(--ink)",
							frameBoxShadowCssValue: "none",
							terminalBackground: "var(--well)",
							terminalTitlebarBackground: "var(--well)",
							terminalTitlebarBorderBottomColor: "var(--rule)",
							terminalTitlebarDotsOpacity: "0",
							terminalTitlebarForeground: "var(--ink-3)",
						},
						uiFontFamily: "var(--font-sans)",
					},
					themes: [syntaxLightTheme, syntaxDarkTheme],
				},
				favicon: "/favicon.svg?v=3",
				plugins: [],
				sidebar: [
					{
						items: [
							{ label: "Introduction", slug: "guide/introduction" },
							{ label: "Install", slug: "guide/install" },
							{ label: "Migrate from rbxtsc", slug: "guide/migrate" },
						],
						label: "Start",
					},
					{ items: [{ autogenerate: { directory: "commands" } }], label: "Commands" },
					{
						items: [
							{ label: "rotor.toml", slug: "configuration/rotor-toml" },
							{ label: "tsconfig rbxts", slug: "configuration/tsconfig" },
							{ label: "Environment", slug: "configuration/environment" },
						],
						label: "Configuration",
					},
					{
						items: [
							{ label: "Macros", slug: "macros" },
							{ label: "Deploy and assets", slug: "deploy" },
							{ label: "Architecture", slug: "architecture" },
							{ label: "Roadmap", slug: "roadmap" },
							{ label: "Contributing", slug: "contributing" },
							{ label: "Changelog", slug: "changelog" },
						],
						label: "More",
					},
				],
				social: [
					{
						href: "https://github.com/howmanyslop/sloptor",
						icon: "github",
						label: "GitHub",
					},
				],
				title: "sloptor",
			}),
		),
		ensureAstroIntegration(mdx()),
		ensureAstroIntegration(react({ compiler: true })),
		ensureAstroIntegration(createContextualMenuIntegration()),
	],
	markdown: {
		// Command docs are full of `--flags`; smart dashes would turn them into
		// en dashes.
		processor: satteri({ features: { smartPunctuation: false } }),
	},
	outDir: "./dist/sloptor",
	site: "https://docs.howmanysmall.com",
	vite: {
		build: {
			rolldownOptions: {
				output: {
					assetFileNames: "_astro/[name].[hash][extname]",
					chunkFileNames: "_astro/[name].[hash].js",
					entryFileNames: "_astro/[name].[hash].js",
				},
			},
		},
		css: {
			transformer: "lightningcss",
		},
		resolve: {
			alias: getAliases(),
		},
	},
});
