import { describe, it } from "vitest";

import {
	createDiscordComponentEmbed,
	DISCORD_EMBED_ACCENT_COLOR,
	DISCORD_EMBED_LIMIT_BYTES,
} from "$utilities/discord-component-embed";

import type { DiscordComponentEmbedOptions } from "$utilities/discord-component-embed";

const PAGE_URL = "https://docs.howmanysmall.com/sloptor/guide/introduction/";
const IMAGE_URL = "https://docs.howmanysmall.com/sloptor/og/sloptor.png";

const OPTIONS: DiscordComponentEmbedOptions = {
	commandsUrl: "https://docs.howmanysmall.com/sloptor/commands/",
	description: "What sloptor is and how it matches rbxtsc.",
	guideUrl: "https://docs.howmanysmall.com/sloptor/guide/introduction/",
	imageAlt: "355 ms full build of a 95-file production game.",
	imageUrl: IMAGE_URL,
	pageUrl: PAGE_URL,
	repositoryUrl: "https://github.com/howmanyslop/sloptor",
	title: "Introduction",
};

interface Button {
	readonly label: string;
	readonly style: number;
	readonly type: number;
	readonly url: string;
}

interface TextDisplay {
	readonly content: string;
	readonly type: 10;
}

interface MediaGallery {
	readonly items: ReadonlyArray<{
		readonly description: string;
		readonly media: { readonly url: string };
	}>;
	readonly type: 12;
}

interface Separator {
	readonly type: 14;
}

interface ActionRow {
	readonly components: ReadonlyArray<Button>;
	readonly type: 1;
}

type EmbedComponent = ActionRow | MediaGallery | Separator | TextDisplay;

interface Payload {
	readonly component: {
		readonly accent_color: number;
		readonly components: ReadonlyArray<EmbedComponent>;
		readonly type: number;
	};
}

function isPayload(value: unknown): value is Payload {
	return typeof value === "object" && value !== null && "component" in value;
}

function parse(options: DiscordComponentEmbedOptions): Payload {
	const value = JSON.parse(createDiscordComponentEmbed(options));
	if (!isPayload(value)) throw new Error("Expected a component embed payload.");

	return value;
}

function getText(payload: Payload): string {
	const text = payload.component.components.find((component) => component.type === 10);
	if (text?.type !== 10) throw new Error("Expected a text display component.");

	return text.content;
}

function getGallery(payload: Payload): MediaGallery {
	const gallery = payload.component.components.find((component) => component.type === 12);
	if (gallery?.type !== 12) throw new Error("Expected a media gallery component.");

	return gallery;
}

function getActionRow(payload: Payload): ActionRow {
	const actionRow = payload.component.components.find((component) => component.type === 1);
	if (actionRow?.type !== 1) throw new Error("Expected an action row component.");

	return actionRow;
}

describe("createDiscordComponentEmbed", () => {
	it("wraps the page in a single container with the brine accent", ({ expect }) => {
		expect.assertions(3);

		const payload = parse(OPTIONS);

		expect(payload.component.type).toBe(17);
		expect(payload.component.accent_color).toBe(DISCORD_EMBED_ACCENT_COLOR);
		expect(payload.component.components).toHaveLength(4);
	});

	it("names the docs above a linked page title and keeps the description beneath it", ({ expect }) => {
		expect.assertions(1);

		expect(getText(parse(OPTIONS))).toBe(
			`-# sloptor docs\n## [Introduction](${PAGE_URL})\nWhat sloptor is and how it matches rbxtsc.`,
		);
	});

	it("escapes markdown punctuation in the title", ({ expect }) => {
		expect.assertions(1);

		expect(getText(parse({ ...OPTIONS, title: "A [bracketed] title" }))).toContain(
			String.raw`A \[bracketed\] title`,
		);
	});

	it("leaves the description line out when the page has none", ({ expect }) => {
		expect.assertions(1);

		expect(getText(parse({ ...OPTIONS, description: undefined }))).toBe(
			`-# sloptor docs\n## [Introduction](${PAGE_URL})`,
		);
	});

	it("shows the card image, then a divider, then three link buttons", ({ expect }) => {
		expect.assertions(4);

		const payload = parse(OPTIONS);
		const gallery = getGallery(payload);
		const actions = getActionRow(payload);

		expect(gallery.items[0]?.media.url).toBe(IMAGE_URL);
		expect(actions.components.map(({ label, style }) => ({ label, style }))).toStrictEqual([
			{ label: "Read the guide", style: 5 },
			{ label: "Commands", style: 5 },
			{ label: "GitHub", style: 5 },
		]);
		expect(actions.components.every((button) => button.type === 2)).toBe(true);
		expect(payload.component.components.map(({ type }) => type)).toStrictEqual([10, 12, 14, 1]);
	});

	it("stays inside Discord's 3,000-byte limit for an oversized page", ({ expect }) => {
		expect.assertions(3);

		const options = { ...OPTIONS, description: "a".repeat(10_000), title: "t".repeat(10_000) };
		const json = createDiscordComponentEmbed(options);
		const text = getText(parse(options));

		expect(new TextEncoder().encode(json).length).toBeLessThanOrEqual(DISCORD_EMBED_LIMIT_BYTES);
		expect(text).toContain("…");
		expect(text.length).toBeLessThan(1000);
	});

	it("never emits a raw closing script tag", ({ expect }) => {
		expect.assertions(2);

		const json = createDiscordComponentEmbed({ ...OPTIONS, description: "</script><script>alert(1)</script>" });

		expect(json).not.toContain("<");
		expect(json).toContain(String.raw`\u003c/script>`);
	});
});
