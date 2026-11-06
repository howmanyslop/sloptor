import { computeL0M24, L0M24Kind, BaseL0M24 } from "../layer0/module24";

export enum L1M09Kind {
	Idle,
	Active,
	Queued,
	Failed,
}

export const L1M09_FACTOR = 14;

export class BaseL1M09 {
	protected total = 65;
	public kind = L1M09Kind.Idle;

	public warp(value: number): number {
		this.total += value * L1M09_FACTOR;
		return this.total;
	}

	public advance(): L1M09Kind {
		const order = [L1M09Kind.Idle, L1M09Kind.Active, L1M09Kind.Queued, L1M09Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M09(input: number): number {
	let result = input * 14 + 65;
	result += computeL0M24(input % 5);
	return result % 22694;
}

export function tableL1M09(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M09(index));
	return lookup;
}

export function linkL1M09(): Array<number> {
	const results = new Array<number>();
	const instanceL0M24 = new BaseL0M24();
	instanceL0M24.advance();
	results.push(instanceL0M24.read() + L0M24Kind.Failed);
	return results;
}
