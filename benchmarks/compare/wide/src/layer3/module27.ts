import { computeL2M32, L2M32Kind, BaseL2M32 } from "../layer2/module32";

export enum L3M27Kind {
	Queued,
	Failed,
	Done,
}

export const L3M27_FACTOR = 14;

export class BaseL3M27 {
	protected total = 90;
	public kind = L3M27Kind.Queued;

	public fold(value: number): number {
		this.total += value * L3M27_FACTOR;
		return this.total;
	}

	public advance(): L3M27Kind {
		const order = [L3M27Kind.Queued, L3M27Kind.Failed, L3M27Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M27(input: number): number {
	let result = input * 14 + 90;
	result += computeL2M32(input % 9);
	return result % 47343;
}

export function tableL3M27(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M27(index));
	return lookup;
}

export function linkL3M27(): Array<number> {
	const results = new Array<number>();
	const instanceL2M32 = new BaseL2M32();
	instanceL2M32.advance();
	results.push(instanceL2M32.read() + L2M32Kind.Active);
	return results;
}
