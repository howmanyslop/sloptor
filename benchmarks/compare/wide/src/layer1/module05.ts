import { computeL0M36, L0M36Kind, BaseL0M36 } from "../layer0/module36";
import { computeL0M20, L0M20Kind, BaseL0M20 } from "../layer0/module20";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L1M05Kind {
	Idle,
	Active,
	Paused,
	Done,
	Locked,
}

export const L1M05_FACTOR = 6;

export class BaseL1M05 {
	protected total = 18;
	public kind = L1M05Kind.Idle;

	public blend(value: number): number {
		this.total += value * L1M05_FACTOR;
		return this.total;
	}

	public advance(): L1M05Kind {
		const order = [L1M05Kind.Idle, L1M05Kind.Active, L1M05Kind.Paused, L1M05Kind.Done, L1M05Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M05(input: number): number {
	let result = input * 6 + 18;
	result += computeL0M36(input % 4);
	result += computeL0M20(input % 3);
	result += computeL0M02(input % 10);
	result += computeL0M01(input % 6);
	return result % 51903;
}

export const L1M05_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 18,
	chain: (value) => computeL1M05(value),
};

export function linkL1M05(): Array<number> {
	const results = new Array<number>();
	const instanceL0M36 = new BaseL0M36();
	instanceL0M36.advance();
	results.push(instanceL0M36.read() + L0M36Kind.Failed);
	const instanceL0M20 = new BaseL0M20();
	instanceL0M20.advance();
	results.push(instanceL0M20.read() + L0M20Kind.Failed);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
