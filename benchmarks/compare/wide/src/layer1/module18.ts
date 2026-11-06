import { computeL0M07, L0M07Kind, BaseL0M07 } from "../layer0/module07";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L1M18Kind {
	Idle,
	Active,
	Locked,
	Failed,
}

export const L1M18_FACTOR = 10;

export class BaseL1M18 {
	protected total = 4;
	public kind = L1M18Kind.Idle;

	public mix(value: number): number {
		this.total += value * L1M18_FACTOR;
		return this.total;
	}

	public advance(): L1M18Kind {
		const order = [L1M18Kind.Idle, L1M18Kind.Active, L1M18Kind.Locked, L1M18Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M18(input: number): number {
	let result = input * 10 + 4;
	result += computeL0M07(input % 8);
	result += computeL0M01(input % 10);
	return result % 53164;
}

export const L1M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 4,
	chain: (value) => computeL1M18(value),
};

export function linkL1M18(): Array<number> {
	const results = new Array<number>();
	const instanceL0M07 = new BaseL0M07();
	instanceL0M07.advance();
	results.push(instanceL0M07.read() + L0M07Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
