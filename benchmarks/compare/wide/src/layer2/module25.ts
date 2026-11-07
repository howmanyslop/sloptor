import { computeL1M17, L1M17Kind, BaseL1M17 } from "../layer1/module17";
import { computeL1M24, L1M24Kind, BaseL1M24 } from "../layer1/module24";

export enum L2M25Kind {
	Idle,
	Active,
	Paused,
	Done,
}

export const L2M25_FACTOR = 14;

export class BaseL2M25 {
	protected total = 93;
	public kind = L2M25Kind.Idle;

	public blend(value: number): number {
		this.total += value * L2M25_FACTOR;
		return this.total;
	}

	public advance(): L2M25Kind {
		const order = [L2M25Kind.Idle, L2M25Kind.Active, L2M25Kind.Paused, L2M25Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M25(input: number): number {
	let result = input * 14 + 93;
	result += computeL1M17(input % 10);
	result += computeL1M24(input % 3);
	return result % 14847;
}

export function tableL2M25(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M25(index));
	return lookup;
}

export function linkL2M25(): Array<number> {
	const results = new Array<number>();
	const instanceL1M17 = new BaseL1M17();
	instanceL1M17.advance();
	results.push(instanceL1M17.read() + L1M17Kind.Paused);
	const instanceL1M24 = new BaseL1M24();
	instanceL1M24.advance();
	results.push(instanceL1M24.read() + L1M24Kind.Active);
	return results;
}
