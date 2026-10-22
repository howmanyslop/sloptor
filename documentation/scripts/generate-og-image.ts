/**
 * Renders public/og/sloptor.png, the 1200x630 card Discord and other unfurlers
 * show for the docs.
 *
 * The card is a one-off build artefact, not part of `astro build`: rendering
 * needs Chromium, and the docs build should not. Rerun it by hand when the
 * card’s copy, the palette, or the benchmarks change:
 *
 *   node --run og
 *
 * The font files come from the same @fontsource packages the site loads, so the
 * card speaks in Newsreader and IBM Plex like every page does. The timings come
 * from src/data/benchmarks.ts, the same numbers the home page prints.
 */
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

import { BENCHMARKS } from "../src/data/benchmarks.ts";

const WIDTH = 1200;
const HEIGHT = 630;
const SCALE = 2;

const root = new URL("../", import.meta.url);
const fontDirectory = new URL("node_modules/", root);
const output = fileURLToPath(new URL("public/og/sloptor.png", root));

const FONTS = {
	monoMedium: new URL("@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-500-normal.woff2", fontDirectory),
	monoRegular: new URL("@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-400-normal.woff2", fontDirectory),
	sans: new URL("@fontsource-variable/ibm-plex-sans/files/ibm-plex-sans-latin-wght-normal.woff2", fontDirectory),
	serif: new URL("@fontsource-variable/newsreader/files/newsreader-latin-opsz-normal.woff2", fontDirectory),
	serifItalic: new URL("@fontsource-variable/newsreader/files/newsreader-latin-opsz-italic.woff2", fontDirectory),
};

async function dataUrl(url) {
	const bytes = await readFile(url);
	return `data:font/woff2;base64,${bytes.toString("base64")}`;
}

const [monoMedium, monoRegular, sans, serif, serifItalic] = await Promise.all([
	dataUrl(FONTS.monoMedium),
	dataUrl(FONTS.monoRegular),
	dataUrl(FONTS.sans),
	dataUrl(FONTS.serif),
	dataUrl(FONTS.serifItalic),
]);

const fullBuild = BENCHMARKS.find(({ label }) => label === "Full build");
if (fullBuild === undefined) throw new Error("BENCHMARKS has no full build to print.");

const readouts = BENCHMARKS.filter((benchmark) => benchmark !== fullBuild)
	.map(
		({ files, label, milliseconds }) =>
			`\t\t\t\t\t<div class="readout"><dt>${label}, ${files} files</dt><dd>${milliseconds}<small>ms</small></dd></div>`,
	)
	.join("\n");

