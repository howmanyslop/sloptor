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

interface Section {
	readonly accessory: Button;
	readonly components: ReadonlyArray<{ readonly content: string }>;
	readonly type: 9;
}

interface MediaGallery {
	readonly items: ReadonlyArray<{
		readonly description: string;
		readonly media: { readonly url: string };
	}>;
	readonly type: 12;
}

interface ActionRow {
	readonly components: ReadonlyArray<Button>;
	readonly type: 1;
}

type EmbedComponent = ActionRow | MediaGallery | Section;

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

function getSection(payload: Payload): Section {
	const section = payload.component.components.find((component) => component.type === 9);
	if (section?.type !== 9) throw new Error("Expected a section component.");

	return section;
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

function getText(section: Section): string {
	const [text] = section.components;
	if (text === undefined) throw new Error("Expected a text display component.");

	return text.content;
}

describe("createDiscordComponentEmbed", () => {
	it("wraps the page in a single container with the brine accent", ({ expect }) => {
		expect.assertions(3);

		const payload = parse(OPTIONS);

		expect(payload.component.type).toBe(17);
		expect(payload.component.accent_color).toBe(DISCORD_EMBED_ACCENT_COLOR);
		expect(payload.component.components).toHaveLength(3);
	});

	it("links the page title and keeps the description beneath it", ({ expect }) => {
		expect.assertions(2);

		const section = getSection(parse(OPTIONS));

		expect(getText(section)).toBe(`# **[Introduction](${PAGE_URL})**\nWhat sloptor is and how it matches rbxtsc.`);
		expect(section.accessory).toStrictEqual({ label: "Open", style: 5, type: 2, url: PAGE_URL });
	});

	it("escapes markdown punctuation in the title", ({ expect }) => {
		expect.assertions(1);

		const section = getSection(parse({ ...OPTIONS, title: "A [bracketed] title" }));

		expect(getText(section)).toContain(String.raw`A \[bracketed\] title`);
	});

	it("leaves the description line out when the page has none", ({ expect }) => {
		expect.assertions(1);

		const section = getSection(parse({ ...OPTIONS, description: undefined }));

		expect(getText(section)).toBe(`# **[Introduction](${PAGE_URL})**`);
	});

	it("shows the card image and three link buttons", ({ expect }) => {
		expect.assertions(3);

		const payload = parse(OPTIONS);
		const gallery = getGallery(payload);
		const actions = getActionRow(payload);

		expect(gallery.items[0]?.media.url).toBe(IMAGE_URL);
		expect(actions.components.map(({ label, style }) => ({ label, style }))).toStrictEqual([
			{ label: "Guide", style: 5 },
			{ label: "Commands", style: 5 },
			{ label: "GitHub", style: 5 },
		]);
		expect(actions.components.every((button) => button.type === 2)).toBe(true);
	});

	it("stays inside Discord's 3,000-byte limit for an oversized page", ({ expect }) => {
		expect.assertions(3);

		const options = { ...OPTIONS, description: "a".repeat(10_000), title: "t".repeat(10_000) };
		const json = createDiscordComponentEmbed(options);
		const text = getText(getSection(parse(options)));

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
