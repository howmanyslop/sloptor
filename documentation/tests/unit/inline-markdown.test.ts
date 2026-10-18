import { describe, it } from "vitest";
import { parseInlineMarkdown } from "$utilities/inline-markdown";

describe("parseInlineMarkdown", () => {
	it("returns an empty array for an empty string", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("")).toStrictEqual([]);
	});

	it("returns a single plain segment when no markdown is present", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("No print")).toStrictEqual([{ bold: false, code: false, text: "No print" }]);
	});

	it("parses a single code span", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("No `print`")).toStrictEqual([
			{ bold: false, code: false, text: "No " },
			{ bold: false, code: true, text: "print" },
		]);
	});

	it("parses multiple code spans", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("Prefer `math.min` and `math.max`")).toStrictEqual([
			{ bold: false, code: false, text: "Prefer " },
			{ bold: false, code: true, text: "math.min" },
			{ bold: false, code: false, text: " and " },
			{ bold: false, code: true, text: "math.max" },
		]);
	});

	it("parses a code span at the start and end", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("`a` then `b`")).toStrictEqual([
			{ bold: false, code: true, text: "a" },
			{ bold: false, code: false, text: " then " },
			{ bold: false, code: true, text: "b" },
		]);
	});

	it("treats an unclosed backtick as literal text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("No `print")).toStrictEqual([{ bold: false, code: false, text: "No `print" }]);
	});

	it("treats an unclosed backtick after a closed pair as literal text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("Use `a` then `b")).toStrictEqual([
			{ bold: false, code: false, text: "Use " },
			{ bold: false, code: true, text: "a" },
			{ bold: false, code: false, text: " then `b" },
		]);
	});

	it("treats a dangling trailing backtick as literal text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("No print`")).toStrictEqual([{ bold: false, code: false, text: "No print`" }]);
	});

	it("drops an empty code span", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("a `` b")).toStrictEqual([{ bold: false, code: false, text: "a  b" }]);
	});

	it("parses a title that is only a code span", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("`print`")).toStrictEqual([{ bold: false, code: true, text: "print" }]);
	});

	it("parses a bold span", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("**No print**")).toStrictEqual([{ bold: true, code: false, text: "No print" }]);
	});

	it("parses bold text mixed with plain text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("Prefer **constant** dispatch")).toStrictEqual([
			{ bold: false, code: false, text: "Prefer " },
			{ bold: true, code: false, text: "constant" },
			{ bold: false, code: false, text: " dispatch" },
		]);
	});

	it("parses a code span inside a bold span", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("**No `print`**")).toStrictEqual([
			{ bold: true, code: false, text: "No " },
			{ bold: true, code: true, text: "print" },
		]);
	});

	it("parses bold spanning multiple code spans", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("**a `x` b `y` c**")).toStrictEqual([
			{ bold: true, code: false, text: "a " },
			{ bold: true, code: true, text: "x" },
			{ bold: true, code: false, text: " b " },
			{ bold: true, code: true, text: "y" },
			{ bold: true, code: false, text: " c" },
		]);
	});

	it("keeps double asterisks inside a code span literal", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("Use `**` carefully")).toStrictEqual([
			{ bold: false, code: false, text: "Use " },
			{ bold: false, code: true, text: "**" },
			{ bold: false, code: false, text: " carefully" },
		]);
	});

	it("treats an unclosed bold delimiter as literal text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("No **print")).toStrictEqual([{ bold: false, code: false, text: "No **print" }]);
	});

	it("treats an unclosed bold delimiter after a closed pair as literal text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("**a** then **b")).toStrictEqual([
			{ bold: true, code: false, text: "a" },
			{ bold: false, code: false, text: " then **b" },
		]);
	});

	it("drops an empty bold span", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("a **** b")).toStrictEqual([{ bold: false, code: false, text: "a  b" }]);
	});

	it("treats single asterisks as literal text", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("2 * 3 * 4")).toStrictEqual([{ bold: false, code: false, text: "2 * 3 * 4" }]);
	});

	it("pairs the opening of triple asterisks with the first possible close", ({ expect }) => {
		expect.assertions(1);

		expect(parseInlineMarkdown("***x***")).toStrictEqual([
			{ bold: true, code: false, text: "*x" },
			{ bold: false, code: false, text: "*" },
		]);
	});
});
