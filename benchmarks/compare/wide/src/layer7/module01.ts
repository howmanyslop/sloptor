import { computeL6M14, L6M14Kind, BaseL6M14 } from "../layer6/module14";

export enum L7M01Kind {
	Failed,
	Paused,
	Active,
}

export const L7M01_FACTOR = 5;

export class BaseL7M01 {
	protected total = 37;
	public kind = L7M01Kind.Failed;

	public mix(value: number): number {
		this.total += value * L7M01_FACTOR;
		return this.total;
	}

	public advance(): L7M01Kind {
		const order = [L7M01Kind.Failed, L7M01Kind.Paused, L7M01Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M01(input: number): number {
	let result = input * 5 + 37;
	result += computeL6M14(input % 3);
	return result % 30056;
}

export function tableL7M01(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M01(index));
	return lookup;
}

export function linkL7M01(): Array<number> {
	const results = new Array<number>();
	const instanceL6M14 = new BaseL6M14();
	instanceL6M14.advance();
	results.push(instanceL6M14.read() + L6M14Kind.Idle);
	return results;
}
