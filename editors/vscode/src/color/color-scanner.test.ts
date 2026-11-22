import { describe, it } from "vitest";

import { findColor3MatchAt, formatColor3Presentations, scanColor3Calls } from "./color-scanner";

import type { ColorTriple } from "./color-math";
import type { Color3Format } from "./color-scanner";

const TIGHT: Color3Format["separators"] = ["(", ", ", ", ", ")"];

describe("scanColor3Calls", () => {
	it("should return nothing for text without Color3", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("const color = rgb(1, 2, 3);")).toStrictEqual([]);
	});

	it("should return nothing when Color3 appears without a call", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("let color: Color3;")).toStrictEqual([]);
	});

	it("should read new Color3 as RGB components", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("new Color3(0.1, 0.2, 0.3)")).toStrictEqual([
			{ components: [0.1, 0.2, 0.3], kind: "new", length: 25, offset: 0, separators: TIGHT },
		]);
	});

	it("should read Color3.fromRGB as 0..255 components", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("Color3.fromRGB(255, 0, 51)")).toStrictEqual([
			{ components: [1, 0, 0.2], kind: "fromRGB", length: 26, offset: 0, separators: TIGHT },
		]);
	});

	it("should convert Color3.fromHSV to RGB components", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("Color3.fromHSV(0.25, 1, 1)")).toStrictEqual([
			{ components: [0.5, 1, 0], kind: "fromHSV", length: 26, offset: 0, separators: TIGHT },
		]);
	});

	it("should accept leading-dot, trailing-dot, and integer literals", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("new Color3(.5, 1., 0)")[0]?.components).toStrictEqual([0.5, 1, 0]);
	});

	it("should keep the whitespace around the arguments", ({ expect }) => {
		expect.assertions(1);

		const text = "new  Color3 ( 0.5 ,0.25,  1 )";

		expect(scanColor3Calls(text)).toStrictEqual([
			{
				components: [0.5, 0.25, 1],
				kind: "new",
				length: text.length,
				offset: 0,
				separators: [" ( ", " ,", ",  ", " )"],
			},
		]);
	});

	it("should accept a multiline call with a trailing comma", ({ expect }) => {
		expect.assertions(1);

		const text = "Color3.fromRGB(\n\t255,\n\t0,\n\t0,\n)";

		expect(scanColor3Calls(text)).toStrictEqual([
			{
				components: [1, 0, 0],
				kind: "fromRGB",
				length: text.length,
				offset: 0,
				separators: ["(\n\t", ",\n\t", ",\n\t", ",\n)"],
			},
		]);
	});

	it("should accept whitespace around the method dot", ({ expect }) => {
		expect.assertions(1);

		expect(scanColor3Calls("Color3 . fromHSV(0, 0, 1)")[0]?.kind).toBe("fromHSV");
	});

	it("should clamp out-of-range components", ({ expect }) => {
		expect.assertions(3);

		expect(scanColor3Calls("Color3.fromRGB(300, -5, 255)")[0]?.components).toStrictEqual([1, 0, 1]);
		expect(scanColor3Calls("new Color3(2, -1, 0.5)")[0]?.components).toStrictEqual([1, 0, 0.5]);
		expect(scanColor3Calls("Color3.fromHSV(-0.5, 2, 1)")[0]?.components).toStrictEqual([1, 0, 0]);
	});

	it("should ignore calls that are not three numeric literals", ({ expect }) => {
		expect.assertions(1);

		const text = [
			"Color3.fromRGB(red, green, blue);",
			"Color3.fromRGB(1, 2);",
			"Color3.fromRGB(1, 2, 3, 4);",
			"Color3.fromRGB(1 + 1, 2, 3);",
			"Color3.fromRGB(1e3, 0, 0);",
			"Color3.fromRGB(1, 2, 3x);",
			"Color3.fromRGB(, 2, 3);",
			"new Color3();",
			'Color3.fromHex("#ffffff");',
			"Color3.fromRGBA(1, 2, 3);",
			"MyColor3.fromRGB(1, 2, 3);",
			"$Color3.fromRGB(1, 2, 3);",
			"renew Color3(1, 1, 1);",
			"new Color3Value(1, 1, 1);",
		].join("\n");

		expect(scanColor3Calls(text)).toStrictEqual([]);
	});

	it("should find every call in document order, nested inside other code", ({ expect }) => {
		expect.assertions(1);

		const text =
			"const mixed = lerp(Color3.fromRGB(255, 0, 0), new Color3(0, 0, 1), 0.5);\nfoo(Color3.fromHSV(0, 0, 0));";

		expect(scanColor3Calls(text).map((match) => [match.kind, match.offset])).toStrictEqual([
			["fromRGB", text.indexOf("Color3.fromRGB")],
			["new", text.indexOf("new Color3")],
			["fromHSV", text.indexOf("Color3.fromHSV")],
		]);
	});

	it("should give the same result when scanning twice", ({ expect }) => {
		expect.assertions(1);

		const text = "a(Color3.fromRGB(1, 2, 3)); b(new Color3(1, 1, 1));";

		expect(scanColor3Calls(text)).toStrictEqual(scanColor3Calls(text));
	});
});

