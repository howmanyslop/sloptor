import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";

export enum L1M35Kind {
	Idle,
	Locked,
	Done,
}

export const L1M35_FACTOR = 15;

export class BaseL1M35 {
	protected total = 68;
	public kind = L1M35Kind.Idle;

	public mix(value: number): number {
		this.total += value * L1M35_FACTOR;
		return this.total;
	}

	public advance(): L1M35Kind {
		const order = [L1M35Kind.Idle, L1M35Kind.Locked, L1M35Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M35(input: number): number {
	let result = input * 15 + 68;
	result += computeL0M06(input % 5);
	return result % 36432;
}

export const L1M35_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 68,
	chain: (value) => computeL1M35(value),
};

export function linkL1M35(): Array<number> {
	const results = new Array<number>();
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	return results;
}
