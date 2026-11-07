import { computeL5M35, L5M35Kind, BaseL5M35 } from "../layer5/module35";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L6M27Kind {
	Queued,
	Done,
	Paused,
}

export const L6M27_FACTOR = 3;

export class BaseL6M27 {
	protected total = 86;
	public kind = L6M27Kind.Queued;

	public shift(value: number): number {
		this.total += value * L6M27_FACTOR;
		return this.total;
	}

	public advance(): L6M27Kind {
		const order = [L6M27Kind.Queued, L6M27Kind.Done, L6M27Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M27(input: number): number {
	let result = input * 3 + 86;
	result += computeL5M35(input % 3);
	result += computeL0M03(input % 4);
	return result % 41451;
}

export function tableL6M27(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M27(index));
	return lookup;
}

export function linkL6M27(): Array<number> {
	const results = new Array<number>();
	const instanceL5M35 = new BaseL5M35();
	instanceL5M35.advance();
	results.push(instanceL5M35.read() + L5M35Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
