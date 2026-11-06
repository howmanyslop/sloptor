import { computeL2M36, L2M36Kind, BaseL2M36 } from "../layer2/module36";

export enum L3M39Kind {
	Done,
	Queued,
	Paused,
	Active,
}

export const L3M39_FACTOR = 2;

export class BaseL3M39 {
	protected total = 5;
	public kind = L3M39Kind.Done;

	public weigh(value: number): number {
		this.total += value * L3M39_FACTOR;
		return this.total;
	}

	public advance(): L3M39Kind {
		const order = [L3M39Kind.Done, L3M39Kind.Queued, L3M39Kind.Paused, L3M39Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M39(input: number): number {
	let result = input * 2 + 5;
	result += computeL2M36(input % 5);
	return result % 91344;
}

export const L3M39_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 5,
	chain: (value) => computeL3M39(value),
};

export function linkL3M39(): Array<number> {
	const results = new Array<number>();
	const instanceL2M36 = new BaseL2M36();
	instanceL2M36.advance();
	results.push(instanceL2M36.read() + L2M36Kind.Idle);
	return results;
}
