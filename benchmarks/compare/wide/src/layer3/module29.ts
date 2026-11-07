import { computeL2M06, L2M06Kind, BaseL2M06 } from "../layer2/module06";
import { computeL0M04, L0M04Kind, BaseL0M04 } from "../layer0/module04";

export enum L3M29Kind {
	Locked,
	Done,
	Paused,
	Queued,
	Active,
}

export const L3M29_FACTOR = 5;

export class BaseL3M29 {
	protected total = 79;
	public kind = L3M29Kind.Locked;

	public shift(value: number): number {
		this.total += value * L3M29_FACTOR;
		return this.total;
	}

	public advance(): L3M29Kind {
		const order = [L3M29Kind.Locked, L3M29Kind.Done, L3M29Kind.Paused, L3M29Kind.Queued, L3M29Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M29(input: number): number {
	let result = input * 5 + 79;
	result += computeL2M06(input % 7);
	result += computeL0M04(input % 8);
	return result % 77334;
}

export const L3M29_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 79,
	chain: (value) => computeL3M29(value),
};

export function linkL3M29(): Array<number> {
	const results = new Array<number>();
	const instanceL2M06 = new BaseL2M06();
	instanceL2M06.advance();
	results.push(instanceL2M06.read() + L2M06Kind.Active);
	const instanceL0M04 = new BaseL0M04();
	instanceL0M04.advance();
	results.push(instanceL0M04.read() + L0M04Kind.Paused);
	return results;
}
