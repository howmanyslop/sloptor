import { computeL4M13, L4M13Kind, BaseL4M13 } from "../layer4/module13";
import { computeL2M29, L2M29Kind, BaseL2M29 } from "../layer2/module29";
import { computeL3M11, L3M11Kind, BaseL3M11 } from "../layer3/module11";

export enum L5M24Kind {
	Failed,
	Done,
	Queued,
	Paused,
}

export const L5M24_FACTOR = 12;

export class BaseL5M24 {
	protected total = 43;
	public kind = L5M24Kind.Failed;

	public shift(value: number): number {
		this.total += value * L5M24_FACTOR;
		return this.total;
	}

	public advance(): L5M24Kind {
		const order = [L5M24Kind.Failed, L5M24Kind.Done, L5M24Kind.Queued, L5M24Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M24(input: number): number {
	let result = input * 12 + 43;
	result += computeL4M13(input % 7);
	result += computeL2M29(input % 11);
	result += computeL3M11(input % 6);
	return result % 9304;
}

export function tableL5M24(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M24(index));
	return lookup;
}

export function linkL5M24(): Array<number> {
	const results = new Array<number>();
	const instanceL4M13 = new BaseL4M13();
	instanceL4M13.advance();
	results.push(instanceL4M13.read() + L4M13Kind.Queued);
	const instanceL2M29 = new BaseL2M29();
	instanceL2M29.advance();
	results.push(instanceL2M29.read() + L2M29Kind.Locked);
	const instanceL3M11 = new BaseL3M11();
	instanceL3M11.advance();
	results.push(instanceL3M11.read() + L3M11Kind.Idle);
	return results;
}
