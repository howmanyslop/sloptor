import { computeL2M35, L2M35Kind, BaseL2M35 } from "../layer2/module35";
import { computeL2M06, L2M06Kind, BaseL2M06 } from "../layer2/module06";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M16Kind {
	Idle,
	Paused,
	Active,
}

export const L3M16_FACTOR = 6;

export class BaseL3M16 {
	protected total = 61;
	public kind = L3M16Kind.Idle;

	public weigh(value: number): number {
		this.total += value * L3M16_FACTOR;
		return this.total;
	}

	public advance(): L3M16Kind {
		const order = [L3M16Kind.Idle, L3M16Kind.Paused, L3M16Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M16(input: number): number {
	let result = input * 6 + 61;
	result += computeL2M35(input % 11);
	result += computeL2M06(input % 6);
	result += computeL0M01(input % 7);
	return result % 31887;
}

export const L3M16_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 61,
	chain: (value) => computeL3M16(value),
};

export function linkL3M16(): Array<number> {
	const results = new Array<number>();
	const instanceL2M35 = new BaseL2M35();
	instanceL2M35.advance();
	results.push(instanceL2M35.read() + L2M35Kind.Done);
	const instanceL2M06 = new BaseL2M06();
	instanceL2M06.advance();
	results.push(instanceL2M06.read() + L2M06Kind.Active);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
