import { clampPrecision, clampUnit, convertHsvToRgb, convertRgbToHsv, roundToPrecision } from "./color-math";

import type { ColorTriple } from "./color-math";

export type Color3Kind = "fromHSV" | "fromRGB" | "new";

/**
 * The text around the three arguments: `(`, the two commas, and `)`, each
 * with its whitespace.
 */
export type Color3Separators = readonly [open: string, first: string, second: string, close: string];

export interface Color3Format {
	readonly kind: Color3Kind;
	readonly separators: Color3Separators;
}

export interface Color3Match extends Color3Format {
	/** Red, green, and blue, each clamped to 0..1, whatever the call's form. */
	readonly components: ColorTriple;
	readonly length: number;
	readonly offset: number;
}

const COLOR3_MARKER = "Color3";
const KIND_ORDER: ReadonlyArray<Color3Kind> = ["fromRGB", "fromHSV", "new"];

type GroupName = "close" | "first" | "firstComma" | "method" | "open" | "second" | "secondComma" | "third";

const COLOR3_CALL =
	/(?<![\w$])(?:new\s+Color3|Color3\s*\.\s*(?<method>fromRGB|fromHSV))(?<open>\s*\(\s*)(?<first>-?(?:\d+(?:\.\d*)?|\.\d+))(?<firstComma>\s*,\s*)(?<second>-?(?:\d+(?:\.\d*)?|\.\d+))(?<secondComma>\s*,\s*)(?<third>-?(?:\d+(?:\.\d*)?|\.\d+))(?<close>\s*(?:,\s*)?\))/gu;

/**
 * Reads a named group. Only `method` is ever missing, on `new Color3`.
 *
 * @param match - A {@link COLOR3_CALL} match.
 * @param name - Which argument or separator to read.
 * @returns The group's text, or an empty string when it did not participate.
 */
function readGroup(match: RegExpExecArray, name: GroupName): string {
	return match.groups?.[name] ?? "";
}

function readKind(method: string): Color3Kind {
	if (method === "fromRGB") return "fromRGB";
	return method === "fromHSV" ? "fromHSV" : "new";
}

function readComponents(kind: Color3Kind, first: number, second: number, third: number): ColorTriple {
	if (kind === "fromRGB") {
		return [clampUnit(first / 255), clampUnit(second / 255), clampUnit(third / 255)];
	}

	if (kind === "fromHSV") return convertHsvToRgb(first, second, third);
	return [clampUnit(first), clampUnit(second), clampUnit(third)];
}

/**
 * Finds every `new Color3(r, g, b)`, `Color3.fromRGB(r, g, b)`, and
 * `Color3.fromHSV(h, s, v)` with three numeric literal arguments, in one pass.
 * Calls with any other argument are skipped.
 *
 * @param text - The whole document.
 * @returns The calls in document order.
 */
export function scanColor3Calls(text: string): ReadonlyArray<Color3Match> {
	if (!text.includes(COLOR3_MARKER)) {
		return [];
	}

	const matches = new Array<Color3Match>();
	let size = 0;

	COLOR3_CALL.lastIndex = 0;
	let match = COLOR3_CALL.exec(text);
	while (match !== null) {
		const kind = readKind(readGroup(match, "method"));
		matches[size++] = {
			components: readComponents(
				kind,
				Number(readGroup(match, "first")),
				Number(readGroup(match, "second")),
				Number(readGroup(match, "third")),
			),
			kind,
			length: match[0].length,
			offset: match.index,
			separators: [
				readGroup(match, "open"),
				readGroup(match, "firstComma"),
				readGroup(match, "secondComma"),
				readGroup(match, "close"),
			],
		};
		match = COLOR3_CALL.exec(text);
	}

	return matches;
}

/**
 * Finds the call that starts at an offset.
 *
 * @param matches - Calls in document order, as {@link scanColor3Calls} returns them.
 * @param offset - The offset the call starts at.
 * @returns The call, or `undefined` when none starts there.
 */
export function findColor3MatchAt(matches: ReadonlyArray<Color3Match>, offset: number): Color3Match | undefined {
	let low = 0;
	let high = matches.length;

	while (low < high) {
		const middle = (low + high) >>> 1;
		const match = matches[middle];
		if (match !== undefined && match.offset < offset) {
			low = middle + 1;
		} else {
			high = middle;
		}
	}

	const found = matches[low];
	return found?.offset === offset ? found : undefined;
}

function formatDecimal(value: number, precision: number): string {
	return String(roundToPrecision(value, precision));
}

function formatKind(kind: Color3Kind, color: ColorTriple, separators: Color3Separators, precision: number): string {
	const [open, first, second, close] = separators;
	const [red, green, blue] = color;

	if (kind === "fromRGB") {
		return `Color3.fromRGB${open}${Math.round(red * 255)}${first}${Math.round(green * 255)}${second}${Math.round(blue * 255)}${close}`;
	}

	if (kind === "fromHSV") {
		const [hue, saturation, value] = convertRgbToHsv(red, green, blue);
		return `Color3.fromHSV${open}${formatDecimal(hue, precision)}${first}${formatDecimal(saturation, precision)}${second}${formatDecimal(value, precision)}${close}`;
	}

	return `new Color3${open}${formatDecimal(red, precision)}${first}${formatDecimal(green, precision)}${second}${formatDecimal(blue, precision)}${close}`;
}

/**
 * Writes a picked color as each of the three Color3 forms, keeping the
 * call's whitespace. `fromRGB` gets integers 0..255; the other two get up to
 * `precision` decimals, trailing zeros dropped.
 *
 * @param color - Red, green, and blue, each 0..1. Out-of-range values are clamped.
 * @param format - The call being replaced. Its form comes first.
 * @param precision - Decimal places for `new` and `fromHSV`, clamped to 0..6.
 * @returns Three replacement texts, the call's current form first.
 */
export function formatColor3Presentations(
	color: ColorTriple,
	format: Color3Format,
	precision: number,
): ReadonlyArray<string> {
	const clamped: ColorTriple = [clampUnit(color[0]), clampUnit(color[1]), clampUnit(color[2])];
	const places = clampPrecision(precision);

	const labels = new Array<string>();
	let size = 0;
	labels[size++] = formatKind(format.kind, clamped, format.separators, places);

	for (const kind of KIND_ORDER) {
		if (kind === format.kind) continue;
		labels[size++] = formatKind(kind, clamped, format.separators, places);
	}

	return labels;
}
