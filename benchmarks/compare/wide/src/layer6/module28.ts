import { computeL5M02, L5M02Kind, BaseL5M02 } from "../layer5/module02";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L6M28Kind {
	Failed,
	Queued,
	Active,
	Idle,
}

export const L6M28_FACTOR = 8;

export class BaseL6M28 {
	protected total = 82;
	public kind = L6M28Kind.Failed;

	public shift(value: number): number {
		this.total += value * L6M28_FACTOR;
		return this.total;
	}

	public advance(): L6M28Kind {
		const order = [L6M28Kind.Failed, L6M28Kind.Queued, L6M28Kind.Active, L6M28Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M28(input: number): number {
	let result = input * 8 + 82;
	result += computeL5M02(input % 11);
	result += computeL0M03(input % 10);
	result += computeL0M01(input % 7);
	return result % 29195;
}

export function tableL6M28(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M28(index));
	return lookup;
}

export function linkL6M28(): Array<number> {
	const results = new Array<number>();
	const instanceL5M02 = new BaseL5M02();
	instanceL5M02.advance();
	results.push(instanceL5M02.read() + L5M02Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
