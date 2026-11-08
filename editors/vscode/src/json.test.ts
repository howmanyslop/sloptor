import { describe, it } from "vitest";

import { isBoolean, isJsonArray, isJsonObject, isNumber, isString } from "./json";

describe("json guards", () => {
	it("should tell booleans, numbers and strings apart", ({ expect }) => {
		expect.assertions(6);

		expect(isBoolean(false)).toBe(true);
		expect(isBoolean("false")).toBe(false);
		expect(isNumber(0)).toBe(true);
		expect(isNumber("0")).toBe(false);
		expect(isString("")).toBe(true);
		expect(isString(null)).toBe(false);
	});

	it("should tell arrays from objects", ({ expect }) => {
		expect.assertions(5);

		expect(isJsonArray([])).toBe(true);
		expect(isJsonArray({})).toBe(false);
		expect(isJsonObject({})).toBe(true);
		expect(isJsonObject([])).toBe(false);
		expect(isJsonObject(null)).toBe(false);
	});
});
