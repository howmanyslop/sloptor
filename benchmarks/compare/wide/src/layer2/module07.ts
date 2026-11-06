import { computeL1M11, L1M11Kind, BaseL1M11 } from "../layer1/module11";
import { computeL0M38, L0M38Kind, BaseL0M38 } from "../layer0/module38";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M07Kind {
	Done,
	Failed,
	Idle,
	Active,
}

export const L2M07_FACTOR = 10;

export class BaseL2M07 {
	protected total = 75;
	public kind = L2M07Kind.Done;

	public fold(value: number): number {
		this.total += value * L2M07_FACTOR;
		return this.total;
	}

	public advance(): L2M07Kind {
		const order = [L2M07Kind.Done, L2M07Kind.Failed, L2M07Kind.Idle, L2M07Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M07(input: number): number {
	let result = input * 10 + 75;
	result += computeL1M11(input % 8);
	result += computeL0M38(input % 6);
	result += computeL0M00(input % 9);
	return result % 80143;
}

export function tableL2M07(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M07(index));
	return lookup;
}

export function linkL2M07(): Array<number> {
	const results = new Array<number>();
	const instanceL1M11 = new BaseL1M11();
	instanceL1M11.advance();
	results.push(instanceL1M11.read() + L1M11Kind.Idle);
	const instanceL0M38 = new BaseL0M38();
	instanceL0M38.advance();
	results.push(instanceL0M38.read() + L0M38Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
