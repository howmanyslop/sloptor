import { makeShared, SHARED_VALUE } from "@alias/shared/mod";
import { dummy, type DummyValue } from "@rbxts/dummy";

export { SHARED_VALUE };

// The declaration emitter has to synthesize an `import("...")` type for this
// inferred return type; that specifier never appears in this file's import
// list, so only a fresh resolve can rewrite it.
export function inferred() {
	return makeShared();
}

// An external-library import must keep its package spelling: the Luau runtime
// resolves that one.
export type External = DummyValue;

export const external = dummy();
