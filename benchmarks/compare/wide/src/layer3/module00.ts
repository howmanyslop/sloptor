import { computeL2M03, L2M03Kind, BaseL2M03 } from "../layer2/module03";
import { computeL0M26, L0M26Kind, BaseL0M26 } from "../layer0/module26";

export enum L3M00Kind {
	Idle,
	Locked,
	Active,
	Paused,
	Done,
}

export const L3M00_FACTOR = 14;

export class BaseL3M00 {
	protected total = 28;
	public kind = L3M00Kind.Idle;

	public fold(value: number): number {
		this.total += value * L3M00_FACTOR;
		return this.total;
	}

	public advance(): L3M00Kind {
		const order = [L3M00Kind.Idle, L3M00Kind.Locked, L3M00Kind.Active, L3M00Kind.Paused, L3M00Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M00(input: number): number {
	let result = input * 14 + 28;
	result += computeL2M03(input % 9);
	result += computeL0M26(input % 9);
	return result % 16765;
}

export const L3M00_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 28,
	chain: (value) => computeL3M00(value),
};

export function linkL3M00(): Array<number> {
	const results = new Array<number>();
	const instanceL2M03 = new BaseL2M03();
	instanceL2M03.advance();
	results.push(instanceL2M03.read() + L2M03Kind.Active);
	const instanceL0M26 = new BaseL0M26();
	instanceL0M26.advance();
	results.push(instanceL0M26.read() + L0M26Kind.Idle);
	return results;
}
