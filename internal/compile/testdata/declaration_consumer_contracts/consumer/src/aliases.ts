import { configuration } from "../../producer/out/contracts";
import type { AnimationConfiguration, SpringConfiguration } from "../../producer/out/contracts";
import type { Equal, Expect, NotAny } from "./assertions";

type ExpectedConfiguration = { stiffness: number };

type _AnimationAliasIsExact = Expect<Equal<AnimationConfiguration, ExpectedConfiguration>>;
type _AnimationAliasIsNotAny = Expect<NotAny<AnimationConfiguration>>;
type _SpringAliasIsExact = Expect<Equal<SpringConfiguration, ExpectedConfiguration>>;
type _SpringAliasIsNotAny = Expect<NotAny<SpringConfiguration>>;
type _PublishedConfigurationIsExact = Expect<Equal<typeof configuration, ExpectedConfiguration>>;

export const animation: AnimationConfiguration = configuration;
export const spring: SpringConfiguration = animation;

// @ts-expect-error Configuration stiffness remains numeric through either alias.
export const rejectedConfiguration: AnimationConfiguration = { stiffness: "slow" };
