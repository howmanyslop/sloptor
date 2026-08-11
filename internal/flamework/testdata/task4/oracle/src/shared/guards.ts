import { Flamework } from "@flamework/core";

interface Leaf {
	readonly id: string;
	readonly value: number;
}

interface Repeated {
	readonly first: Leaf;
	readonly second: Leaf;
	readonly third: Leaf;
}

export const repeatedGuard = Flamework.createGuard<Repeated>();
export const unionGuard = Flamework.createGuard<"ready" | number | undefined>();
export const tupleGuard = Flamework.createGuard<readonly [string, number, boolean]>();
