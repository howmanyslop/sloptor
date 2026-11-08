import { Predicate } from "effect";

import type { JsonArray, JsonObject, JsonValue } from "type-fest";

export type { JsonArray, JsonObject, JsonPrimitive, JsonValue } from "type-fest";

export function isJsonArray(value: JsonValue): value is JsonArray {
	return Array.isArray(value);
}

export function isJsonObject(value: JsonValue): value is JsonObject {
	return Predicate.isObject(value);
}
