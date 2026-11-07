import { computeL2M39, L2M39Kind, BaseL2M39 } from "../layer2/module39";
import { computeL2M24, L2M24Kind, BaseL2M24 } from "../layer2/module24";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L3M28Kind {
	Idle,
	Queued,
	Failed,
}

export const L3M28_FACTOR = 19;

export class BaseL3M28 {
	protected total = 37;
	public kind = L3M28Kind.Idle;

	public scale(value: number): number {
		this.total += value * L3M28_FACTOR;
		return this.total;
	}

	public advance(): L3M28Kind {
		const order = [L3M28Kind.Idle, L3M28Kind.Queued, L3M28Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M28(input: number): number {
	let result = input * 19 + 37;
	result += computeL2M39(input % 3);
	result += computeL2M24(input % 9);
	result += computeL0M02(input % 10);
	return result % 34595;
}

export function tableL3M28(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M28(index));
	return lookup;
}

export function linkL3M28(): Array<number> {
	const results = new Array<number>();
	const instanceL2M39 = new BaseL2M39();
	instanceL2M39.advance();
	results.push(instanceL2M39.read() + L2M39Kind.Failed);
	const instanceL2M24 = new BaseL2M24();
	instanceL2M24.advance();
	results.push(instanceL2M24.read() + L2M24Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
