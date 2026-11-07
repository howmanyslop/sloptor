import { computeL5M19, L5M19Kind, BaseL5M19 } from "../layer5/module19";

export enum L6M19Kind {
	Active,
	Queued,
	Idle,
}

export const L6M19_FACTOR = 6;

export class BaseL6M19 {
	protected total = 27;
	public kind = L6M19Kind.Active;

	public scale(value: number): number {
		this.total += value * L6M19_FACTOR;
		return this.total;
	}

	public advance(): L6M19Kind {
		const order = [L6M19Kind.Active, L6M19Kind.Queued, L6M19Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M19(input: number): number {
	let result = input * 6 + 27;
	result += computeL5M19(input % 9);
	return result % 71299;
}

export function tableL6M19(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M19(index));
	return lookup;
}

export function linkL6M19(): Array<number> {
	const results = new Array<number>();
	const instanceL5M19 = new BaseL5M19();
	instanceL5M19.advance();
	results.push(instanceL5M19.read() + L5M19Kind.Locked);
	return results;
}
