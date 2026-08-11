import { Flamework } from "@flamework/core";

export function missingType<T>() {
	return Flamework.id<T extends string ? 1 : 2>();
}
