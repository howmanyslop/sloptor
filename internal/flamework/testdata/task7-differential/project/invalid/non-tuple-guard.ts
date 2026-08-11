import { Modding } from "@flamework/core";

/** @metadata macro */
declare function tupleGuards(value?: Modding.Intrinsic<"tuple-guards", [string], unknown>): unknown;

export const result = tupleGuards();
