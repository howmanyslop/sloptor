import { computeL7M02, L7M02Kind, BaseL7M02 } from "../layer7/module02";
import { computeL7M27, L7M27Kind, BaseL7M27 } from "../layer7/module27";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L8M22Kind {
	Done,
	Failed,
	Active,
	Locked,
	Paused,
}

export const L8M22_FACTOR = 10;

export class BaseL8M22 {
	protected total = 77;
	public kind = L8M22Kind.Done;

	public tally(value: number): number {
		this.total += value * L8M22_FACTOR;
		return this.total;
	}

	public advance(): L8M22Kind {
		const order = [L8M22Kind.Done, L8M22Kind.Failed, L8M22Kind.Active, L8M22Kind.Locked, L8M22Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M22(input: number): number {
	let result = input * 10 + 77;
	result += computeL7M02(input % 6);
	result += computeL7M27(input % 6);
	result += computeL0M00(input % 11);
	return result % 34976;
}

export function tableL8M22(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M22(index));
	return lookup;
}

export function linkL8M22(): Array<number> {
	const results = new Array<number>();
	const instanceL7M02 = new BaseL7M02();
	instanceL7M02.advance();
	results.push(instanceL7M02.read() + L7M02Kind.Queued);
	const instanceL7M27 = new BaseL7M27();
	instanceL7M27.advance();
	results.push(instanceL7M27.read() + L7M27Kind.Queued);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
