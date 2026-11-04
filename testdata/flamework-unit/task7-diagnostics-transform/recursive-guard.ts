import { Flamework } from "@flamework/core";

interface Recursive {
	readonly next: Recursive;
}

Flamework.createGuard<Recursive>();