describe("findColor3MatchAt", () => {
	const text = "x(Color3.fromRGB(1, 2, 3), new Color3(1, 1, 1), Color3.fromHSV(0, 0, 0));";
	const matches = scanColor3Calls(text);

	it("should find the call that starts at an offset", ({ expect }) => {
		expect.assertions(3);

		expect(findColor3MatchAt(matches, text.indexOf("Color3.fromRGB"))?.kind).toBe("fromRGB");
		expect(findColor3MatchAt(matches, text.indexOf("new Color3"))?.kind).toBe("new");
		expect(findColor3MatchAt(matches, text.indexOf("Color3.fromHSV"))?.kind).toBe("fromHSV");
	});

	it("should return undefined when no call starts at an offset", ({ expect }) => {
		expect.assertions(3);

		expect(findColor3MatchAt(matches, 0)).toBeUndefined();
		expect(findColor3MatchAt(matches, text.indexOf("new Color3") + 1)).toBeUndefined();
		expect(findColor3MatchAt(matches, text.length)).toBeUndefined();
	});

	it("should return undefined for no calls", ({ expect }) => {
		expect.assertions(1);

		expect(findColor3MatchAt([], 0)).toBeUndefined();
	});
});

describe("formatColor3Presentations", () => {
	function formatOf(kind: Color3Format["kind"], separators = TIGHT): Color3Format {
		return { kind, separators };
	}

	it("should put fromRGB first for a fromRGB call", ({ expect }) => {
		expect.assertions(1);

		expect(formatColor3Presentations([1, 0, 0], formatOf("fromRGB"), 2)).toStrictEqual([
			"Color3.fromRGB(255, 0, 0)",
			"Color3.fromHSV(0, 1, 1)",
			"new Color3(1, 0, 0)",
		]);
	});

	it("should put fromHSV first for a fromHSV call", ({ expect }) => {
		expect.assertions(1);

		expect(formatColor3Presentations([0.5, 0.25, 0.25], formatOf("fromHSV"), 2)).toStrictEqual([
			"Color3.fromHSV(0, 0.5, 0.5)",
			"Color3.fromRGB(128, 64, 64)",
			"new Color3(0.5, 0.25, 0.25)",
		]);
	});

	it("should put new first for a new call", ({ expect }) => {
		expect.assertions(1);

		expect(formatColor3Presentations([0.2, 0.4, 0.6], formatOf("new"), 2)).toStrictEqual([
			"new Color3(0.2, 0.4, 0.6)",
			"Color3.fromRGB(51, 102, 153)",
			"Color3.fromHSV(0.58, 0.67, 0.6)",
		]);
	});

	it("should keep the call's separators in every form", ({ expect }) => {
		expect.assertions(1);

		const separators: Color3Format["separators"] = [" ( ", " ,", ",\n\t", ",\n)"];

		expect(formatColor3Presentations([0, 0, 1], formatOf("new", separators), 2)).toStrictEqual([
			"new Color3 ( 0 ,0,\n\t1,\n)",
			"Color3.fromRGB ( 0 ,0,\n\t255,\n)",
			"Color3.fromHSV ( 0.67 ,1,\n\t1,\n)",
		]);
	});

	it("should round decimals to the precision and drop trailing zeros", ({ expect }) => {
		expect.assertions(3);

		const color: ColorTriple = [0.123_456, 0.5, 0.75];

		expect(formatColor3Presentations(color, formatOf("new"), 2)[0]).toBe("new Color3(0.12, 0.5, 0.75)");
		expect(formatColor3Presentations(color, formatOf("new"), 4)[0]).toBe("new Color3(0.1235, 0.5, 0.75)");
		expect(formatColor3Presentations(color, formatOf("new"), 0)[0]).toBe("new Color3(0, 1, 1)");
	});

	it("should write fromRGB as integers whatever the precision", ({ expect }) => {
		expect.assertions(1);

		expect(formatColor3Presentations([0.123_456, 0.5, 0.75], formatOf("fromRGB"), 6)[0]).toBe(
			"Color3.fromRGB(31, 128, 191)",
		);
	});

	it("should clamp the precision to 0..6", ({ expect }) => {
		expect.assertions(3);

		const color: ColorTriple = [0.123_456_789, 0, 0];

		expect(formatColor3Presentations(color, formatOf("new"), 10)[0]).toBe("new Color3(0.123457, 0, 0)");
		expect(formatColor3Presentations(color, formatOf("new"), -3)[0]).toBe("new Color3(0, 0, 0)");
		expect(formatColor3Presentations(color, formatOf("new"), 2.9)[0]).toBe("new Color3(0.12, 0, 0)");
	});

	it("should clamp out-of-range picked colors", ({ expect }) => {
		expect.assertions(1);

		expect(formatColor3Presentations([1.2, -0.1, 0.5], formatOf("fromRGB"), 2)).toStrictEqual([
			"Color3.fromRGB(255, 0, 128)",
			"Color3.fromHSV(0.92, 1, 1)",
			"new Color3(1, 0, 0.5)",
		]);
	});

	it("should write text that scans back to the same color", ({ expect }) => {
		expect.assertions(1);

		const labels = formatColor3Presentations([0.25, 0.5, 1], formatOf("new"), 6);
		const bytes = labels.map((label) =>
			scanColor3Calls(label)[0]?.components.map((component) => Math.round(component * 255)),
		);

		expect(bytes).toStrictEqual([
			[64, 128, 255],
			[64, 128, 255],
			[64, 128, 255],
		]);
	});
});
