import { pair } from "../../producer/out/contracts";
import type { AnimationConfiguration } from "../../producer/out/contracts";
import type { ImportedState } from "@contract/dependency";
import type { Equal, Expect, NotAny } from "./assertions";

type ExpectedPair = [{ id: string }, { stiffness: number }];

type _PairIsExact = Expect<Equal<typeof pair, ExpectedPair>>;
type _PairIsNotAny = Expect<NotAny<typeof pair>>;

export const state: ImportedState = pair[0];
export const configuration: AnimationConfiguration = pair[1];

// @ts-expect-error Tuple positions must keep state before configuration.
export const rejectedPair: typeof pair = [configuration, state];
