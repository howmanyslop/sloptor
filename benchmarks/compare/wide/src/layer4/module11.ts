import { computeL3M01, L3M01Kind, BaseL3M01 } from "../layer3/module01";
import { computeL3M34, L3M34Kind, BaseL3M34 } from "../layer3/module34";
import { computeL3M02, L3M02Kind, BaseL3M02 } from "../layer3/module02";

export enum L4M11Kind {
	Done,
	Paused,
	Queued,
	Failed,
}

export const L4M11_FACTOR = 16;

export class BaseL4M11 {
	protected total = 94;
	public kind = L4M11Kind.Done;

	public blend(value: number): number {
		this.total += value * L4M11_FACTOR;
		return this.total;
	}

	public advance(): L4M11Kind {
		const order = [L4M11Kind.Done, L4M11Kind.Paused, L4M11Kind.Queued, L4M11Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M11(input: number): number {
	let result = input * 16 + 94;
	result += computeL3M01(input % 10);
	result += computeL3M34(input % 10);
	result += computeL3M02(input % 11);
	return result % 3455;
}

export function tableL4M11(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M11(index));
	return lookup;
}

export function linkL4M11(): Array<number> {
	const results = new Array<number>();
	const instanceL3M01 = new BaseL3M01();
	instanceL3M01.advance();
	results.push(instanceL3M01.read() + L3M01Kind.Done);
	const instanceL3M34 = new BaseL3M34();
	instanceL3M34.advance();
	results.push(instanceL3M34.read() + L3M34Kind.Failed);
	const instanceL3M02 = new BaseL3M02();
	instanceL3M02.advance();
	results.push(instanceL3M02.read() + L3M02Kind.Failed);
	return results;
}
