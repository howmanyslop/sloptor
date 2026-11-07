import { computeL3M17, L3M17Kind, BaseL3M17 } from "../layer3/module17";

export enum L4M19Kind {
	Locked,
	Idle,
	Active,
	Failed,
	Paused,
}

export const L4M19_FACTOR = 13;

export class BaseL4M19 {
	protected total = 65;
	public kind = L4M19Kind.Locked;

	public mix(value: number): number {
		this.total += value * L4M19_FACTOR;
		return this.total;
	}

	public advance(): L4M19Kind {
		const order = [L4M19Kind.Locked, L4M19Kind.Idle, L4M19Kind.Active, L4M19Kind.Failed, L4M19Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M19(input: number): number {
	let result = input * 13 + 65;
	result += computeL3M17(input % 10);
	return result % 97879;
}

export function tableL4M19(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M19(index));
	return lookup;
}

export function linkL4M19(): Array<number> {
	const results = new Array<number>();
	const instanceL3M17 = new BaseL3M17();
	instanceL3M17.advance();
	results.push(instanceL3M17.read() + L3M17Kind.Done);
	return results;
}
