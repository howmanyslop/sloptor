import { computeL8M29, L8M29Kind, BaseL8M29 } from "../layer8/module29";
import { computeL3M17, L3M17Kind, BaseL3M17 } from "../layer3/module17";

export enum L9M20Kind {
	Idle,
	Failed,
	Active,
}

export const L9M20_FACTOR = 2;

export class BaseL9M20 {
	protected total = 14;
	public kind = L9M20Kind.Idle;

	public shift(value: number): number {
		this.total += value * L9M20_FACTOR;
		return this.total;
	}

	public advance(): L9M20Kind {
		const order = [L9M20Kind.Idle, L9M20Kind.Failed, L9M20Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M20(input: number): number {
	let result = input * 2 + 14;
	result += computeL8M29(input % 11);
	result += computeL3M17(input % 8);
	return result % 60242;
}

export function tableL9M20(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M20(index));
	return lookup;
}

export function linkL9M20(): Array<number> {
	const results = new Array<number>();
	const instanceL8M29 = new BaseL8M29();
	instanceL8M29.advance();
	results.push(instanceL8M29.read() + L8M29Kind.Idle);
	const instanceL3M17 = new BaseL3M17();
	instanceL3M17.advance();
	results.push(instanceL3M17.read() + L3M17Kind.Done);
	return results;
}
