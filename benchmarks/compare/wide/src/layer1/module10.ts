import { computeL0M31, L0M31Kind, BaseL0M31 } from "../layer0/module31";

export enum L1M10Kind {
	Idle,
	Active,
	Queued,
	Done,
}

export const L1M10_FACTOR = 2;

export class BaseL1M10 {
	protected total = 17;
	public kind = L1M10Kind.Idle;

	public blend(value: number): number {
		this.total += value * L1M10_FACTOR;
		return this.total;
	}

	public advance(): L1M10Kind {
		const order = [L1M10Kind.Idle, L1M10Kind.Active, L1M10Kind.Queued, L1M10Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M10(input: number): number {
	let result = input * 2 + 17;
	result += computeL0M31(input % 7);
	return result % 73245;
}

export const L1M10_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 17,
	chain: (value) => computeL1M10(value),
};

export function linkL1M10(): Array<number> {
	const results = new Array<number>();
	const instanceL0M31 = new BaseL0M31();
	instanceL0M31.advance();
	results.push(instanceL0M31.read() + L0M31Kind.Failed);
	return results;
}
