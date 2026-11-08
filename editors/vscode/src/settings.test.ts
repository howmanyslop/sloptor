import { describe, it } from "vitest";

import { DEFAULT_SETTINGS, needsRestart, readSettings } from "./settings";

import type { JsonObject, JsonValue } from "./json";

function createReader(values: JsonObject): (key: string) => JsonValue | undefined {
	return function read(key) {
		return values[key];
	};
}

describe("readSettings", () => {
	it("should return the defaults when nothing is set", ({ expect }) => {
		expect.assertions(1);

		expect(readSettings(createReader({}))).toStrictEqual(DEFAULT_SETTINGS);
	});

	it("should keep values of the right shape", ({ expect }) => {
		expect.assertions(1);

		const settings = readSettings(
			createReader({
				"colorPicker.enabled": false,
				"colorPicker.precision": 4,
				"compiler.arguments": ["build", "--build", "--watch"],
				openOutputToSide: false,
				path: "bin/sloptor",
				"statusBar.enabled": false,
				typeErrors: true,
			}),
		);

		expect(settings).toStrictEqual({
			colorPickerEnabled: false,
			colorPrecision: 4,
			compilerArguments: ["build", "--build", "--watch"],
			openOutputToSide: false,
			path: "bin/sloptor",
			statusBarEnabled: false,
			typeErrors: true,
		});
	});

	it("should fall back to the defaults for values of the wrong shape", ({ expect }) => {
		expect.assertions(1);

		const settings = readSettings(
			createReader({
				"colorPicker.enabled": "yes",
				"colorPicker.precision": "2",
				"compiler.arguments": "build --watch",
				openOutputToSide: 1,
				path: 42,
				"statusBar.enabled": null,
				typeErrors: "true",
			}),
		);

		expect(settings).toStrictEqual(DEFAULT_SETTINGS);
	});

	it("should fall back to the default arguments when any argument is not a string", ({ expect }) => {
		expect.assertions(1);

		const settings = readSettings(createReader({ "compiler.arguments": ["build", 1] }));

		expect(settings.compilerArguments).toStrictEqual(DEFAULT_SETTINGS.compilerArguments);
	});

	it("should keep an empty argument list", ({ expect }) => {
		expect.assertions(1);

		const settings = readSettings(createReader({ "compiler.arguments": [] }));

		expect(settings.compilerArguments).toStrictEqual([]);
	});

	it("should clamp an out-of-range precision", ({ expect }) => {
		expect.assertions(2);

		expect(readSettings(createReader({ "colorPicker.precision": -3 })).colorPrecision).toBe(0);
		expect(readSettings(createReader({ "colorPicker.precision": 9 })).colorPrecision).toBe(6);
	});

	it("should fall back to the default precision for a fraction", ({ expect }) => {
		expect.assertions(2);

		expect(readSettings(createReader({ "colorPicker.precision": 2.5 })).colorPrecision).toBe(2);
		expect(readSettings(createReader({ "colorPicker.precision": Number.NaN })).colorPrecision).toBe(2);
	});

	it("should trim the path and treat a blank one as unset", ({ expect }) => {
		expect.assertions(2);

		const padded = createReader({ path: "  /opt/sloptor  " });
		const blank = createReader({ path: " ".repeat(3) });

		expect(readSettings(padded).path).toBe("/opt/sloptor");
		expect(readSettings(blank).path).toBeUndefined();
	});
});

describe("needsRestart", () => {
	it("should restart when the binary or typeErrors changes", ({ expect }) => {
		expect.assertions(2);

		expect(needsRestart(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, path: "/opt/sloptor" })).toBe(true);
		expect(needsRestart(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, typeErrors: true })).toBe(true);
	});

	it("should not restart for settings the extension reads live", ({ expect }) => {
		expect.assertions(1);

		const next = {
			...DEFAULT_SETTINGS,
			colorPrecision: 4,
			compilerArguments: ["build"],
			openOutputToSide: false,
			statusBarEnabled: false,
		};

		expect(needsRestart(DEFAULT_SETTINGS, next)).toBe(false);
	});
});
