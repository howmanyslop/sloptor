import { Predicate } from "effect";

import { isJsonArray } from "./json";

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

function readBoolean(value: JsonValue | undefined, fallback: boolean): boolean {
	return Predicate.isBoolean(value) ? value : fallback;
}

function readPrecision(value?: JsonValue): number {
	if (!Predicate.isNumber(value) || !Number.isInteger(value)) return DEFAULT_SETTINGS.colorPrecision;
	return Math.min(Math.max(value, 0), MAXIMUM_PRECISION);
}

function readArguments(value?: JsonValue): ReadonlyArray<string> {
	if (value === undefined || !isJsonArray(value)) return DEFAULT_SETTINGS.compilerArguments;

	const result = new Array<string>();
	let size = 0;
	for (const argument of value) {
		if (!Predicate.isString(argument)) return DEFAULT_SETTINGS.compilerArguments;
		result[size] = argument;
		size += 1;
	}

	return result;
}

function readPath(value?: JsonValue): string | undefined {
	if (!Predicate.isString(value)) return undefined;

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
export function readSettings(read: (key: string) => JsonValue | undefined): Settings {
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

/**
 * Whether the language server must restart for `next` to apply: only the
 * binary and the `typeErrors` initialization option are fixed at start.
 *
 * @param previous - The settings the server started with.
 * @param next - The settings now.
 * @returns `true` when the server must restart.
 */
export function needsRestart(previous: Settings, next: Settings): boolean {
	return previous.path !== next.path || previous.typeErrors !== next.typeErrors;
}