const html = `<!doctype html>
<html lang="en">
	<head>
		<meta charset="utf-8" />
		<style>
			@font-face {
				font-family: "Newsreader";
				font-style: normal;
				font-weight: 200 800;
				src: url(${serif}) format("woff2");
			}
			@font-face {
				font-family: "Newsreader";
				font-style: italic;
				font-weight: 200 800;
				src: url(${serifItalic}) format("woff2");
			}
			@font-face {
				font-family: "IBM Plex Sans";
				font-style: normal;
				font-weight: 100 700;
				src: url(${sans}) format("woff2");
			}
			@font-face {
				font-family: "IBM Plex Mono";
				font-style: normal;
				font-weight: 400;
				src: url(${monoRegular}) format("woff2");
			}
			@font-face {
				font-family: "IBM Plex Mono";
				font-style: normal;
				font-weight: 500;
				src: url(${monoMedium}) format("woff2");
			}

			:root {
				--paper: #fafaf9;
				--ink: #1a1c20;
				--ink-2: #555a63;
				--ink-3: #6b7079;
				--rule: #e2e1dc;
				--footnote: #f2f2f0;
				--slime: #c3ec6a;
				--tangerine: #ffb070;
				--serif: "Newsreader", Georgia, serif;
				--sans: "IBM Plex Sans", system-ui, sans-serif;
				--mono: "IBM Plex Mono", ui-monospace, monospace;
				--gutter: 64px;
			}

			* { box-sizing: border-box; }

			html,
			body {
				width: ${WIDTH}px;
				height: ${HEIGHT}px;
				margin: 0;
			}

			body {
				background: var(--paper);
				color: var(--ink);
			}

			/* Discord shows the card at about 40% of its size, so nothing here is set
			   below 18px: smaller type turns to grey fuzz in the preview. */
			.card {
				display: flex;
				flex-direction: column;
				width: 100%;
				height: 100%;
			}

			.header {
				display: flex;
				align-items: baseline;
				justify-content: space-between;
				padding: 40px var(--gutter) 22px;
				border-bottom: 1px solid var(--rule);
			}

			.wordmark {
				font-family: var(--serif);
				font-size: 52px;
				font-weight: 500;
				line-height: 1;
				letter-spacing: -0.01em;
			}

			.address {
				font-family: var(--mono);
				font-size: 20px;
				line-height: 1;
				color: var(--ink-3);
			}

			.main {
				display: grid;
				grid-template-columns: 286px minmax(0, 1fr) 236px;
				gap: 40px;
				align-items: center;
				flex: 1;
				padding: 0 var(--gutter);
			}

			.note h2 {
				margin: 0 0 14px;
				font-family: var(--serif);
				font-size: 36px;
				font-style: italic;
				font-weight: 400;
				line-height: 1.12;
			}

			.note p {
				margin: 0;
				font-family: var(--sans);
				font-size: 19px;
				line-height: 1.45;
				color: var(--ink-2);
			}

			.figure {
				display: flex;
				gap: 14px;
				align-items: baseline;
				margin: 0;
			}

			.overprint {
				position: relative;
				display: inline-block;
				font-family: var(--mono);
				font-size: 204px;
				font-weight: 500;
				font-variant-numeric: tabular-nums;
				line-height: 1;
				letter-spacing: -0.04em;
				color: var(--ink);
				isolation: isolate;
			}

			.overprint::before,
			.overprint::after {
				position: absolute;
				inset: 0;
				z-index: -1;
				content: attr(data-value);
				mix-blend-mode: multiply;
			}

			.overprint::before {
				color: var(--slime);
				translate: -0.03125em 0.03125em;
			}

			.overprint::after {
				color: var(--tangerine);
				translate: 0.03125em -0.03125em;
			}

			.unit {
				font-family: var(--mono);
				font-size: 34px;
				font-weight: 500;
				line-height: 1;
			}

			.readouts {
				margin: 0;
			}

			.readout {
				padding: 16px 0;
			}

			.readout + .readout {
				border-top: 1px solid var(--rule);
			}

			.readout dt {
				margin-bottom: 6px;
				font-family: var(--sans);
				font-size: 18px;
				font-weight: 500;
				line-height: 1.2;
				color: var(--ink-2);
			}

			.readout dd {
				margin: 0;
				font-family: var(--mono);
				font-size: 32px;
				font-variant-numeric: tabular-nums;
				line-height: 1.1;
			}

			.readout dd small {
				margin-left: 8px;
				font-size: 20px;
				color: var(--ink-3);
			}

			.footer {
				display: flex;
				align-items: baseline;
				justify-content: space-between;
				gap: 40px;
				padding: 24px var(--gutter) 28px;
				background: var(--footnote);
				border-top: 1px solid var(--rule);
			}

			.tagline {
				font-family: var(--serif);
				font-size: 36px;
				line-height: 1.15;
			}

			.meta {
				font-family: var(--sans);
				font-size: 19px;
				line-height: 1.3;
				color: var(--ink-2);
				text-align: right;
			}
		</style>
	</head>
	<body>
		<div class="card">
			<header class="header">
				<span class="wordmark">sloptor</span>
				<span class="address">docs.howmanysmall.com/sloptor</span>
			</header>
			<main class="main">
				<section class="note">
					<h2>${fullBuild.label}, ${fullBuild.files}-file production game</h2>
					<p>fig. 1 — cold, no cache. The output is identical to rbxtsc.</p>
				</section>
				<figure class="figure">
					<span class="overprint" data-value="${fullBuild.milliseconds}">${fullBuild.milliseconds}</span>
					<span class="unit">ms</span>
				</figure>
				<dl class="readouts">
${readouts}
				</dl>
			</main>
			<footer class="footer">
				<span class="tagline">TypeScript in, Roblox out, at native speed.</span>
				<span class="meta">A native roblox-ts toolchain,<br />written in Go</span>
			</footer>
		</div>
	</body>
</html>`;

const browser = await chromium.launch();
try {
	const page = await browser.newPage({ deviceScaleFactor: SCALE, viewport: { height: HEIGHT, width: WIDTH } });
	await page.setContent(html, { waitUntil: "load" });
	await page.evaluate(() => document.fonts.ready);
	await page.screenshot({ path: output, type: "png" });
} finally {
	await browser.close();
}

console.log(`Wrote ${output} (${WIDTH * SCALE}x${HEIGHT * SCALE}).`);
