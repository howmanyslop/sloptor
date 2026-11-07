import { computeL2M28, L2M28Kind, BaseL2M28 } from "../layer2/module28";
import { computeL1M06, L1M06Kind, BaseL1M06 } from "../layer1/module06";

export enum L3M15Kind {
	Queued,
	Active,
	Done,
	Failed,
	Paused,
}

export const L3M15_FACTOR = 3;

export class BaseL3M15 {
	protected total = 62;
	public kind = L3M15Kind.Queued;

	public shift(value: number): number {
		this.total += value * L3M15_FACTOR;
		return this.total;
	}

	public advance(): L3M15Kind {
		const order = [L3M15Kind.Queued, L3M15Kind.Active, L3M15Kind.Done, L3M15Kind.Failed, L3M15Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M15(input: number): number {
	let result = input * 3 + 62;
	result += computeL2M28(input % 9);
	result += computeL1M06(input % 8);
	return result % 28697;
}

export const L3M15_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 62,
	chain: (value) => computeL3M15(value),
};

export function linkL3M15(): Array<number> {
	const results = new Array<number>();
	const instanceL2M28 = new BaseL2M28();
	instanceL2M28.advance();
	results.push(instanceL2M28.read() + L2M28Kind.Failed);
	const instanceL1M06 = new BaseL1M06();
	instanceL1M06.advance();
	results.push(instanceL1M06.read() + L1M06Kind.Done);
	return results;
}
