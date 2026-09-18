import type { OrderedMembers, OrderedUnion } from "../../producer/out/contracts";
import type { Equal, Expect, NotAny } from "./assertions";

type ExpectedMembers = { leading: number; trailing: string };
type ExpectedUnion = { kind: "string"; value: string } | { kind: "number"; value: number };

type _MembersAreExact = Expect<Equal<OrderedMembers, ExpectedMembers>>;
type _MembersAreNotAny = Expect<NotAny<OrderedMembers>>;
type _UnionIsExact = Expect<Equal<OrderedUnion, ExpectedUnion>>;
type _UnionIsNotAny = Expect<NotAny<OrderedUnion>>;

export const orderedMembers: OrderedMembers = { leading: 1, trailing: "ok" };

// @ts-expect-error A string branch cannot carry a numeric payload.
export const rejectedUnion: OrderedUnion = { kind: "string", value: 1 };

export function narrow(union: OrderedUnion): string {
  if (union.kind === "string") {
    return union.value;
  }
  return "number";
}
