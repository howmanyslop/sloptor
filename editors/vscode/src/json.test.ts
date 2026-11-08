import { describe, it } from "vitest";
import { Predicate } from "effect";

import { isJsonArray, isJsonObject } from "./json";

describe("json guards", () => {
	it("should tell booleans, numbers and strings apart", ({ expect }) => {
		expect.assertions(6);

		expect(Predicate.isBoolean(false)).toBe(true);
		expect(Predicate.isBoolean("false")).toBe(false);
		expect(Predicate.isNumber(0)).toBe(true);
		expect(Predicate.isNumber("0")).toBe(false);
		expect(Predicate.isString("")).toBe(true);
		expect(Predicate.isString(null)).toBe(false);
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
