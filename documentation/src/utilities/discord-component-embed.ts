/**
 * Builds the `<script id="discord:component-embed">` payload described in
 * https://docs.discord.com/developers/link-previews/component-embeds.
 *
 * Discord caps the payload at 3,000 bytes and rejects the whole embed when a
 * component carries a key it does not know, so the shape here is deliberately
 * smaller than what Components V2 allows: a container holding the page title
 * and description, the card image, a divider, and three link buttons.
 */

/** Brine, the focus color, as the decimal integer `accent_color` wants. */
export const DISCORD_EMBED_ACCENT_COLOR = 0x4f6b12;

/** Discord drops any payload above 3,000 bytes. */
export const DISCORD_EMBED_LIMIT_BYTES = 3000;

const MAX_TITLE_LENGTH = 100;
const MAX_DESCRIPTION_LENGTH = 280;

/**
 * The small grey line above the title, standing in for an OG card's site
 * name.
 */
const PROVIDER_LINE = "-# sloptor docs";

const COMPONENT_TYPE = {
	actionRow: 1,
	button: 2,
	container: 17,
	mediaGallery: 12,
	separator: 14,
	textDisplay: 10,
} as const;

const BUTTON_STYLE_LINK = 5;

interface DiscordActionRow {
	readonly components: ReadonlyArray<DiscordButton>;
	readonly type: number;
}

interface DiscordButton {
	readonly label: string;
	readonly style: number;
	readonly type: number;
	readonly url: string;
}

interface DiscordMediaGallery {
	readonly items: ReadonlyArray<{
		readonly description: string;
		readonly media: { readonly url: string };
	}>;
	readonly type: number;
}

interface DiscordSeparator {
	readonly type: number;
}

interface DiscordTextDisplay {
	readonly content: string;
	readonly type: number;
}

type DiscordComponent = DiscordActionRow | DiscordMediaGallery | DiscordSeparator | DiscordTextDisplay;

export interface DiscordComponentEmbedOptions {
	readonly commandsUrl: string;
	readonly description: string | undefined;
	readonly guideUrl: string;
	readonly imageAlt: string;
	readonly imageUrl: string;
	readonly pageUrl: string;
	readonly repositoryUrl: string;
	readonly title: string;
}

export function createDiscordComponentEmbed(options: DiscordComponentEmbedOptions): string {
	const title = truncate(escapeMarkdown(options.title.trim()), MAX_TITLE_LENGTH);
	const pageUrl = escapeMarkdownUrl(options.pageUrl);
	const heading = `${PROVIDER_LINE}\n## [${title}](${pageUrl})`;

	let description = truncate(options.description?.trim() ?? "", MAX_DESCRIPTION_LENGTH);
	let payload = buildPayload(heading, description, options);

	// The length caps above keep a real page far below the limit. Shrink the
	// description anyway rather than shipping an embed Discord would discard.
	while (countBytes(payload) > DISCORD_EMBED_LIMIT_BYTES && description.length > 0) {
		description = truncate(description, Math.floor(description.length * 0.75));
		payload = buildPayload(heading, description, options);
	}

	return payload;
}

function buildPayload(heading: string, description: string, options: DiscordComponentEmbedOptions): string {
	const content = description.length === 0 ? heading : `${heading}\n${description}`;

	const components: ReadonlyArray<DiscordComponent> = [
		{ content, type: COMPONENT_TYPE.textDisplay },
		{
			items: [
				{
					description: options.imageAlt,
					media: { url: options.imageUrl },
				},
			],
			type: COMPONENT_TYPE.mediaGallery,
		},
		{ type: COMPONENT_TYPE.separator },
		{
			components: [
				{
					label: "Read the guide",
					style: BUTTON_STYLE_LINK,
					type: COMPONENT_TYPE.button,
					url: options.guideUrl,
				},
				{
					label: "Commands",
					style: BUTTON_STYLE_LINK,
					type: COMPONENT_TYPE.button,
					url: options.commandsUrl,
				},
				{
					label: "GitHub",
					style: BUTTON_STYLE_LINK,
					type: COMPONENT_TYPE.button,
					url: options.repositoryUrl,
				},
			],
			type: COMPONENT_TYPE.actionRow,
		},
	];

	const payload = {
		component: {
			accent_color: DISCORD_EMBED_ACCENT_COLOR,
			components,
			type: COMPONENT_TYPE.container,
		},
	};

	// `<` never has to appear literally in the JSON, and escaping it keeps a
	// `</script>` in a page description from closing the element early.
	return JSON.stringify(payload).replaceAll("<", String.raw`\u003c`);
}

/**
 * Markdown is allowed in a text display; a title's own punctuation is not.
 *
 * @returns The value with markdown punctuation escaped.
 */
function escapeMarkdown(value: string): string {
	return value.replaceAll(/[\\[\]*_`~]/gu, (character) => `\\${character}`);
}

/**
 * A closing parenthesis in the URL would end the markdown link early.
 *
 * @returns The URL with parentheses escaped.
 */
function escapeMarkdownUrl(value: string): string {
	return value.replaceAll("(", String.raw`\(`).replaceAll(")", String.raw`\)`);
}

function truncate(value: string, maximumLength: number): string {
	if (value.length <= maximumLength) return value;
	return `${value.slice(0, Math.max(0, maximumLength - 1)).trimEnd()}…`;
}

function countBytes(value: string): number {
	return new TextEncoder().encode(value).length;
}
