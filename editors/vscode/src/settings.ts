import { isBoolean, isJsonArray, isNumber, isString } from "./json";

import type { JsonValue } from "./json";

export interface Settings {
	readonly colorPickerEnabled: boolean;
	readonly colorPrecision: number;
	readonly compilerArguments: ReadonlyArray<string>;
	readonly openOutputToSide: boolean;
	readonly path: string | undefined;
	readonly statusBarEnabled: boolean;
	readonly typeErrors: boolean;
}

const MAXIMUM_PRECISION = 6;

export const DEFAULT_SETTINGS: Settings = {
	colorPickerEnabled: true,
	colorPrecision: 2,
	compilerArguments: ["build", "--watch"],
	openOutputToSide: true,
	path: undefined,
	statusBarEnabled: true,
	typeErrors: false,
};

function readBoolean(value: JsonValue, fallback: boolean): boolean {
	return isBoolean(value) ? value : fallback;
}

function readPrecision(value: JsonValue): number {
	if (!isNumber(value) || !Number.isInteger(value)) return DEFAULT_SETTINGS.colorPrecision;
	return Math.min(Math.max(value, 0), MAXIMUM_PRECISION);
}

function readArguments(value: JsonValue): ReadonlyArray<string> {
	if (!isJsonArray(value)) return DEFAULT_SETTINGS.compilerArguments;

	const result = new Array<string>();
	let size = 0;
	for (const argument of value) {
		if (!isString(argument)) return DEFAULT_SETTINGS.compilerArguments;
		result[size] = argument;
		size += 1;
	}

	return result;
}

function readPath(value: JsonValue): string | undefined {
	if (!isString(value)) return undefined;

	const trimmed = value.trim();
	return trimmed.length === 0 ? undefined : trimmed;
}

/**
 * Reads the `sloptor.*` settings. The package.json schema only guards the
 * settings UI, so a hand-edited settings.json can hold anything; each
 * wrong-shaped value falls back to its default.
 *
 * @param read - Returns the value of a key relative to `sloptor.`, such as
 *   `compiler.arguments`.
 * @returns The validated settings.
 */
export function readSettings(read: (key: string) => JsonValue): Settings {
	return {
		colorPickerEnabled: readBoolean(read("colorPicker.enabled"), DEFAULT_SETTINGS.colorPickerEnabled),
		colorPrecision: readPrecision(read("colorPicker.precision")),
		compilerArguments: readArguments(read("compiler.arguments")),
		openOutputToSide: readBoolean(read("openOutputToSide"), DEFAULT_SETTINGS.openOutputToSide),
		path: readPath(read("path")),
		statusBarEnabled: readBoolean(read("statusBar.enabled"), DEFAULT_SETTINGS.statusBarEnabled),
		typeErrors: readBoolean(read("typeErrors"), DEFAULT_SETTINGS.typeErrors),
	};
}
