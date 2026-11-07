import { computeL0M16, L0M16Kind, BaseL0M16 } from "../layer0/module16";

export enum L1M25Kind {
	Queued,
	Failed,
	Done,
	Paused,
}

export const L1M25_FACTOR = 13;

export class BaseL1M25 {
	protected total = 92;
	public kind = L1M25Kind.Queued;

	public shift(value: number): number {
		this.total += value * L1M25_FACTOR;
		return this.total;
	}

	public advance(): L1M25Kind {
		const order = [L1M25Kind.Queued, L1M25Kind.Failed, L1M25Kind.Done, L1M25Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M25(input: number): number {
	let result = input * 13 + 92;
	result += computeL0M16(input % 3);
	return result % 34105;
}

export const L1M25_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 92,
	chain: (value) => computeL1M25(value),
};

export function linkL1M25(): Array<number> {
	const results = new Array<number>();
	const instanceL0M16 = new BaseL0M16();
	instanceL0M16.advance();
	results.push(instanceL0M16.read() + L0M16Kind.Paused);
	return results;
}
