import { Flamework } from "@flamework/core";

type Unsupported = `prefix-${string}`;

export const invalidGuard = Flamework.createGuard<Unsupported>();
