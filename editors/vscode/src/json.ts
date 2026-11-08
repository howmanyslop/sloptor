/**
 * What settings.json and an LSP response can hold. Neither is checked against
 * any schema on the way in, so every read narrows from here.
 */
export type JsonValue = boolean | JsonArray | JsonObject | null | number | string | undefined;

export type JsonArray = ReadonlyArray<JsonValue>;

export interface JsonObject {
	readonly [key: string]: JsonValue;
}

/**
 * Narrows a JSON value to a boolean.
 *
 * @param value - The value to check.
 * @returns Whether `value` is a boolean.
 */
export function isBoolean(value: JsonValue): value is boolean {
	return typeof value === "boolean";
}

/**
 * Narrows a JSON value to a number.
 *
 * @param value - The value to check.
 * @returns Whether `value` is a number.
 */
export function isNumber(value: JsonValue): value is number {
	return typeof value === "number";
}

/**
 * Narrows a JSON value to a string.
 *
 * @param value - The value to check.
 * @returns Whether `value` is a string.
 */
export function isString(value: JsonValue): value is string {
	return typeof value === "string";
}

/**
 * Narrows a JSON value to an array.
 *
 * @param value - The value to check.
 * @returns Whether `value` is an array.
 */
export function isJsonArray(value: JsonValue): value is JsonArray {
	return Array.isArray(value);
}

/**
 * Narrows a JSON value to an object, which an array is not.
 *
 * @param value - The value to check.
 * @returns Whether `value` is a plain object.
 */
export function isJsonObject(value: JsonValue): value is JsonObject {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
