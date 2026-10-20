import type { Writable } from "type-fest";

export interface InlineSegment {
	readonly bold: boolean;
	readonly code: boolean;
	readonly text: string;
}

function createFlags(length: number): Array<boolean> {
	return Array.from<boolean>({ length }).fill(false);
}

function markCodeSpans(text: string, code: Array<boolean>, drop: Array<boolean>): void {
	let index = 0;
	while (index < text.length) {
		if (text.charAt(index) !== "`") {
			index += 1;
			continue;
		}

		const close = text.indexOf("`", index + 1);
		if (close === -1) return;

		drop[index] = true;
		drop[close] = true;
		code.fill(true, index + 1, close);
		index = close + 1;
	}
}

function markBoldSpans(text: string, code: ReadonlyArray<boolean>, bold: Array<boolean>, drop: Array<boolean>): void {
	let index = 0;
	while (index < text.length - 1) {
		if (!text.startsWith("**", index) || code[index] === true) {
			index += 1;
			continue;
		}

		let close = index + 2;
		while (close < text.length - 1 && (code[close] === true || !text.startsWith("**", close))) {
			close += 1;
		}

		if (close >= text.length - 1) return;

		drop[index] = true;
		drop[index + 1] = true;
		drop[close] = true;
		drop[close + 1] = true;
		bold.fill(true, index + 2, close);
		index = close + 2;
	}
}

function collectSegments(
	text: string,
	code: ReadonlyArray<boolean>,
	bold: ReadonlyArray<boolean>,
	drop: ReadonlyArray<boolean>,
): Array<InlineSegment> {
	const segments = new Array<InlineSegment>();
	let size = 0;

	let current: undefined | Writable<InlineSegment>;
	for (let position = 0; position < text.length; position += 1) {
		if (drop[position] === true) continue;

		const isCode = code[position] === true;
		const isBold = bold[position] === true;
		if (current?.code === isCode && current.bold === isBold) current.text += text.charAt(position);
		else {
			current = { bold: isBold, code: isCode, text: text.charAt(position) };
			segments[size++] = current;
		}
	}
	return segments;
}

/**
 * Splits text on inline Markdown so titles like "No `print`" or "**No
 * `print`**" can render code spans and bold as elements. Code spans take
 * precedence over bold, and unclosed delimiters stay literal.
 *
 * @param text - The raw title text that may contain backtick or `**` pairs.
 * @returns The flagged segments in source order.
 */
export function parseInlineMarkdown(text: string): ReadonlyArray<InlineSegment> {
	if (text.length === 0) return [];

	const code = createFlags(text.length);
	const bold = createFlags(text.length);
	const drop = createFlags(text.length);
	markCodeSpans(text, code, drop);
	markBoldSpans(text, code, bold, drop);
	return collectSegments(text, code, bold, drop);
}
