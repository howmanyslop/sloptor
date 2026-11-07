import { computeL3M22, L3M22Kind, BaseL3M22 } from "../layer3/module22";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";
import { computeL1M16, L1M16Kind, BaseL1M16 } from "../layer1/module16";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L4M14Kind {
	Idle,
	Active,
	Locked,
	Paused,
	Done,
}

export const L4M14_FACTOR = 18;

export class BaseL4M14 {
	protected total = 23;
	public kind = L4M14Kind.Idle;

	public mix(value: number): number {
		this.total += value * L4M14_FACTOR;
		return this.total;
	}

	public advance(): L4M14Kind {
		const order = [L4M14Kind.Idle, L4M14Kind.Active, L4M14Kind.Locked, L4M14Kind.Paused, L4M14Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M14(input: number): number {
	let result = input * 18 + 23;
	result += computeL3M22(input % 10);
	result += computeL0M06(input % 3);
	result += computeL1M16(input % 6);
	result += computeL0M02(input % 3);
	return result % 96838;
}

export function tableL4M14(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M14(index));
	return lookup;
}

export function linkL4M14(): Array<number> {
	const results = new Array<number>();
	const instanceL3M22 = new BaseL3M22();
	instanceL3M22.advance();
	results.push(instanceL3M22.read() + L3M22Kind.Paused);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	const instanceL1M16 = new BaseL1M16();
	instanceL1M16.advance();
	results.push(instanceL1M16.read() + L1M16Kind.Queued);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
