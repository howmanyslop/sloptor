import { Flamework } from "@flamework/core";

export function createGenericGuard<Type>() {
	return Flamework.createGuard<Type>();
}
