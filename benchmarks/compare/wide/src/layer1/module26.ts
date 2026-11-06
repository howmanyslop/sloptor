import { computeL0M23, L0M23Kind, BaseL0M23 } from "../layer0/module23";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";
import { computeL0M27, L0M27Kind, BaseL0M27 } from "../layer0/module27";

export enum L1M26Kind {
	Locked,
	Done,
	Paused,
}

export const L1M26_FACTOR = 4;

export class BaseL1M26 {
	protected total = 84;
	public kind = L1M26Kind.Locked;

	public tally(value: number): number {
		this.total += value * L1M26_FACTOR;
		return this.total;
	}

	public advance(): L1M26Kind {
		const order = [L1M26Kind.Locked, L1M26Kind.Done, L1M26Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M26(input: number): number {
	let result = input * 4 + 84;
	result += computeL0M23(input % 5);
	result += computeL0M01(input % 11);
	result += computeL0M27(input % 9);
	return result % 29453;
}

export const L1M26_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 84,
	chain: (value) => computeL1M26(value),
};

export function linkL1M26(): Array<number> {
	const results = new Array<number>();
	const instanceL0M23 = new BaseL0M23();
	instanceL0M23.advance();
	results.push(instanceL0M23.read() + L0M23Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	const instanceL0M27 = new BaseL0M27();
	instanceL0M27.advance();
	results.push(instanceL0M27.read() + L0M27Kind.Done);
	return results;
}
