import { Flamework } from "@flamework/core";

/** @metadata intrinsic-component-attributes */
declare const attributes: { count?: number };

delete attributes.count;
Flamework.createGuard<string>();
