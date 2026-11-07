import { computeL8M00, L8M00Kind, BaseL8M00 } from "../layer8/module00";

export enum L9M33Kind {
	Queued,
	Failed,
	Done,
}

export const L9M33_FACTOR = 9;

export class BaseL9M33 {
	protected total = 41;
	public kind = L9M33Kind.Queued;

	public shift(value: number): number {
		this.total += value * L9M33_FACTOR;
		return this.total;
	}

	public advance(): L9M33Kind {
		const order = [L9M33Kind.Queued, L9M33Kind.Failed, L9M33Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M33(input: number): number {
	let result = input * 9 + 41;
	result += computeL8M00(input % 7);
	return result % 52517;
}

export function tableL9M33(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M33(index));
	return lookup;
}

export function linkL9M33(): Array<number> {
	const results = new Array<number>();
	const instanceL8M00 = new BaseL8M00();
	instanceL8M00.advance();
	results.push(instanceL8M00.read() + L8M00Kind.Queued);
	return results;
}
