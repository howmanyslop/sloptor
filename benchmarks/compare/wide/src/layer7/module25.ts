import { computeL6M22, L6M22Kind, BaseL6M22 } from "../layer6/module22";

export enum L7M25Kind {
	Paused,
	Idle,
	Queued,
	Failed,
}

export const L7M25_FACTOR = 16;

export class BaseL7M25 {
	protected total = 33;
	public kind = L7M25Kind.Paused;

	public scale(value: number): number {
		this.total += value * L7M25_FACTOR;
		return this.total;
	}

	public advance(): L7M25Kind {
		const order = [L7M25Kind.Paused, L7M25Kind.Idle, L7M25Kind.Queued, L7M25Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M25(input: number): number {
	let result = input * 16 + 33;
	result += computeL6M22(input % 4);
	return result % 21461;
}

export function tableL7M25(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M25(index));
	return lookup;
}

export function linkL7M25(): Array<number> {
	const results = new Array<number>();
	const instanceL6M22 = new BaseL6M22();
	instanceL6M22.advance();
	results.push(instanceL6M22.read() + L6M22Kind.Active);
	return results;
}
