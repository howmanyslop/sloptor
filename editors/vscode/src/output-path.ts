import { Predicate } from "effect";

import { isJsonObject } from "./json";

import type { JsonValue } from "./json";

export const OUTPUT_PATH_METHOD = "sloptor/outputPath";

/** `RequestFailed`: the server's way of saying why there is no output path. */
const REQUEST_FAILED = -32_803;

export type OutputPathAnswer =
	| { readonly kind: "malformed" }
	| { readonly kind: "not-typescript" }
	| { readonly kind: "path"; readonly path: string };

interface RequestFailure {
	readonly code: number;
	readonly message: string;
}

const MALFORMED: OutputPathAnswer = { kind: "malformed" };
const NOT_TYPESCRIPT: OutputPathAnswer = { kind: "not-typescript" };

function isRequestFailure(cause: unknown): cause is RequestFailure {
	return (
		typeof cause === "object" &&
		cause !== null &&
		"code" in cause &&
		"message" in cause &&
		cause.code === REQUEST_FAILED &&
		typeof cause.message === "string"
	);
}

/**
 * Reads the result of `sloptor/outputPath`: `{ path }` for a source file,
 * `null` for anything else.
 *
 * @param value - The result as it came off the wire.
 * @returns What the server answered, or `malformed` for a shape it never
 *   sends.
 */
export function parseOutputPathAnswer(value: JsonValue): OutputPathAnswer {
	if (value === null) return NOT_TYPESCRIPT;
	if (!isJsonObject(value)) return MALFORMED;

	const { path } = value;
	if (!Predicate.isString(path) || path.length === 0) return MALFORMED;
	return { kind: "path", path };
}

/**
 * The message to show for a failed `sloptor/outputPath`. A `RequestFailed`
 * error carries the server's reason as its message.
 *
 * @param cause - What the request rejected with.
 * @returns A sentence for the user.
 */
export function describeOutputPathError(cause: unknown): string {
	if (isRequestFailure(cause)) return `sloptor can't find the output: ${cause.message}`;
	if (cause instanceof Error) return `sloptor/outputPath failed: ${cause.message}`;
	return "sloptor/outputPath failed.";
}
