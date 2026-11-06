import { computeL8M22, L8M22Kind, BaseL8M22 } from "../layer8/module22";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L9M19Kind {
	Queued,
	Paused,
	Failed,
	Active,
}

export const L9M19_FACTOR = 5;

export class BaseL9M19 {
	protected total = 7;
	public kind = L9M19Kind.Queued;

	public tally(value: number): number {
		this.total += value * L9M19_FACTOR;
		return this.total;
	}

	public advance(): L9M19Kind {
		const order = [L9M19Kind.Queued, L9M19Kind.Paused, L9M19Kind.Failed, L9M19Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M19(input: number): number {
	let result = input * 5 + 7;
	result += computeL8M22(input % 4);
	result += computeL0M03(input % 6);
	return result % 49657;
}

export function tableL9M19(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M19(index));
	return lookup;
}

export function linkL9M19(): Array<number> {
	const results = new Array<number>();
	const instanceL8M22 = new BaseL8M22();
	instanceL8M22.advance();
	results.push(instanceL8M22.read() + L8M22Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
