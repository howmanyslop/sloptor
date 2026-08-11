import { Flamework } from "@flamework/core";

type Unsupported = `prefix-${string}`;

Flamework.createGuard<Unsupported>();
