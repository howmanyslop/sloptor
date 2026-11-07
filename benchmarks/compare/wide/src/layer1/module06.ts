import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";
import { computeL0M27, L0M27Kind, BaseL0M27 } from "../layer0/module27";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L1M06Kind {
	Done,
	Locked,
	Queued,
	Failed,
}

export const L1M06_FACTOR = 5;

export class BaseL1M06 {
	protected total = 55;
	public kind = L1M06Kind.Done;

	public warp(value: number): number {
		this.total += value * L1M06_FACTOR;
		return this.total;
	}

	public advance(): L1M06Kind {
		const order = [L1M06Kind.Done, L1M06Kind.Locked, L1M06Kind.Queued, L1M06Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M06(input: number): number {
	let result = input * 5 + 55;
	result += computeL0M03(input % 3);
	result += computeL0M27(input % 4);
	result += computeL0M01(input % 11);
	return result % 4177;
}

export const L1M06_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 55,
	chain: (value) => computeL1M06(value),
};

export function linkL1M06(): Array<number> {
	const results = new Array<number>();
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	const instanceL0M27 = new BaseL0M27();
	instanceL0M27.advance();
	results.push(instanceL0M27.read() + L0M27Kind.Done);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
