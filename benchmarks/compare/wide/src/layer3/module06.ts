import { computeL2M05, L2M05Kind, BaseL2M05 } from "../layer2/module05";
import { computeL1M21, L1M21Kind, BaseL1M21 } from "../layer1/module21";
import { computeL2M13, L2M13Kind, BaseL2M13 } from "../layer2/module13";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M06Kind {
	Done,
	Active,
	Locked,
}

export const L3M06_FACTOR = 3;

export class BaseL3M06 {
	protected total = 22;
	public kind = L3M06Kind.Done;

	public shift(value: number): number {
		this.total += value * L3M06_FACTOR;
		return this.total;
	}

	public advance(): L3M06Kind {
		const order = [L3M06Kind.Done, L3M06Kind.Active, L3M06Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M06(input: number): number {
	let result = input * 3 + 22;
	result += computeL2M05(input % 8);
	result += computeL1M21(input % 5);
	result += computeL2M13(input % 8);
	result += computeL0M01(input % 4);
	return result % 29013;
}

export function tableL3M06(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M06(index));
	return lookup;
}

export function linkL3M06(): Array<number> {
	const results = new Array<number>();
	const instanceL2M05 = new BaseL2M05();
	instanceL2M05.advance();
	results.push(instanceL2M05.read() + L2M05Kind.Failed);
	const instanceL1M21 = new BaseL1M21();
	instanceL1M21.advance();
	results.push(instanceL1M21.read() + L1M21Kind.Failed);
	const instanceL2M13 = new BaseL2M13();
	instanceL2M13.advance();
	results.push(instanceL2M13.read() + L2M13Kind.Failed);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
