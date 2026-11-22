import { describe, it } from "vitest";

import { clampPrecision, clampUnit, convertHsvToRgb, convertRgbToHsv, roundToPrecision } from "./color-math";

import type { ColorTriple } from "./color-math";

function roundTriple(triple: ColorTriple): ColorTriple {
	return [roundToPrecision(triple[0], 6), roundToPrecision(triple[1], 6), roundToPrecision(triple[2], 6)];
}

describe("clampUnit", () => {
	it("should keep values inside 0..1", ({ expect }) => {
		expect.assertions(3);

		expect(clampUnit(0)).toBe(0);
		expect(clampUnit(0.3)).toBeCloseTo(0.3, 12);
		expect(clampUnit(1)).toBe(1);
	});

	it("should clamp values outside 0..1", ({ expect }) => {
		expect.assertions(2);

		expect(clampUnit(1.5)).toBe(1);
		expect(clampUnit(-0.2)).toBe(0);
	});
});

describe("clampPrecision", () => {
	it("should keep whole precisions inside 0..6", ({ expect }) => {
		expect.assertions(3);

		expect(clampPrecision(0)).toBe(0);
		expect(clampPrecision(2)).toBe(2);
		expect(clampPrecision(6)).toBe(6);
	});

	it("should truncate fractional precisions", ({ expect }) => {
		expect.assertions(1);

		expect(clampPrecision(2.9)).toBe(2);
	});

	it("should clamp precisions outside 0..6", ({ expect }) => {
		expect.assertions(3);

		expect(clampPrecision(-3)).toBe(0);
		expect(clampPrecision(10)).toBe(6);
		expect(clampPrecision(Number.POSITIVE_INFINITY)).toBe(6);
	});

	it("should turn NaN into 0", ({ expect }) => {
		expect.assertions(1);

		expect(clampPrecision(Number.NaN)).toBe(0);
	});
});

describe("roundToPrecision", () => {
	it("should round to a value that prints as the given number of decimal places", ({ expect }) => {
		expect.assertions(3);

		expect(String(roundToPrecision(0.123_456, 2))).toBe("0.12");
		expect(String(roundToPrecision(0.123_456, 4))).toBe("0.1235");
		expect(String(roundToPrecision(0.29, 2))).toBe("0.29");
	});

	it("should round to a whole number at precision 0", ({ expect }) => {
		expect.assertions(2);

		expect(roundToPrecision(0.4, 0)).toBe(0);
		expect(roundToPrecision(0.5, 0)).toBe(1);
	});
});

describe("convertHsvToRgb", () => {
	it("should convert the primary and secondary hues", ({ expect }) => {
		expect.assertions(4);

		expect(convertHsvToRgb(0, 1, 1)).toStrictEqual([1, 0, 0]);
		expect(convertHsvToRgb(0.5, 1, 1)).toStrictEqual([0, 1, 1]);
		expect(roundTriple(convertHsvToRgb(1 / 3, 1, 1))).toStrictEqual([0, 1, 0]);
		expect(roundTriple(convertHsvToRgb(2 / 3, 1, 1))).toStrictEqual([0, 0, 1]);
	});

	it("should convert a hue in each of the six sectors", ({ expect }) => {
		expect.assertions(6);

		expect(convertHsvToRgb(1 / 12, 1, 1)).toStrictEqual([1, 0.5, 0]);
		expect(convertHsvToRgb(0.25, 1, 1)).toStrictEqual([0.5, 1, 0]);
		expect(roundTriple(convertHsvToRgb(5 / 12, 1, 1))).toStrictEqual([0, 1, 0.5]);
		expect(roundTriple(convertHsvToRgb(7 / 12, 1, 1))).toStrictEqual([0, 0.5, 1]);
		expect(convertHsvToRgb(0.75, 1, 1)).toStrictEqual([0.5, 0, 1]);
		expect(roundTriple(convertHsvToRgb(11 / 12, 1, 1))).toStrictEqual([1, 0, 0.5]);
	});

	it("should wrap hue 1 around to red", ({ expect }) => {
		expect.assertions(1);

		expect(convertHsvToRgb(1, 1, 1)).toStrictEqual([1, 0, 0]);
	});

	it("should apply saturation and value", ({ expect }) => {
		expect.assertions(3);

		expect(convertHsvToRgb(0, 0.5, 1)).toStrictEqual([1, 0.5, 0.5]);
		expect(convertHsvToRgb(0, 1, 0.5)).toStrictEqual([0.5, 0, 0]);
		expect(convertHsvToRgb(0.3, 0, 0.25)).toStrictEqual([0.25, 0.25, 0.25]);
	});

	it("should clamp out-of-range components", ({ expect }) => {
		expect.assertions(2);

		expect(convertHsvToRgb(2, 1, 1)).toStrictEqual([1, 0, 0]);
		expect(convertHsvToRgb(-1, -1, 3)).toStrictEqual([1, 1, 1]);
	});
});

describe("convertRgbToHsv", () => {
	it("should give grays hue and saturation 0", ({ expect }) => {
		expect.assertions(3);

		expect(convertRgbToHsv(0, 0, 0)).toStrictEqual([0, 0, 0]);
		expect(convertRgbToHsv(0.5, 0.5, 0.5)).toStrictEqual([0, 0, 0.5]);
		expect(convertRgbToHsv(1, 1, 1)).toStrictEqual([0, 0, 1]);
	});

	it("should convert colors whose maximum is red", ({ expect }) => {
		expect.assertions(3);

		expect(convertRgbToHsv(1, 0, 0)).toStrictEqual([0, 1, 1]);
		expect(convertRgbToHsv(1, 1, 0)).toStrictEqual([1 / 6, 1, 1]);
		expect(convertRgbToHsv(1, 0, 1)).toStrictEqual([5 / 6, 1, 1]);
	});

	it("should convert colors whose maximum is green", ({ expect }) => {
		expect.assertions(2);

		expect(convertRgbToHsv(0, 1, 0)).toStrictEqual([1 / 3, 1, 1]);
		expect(convertRgbToHsv(0, 1, 1)).toStrictEqual([0.5, 1, 1]);
	});

	it("should convert colors whose maximum is blue", ({ expect }) => {
		expect.assertions(1);

		expect(convertRgbToHsv(0, 0, 1)).toStrictEqual([2 / 3, 1, 1]);
	});

	it("should compute saturation and value from the extremes", ({ expect }) => {
		expect.assertions(1);

		expect(convertRgbToHsv(0.5, 0.25, 0.25)).toStrictEqual([0, 0.5, 0.5]);
	});

	it("should round-trip through convertHsvToRgb", ({ expect }) => {
		expect.assertions(1);

		const colors: ReadonlyArray<ColorTriple> = [
			[0.2, 0.4, 0.6],
			[0.9, 0.1, 0.3],
			[0.35, 0.8, 0.05],
		];
		const roundTripped = colors.map(([red, green, blue]) => {
			const [hue, saturation, value] = convertRgbToHsv(red, green, blue);
			return roundTriple(convertHsvToRgb(hue, saturation, value));
		});

		expect(roundTripped).toStrictEqual(colors);
	});
});
