import { computeL5M13, L5M13Kind, BaseL5M13 } from "../layer5/module13";
import { computeL3M31, L3M31Kind, BaseL3M31 } from "../layer3/module31";

export enum L6M01Kind {
	Queued,
	Failed,
	Done,
}

export const L6M01_FACTOR = 11;

export class BaseL6M01 {
	protected total = 89;
	public kind = L6M01Kind.Queued;

	public tally(value: number): number {
		this.total += value * L6M01_FACTOR;
		return this.total;
	}

	public advance(): L6M01Kind {
		const order = [L6M01Kind.Queued, L6M01Kind.Failed, L6M01Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M01(input: number): number {
	let result = input * 11 + 89;
	result += computeL5M13(input % 8);
	result += computeL3M31(input % 6);
	return result % 66002;
}

export function tableL6M01(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M01(index));
	return lookup;
}

export function linkL6M01(): Array<number> {
	const results = new Array<number>();
	const instanceL5M13 = new BaseL5M13();
	instanceL5M13.advance();
	results.push(instanceL5M13.read() + L5M13Kind.Idle);
	const instanceL3M31 = new BaseL3M31();
	instanceL3M31.advance();
	results.push(instanceL3M31.read() + L3M31Kind.Idle);
	return results;
}
