import { computeL0M04, L0M04Kind, BaseL0M04 } from "../layer0/module04";

export enum L1M29Kind {
	Idle,
	Queued,
	Failed,
}

export const L1M29_FACTOR = 6;

export class BaseL1M29 {
	protected total = 45;
	public kind = L1M29Kind.Idle;

	public tally(value: number): number {
		this.total += value * L1M29_FACTOR;
		return this.total;
	}

	public advance(): L1M29Kind {
		const order = [L1M29Kind.Idle, L1M29Kind.Queued, L1M29Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M29(input: number): number {
	let result = input * 6 + 45;
	result += computeL0M04(input % 4);
	return result % 45122;
}

export const L1M29_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 45,
	chain: (value) => computeL1M29(value),
};

export function linkL1M29(): Array<number> {
	const results = new Array<number>();
	const instanceL0M04 = new BaseL0M04();
	instanceL0M04.advance();
	results.push(instanceL0M04.read() + L0M04Kind.Paused);
	return results;
}
