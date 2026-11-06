import { computeL2M10, L2M10Kind, BaseL2M10 } from "../layer2/module10";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M01Kind {
	Done,
	Paused,
	Active,
}

export const L3M01_FACTOR = 17;

export class BaseL3M01 {
	protected total = 19;
	public kind = L3M01Kind.Done;

	public weigh(value: number): number {
		this.total += value * L3M01_FACTOR;
		return this.total;
	}

	public advance(): L3M01Kind {
		const order = [L3M01Kind.Done, L3M01Kind.Paused, L3M01Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M01(input: number): number {
	let result = input * 17 + 19;
	result += computeL2M10(input % 5);
	result += computeL0M01(input % 5);
	return result % 83224;
}

export const L3M01_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 19,
	chain: (value) => computeL3M01(value),
};

export function linkL3M01(): Array<number> {
	const results = new Array<number>();
	const instanceL2M10 = new BaseL2M10();
	instanceL2M10.advance();
	results.push(instanceL2M10.read() + L2M10Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
