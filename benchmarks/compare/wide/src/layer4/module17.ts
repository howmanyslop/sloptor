import { computeL3M03, L3M03Kind, BaseL3M03 } from "../layer3/module03";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L4M17Kind {
	Done,
	Active,
	Failed,
	Idle,
}

export const L4M17_FACTOR = 9;

export class BaseL4M17 {
	protected total = 5;
	public kind = L4M17Kind.Done;

	public scale(value: number): number {
		this.total += value * L4M17_FACTOR;
		return this.total;
	}

	public advance(): L4M17Kind {
		const order = [L4M17Kind.Done, L4M17Kind.Active, L4M17Kind.Failed, L4M17Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M17(input: number): number {
	let result = input * 9 + 5;
	result += computeL3M03(input % 3);
	result += computeL0M02(input % 3);
	return result % 57260;
}

export function tableL4M17(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M17(index));
	return lookup;
}

export function linkL4M17(): Array<number> {
	const results = new Array<number>();
	const instanceL3M03 = new BaseL3M03();
	instanceL3M03.advance();
	results.push(instanceL3M03.read() + L3M03Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
