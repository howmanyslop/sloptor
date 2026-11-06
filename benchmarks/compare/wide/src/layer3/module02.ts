import { computeL2M17, L2M17Kind, BaseL2M17 } from "../layer2/module17";

export enum L3M02Kind {
	Failed,
	Idle,
	Locked,
	Done,
	Active,
}

export const L3M02_FACTOR = 10;

export class BaseL3M02 {
	protected total = 63;
	public kind = L3M02Kind.Failed;

	public fold(value: number): number {
		this.total += value * L3M02_FACTOR;
		return this.total;
	}

	public advance(): L3M02Kind {
		const order = [L3M02Kind.Failed, L3M02Kind.Idle, L3M02Kind.Locked, L3M02Kind.Done, L3M02Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M02(input: number): number {
	let result = input * 10 + 63;
	result += computeL2M17(input % 7);
	return result % 53568;
}

export function tableL3M02(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M02(index));
	return lookup;
}

export function linkL3M02(): Array<number> {
	const results = new Array<number>();
	const instanceL2M17 = new BaseL2M17();
	instanceL2M17.advance();
	results.push(instanceL2M17.read() + L2M17Kind.Idle);
	return results;
}
