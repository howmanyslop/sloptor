import { computeL8M10, L8M10Kind, BaseL8M10 } from "../layer8/module10";
import { computeL0M13, L0M13Kind, BaseL0M13 } from "../layer0/module13";

export enum L9M23Kind {
	Locked,
	Queued,
	Done,
	Idle,
	Failed,
}

export const L9M23_FACTOR = 9;

export class BaseL9M23 {
	protected total = 27;
	public kind = L9M23Kind.Locked;

	public mix(value: number): number {
		this.total += value * L9M23_FACTOR;
		return this.total;
	}

	public advance(): L9M23Kind {
		const order = [L9M23Kind.Locked, L9M23Kind.Queued, L9M23Kind.Done, L9M23Kind.Idle, L9M23Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M23(input: number): number {
	let result = input * 9 + 27;
	result += computeL8M10(input % 7);
	result += computeL0M13(input % 6);
	return result % 92532;
}

export function tableL9M23(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M23(index));
	return lookup;
}

export function linkL9M23(): Array<number> {
	const results = new Array<number>();
	const instanceL8M10 = new BaseL8M10();
	instanceL8M10.advance();
	results.push(instanceL8M10.read() + L8M10Kind.Active);
	const instanceL0M13 = new BaseL0M13();
	instanceL0M13.advance();
	results.push(instanceL0M13.read() + L0M13Kind.Idle);
	return results;
}
