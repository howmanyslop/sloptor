import { computeL8M18, L8M18Kind, BaseL8M18 } from "../layer8/module18";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L9M07Kind {
	Active,
	Done,
	Idle,
	Failed,
	Paused,
}

export const L9M07_FACTOR = 11;

export class BaseL9M07 {
	protected total = 15;
	public kind = L9M07Kind.Active;

	public weigh(value: number): number {
		this.total += value * L9M07_FACTOR;
		return this.total;
	}

	public advance(): L9M07Kind {
		const order = [L9M07Kind.Active, L9M07Kind.Done, L9M07Kind.Idle, L9M07Kind.Failed, L9M07Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M07(input: number): number {
	let result = input * 11 + 15;
	result += computeL8M18(input % 9);
	result += computeL0M01(input % 7);
	return result % 59026;
}

export function tableL9M07(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M07(index));
	return lookup;
}

export function linkL9M07(): Array<number> {
	const results = new Array<number>();
	const instanceL8M18 = new BaseL8M18();
	instanceL8M18.advance();
	results.push(instanceL8M18.read() + L8M18Kind.Done);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
