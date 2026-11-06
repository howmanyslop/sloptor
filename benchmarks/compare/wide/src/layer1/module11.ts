import { computeL0M38, L0M38Kind, BaseL0M38 } from "../layer0/module38";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L1M11Kind {
	Idle,
	Queued,
	Done,
	Active,
}

export const L1M11_FACTOR = 13;

export class BaseL1M11 {
	protected total = 59;
	public kind = L1M11Kind.Idle;

	public shift(value: number): number {
		this.total += value * L1M11_FACTOR;
		return this.total;
	}

	public advance(): L1M11Kind {
		const order = [L1M11Kind.Idle, L1M11Kind.Queued, L1M11Kind.Done, L1M11Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M11(input: number): number {
	let result = input * 13 + 59;
	result += computeL0M38(input % 9);
	result += computeL0M00(input % 4);
	return result % 15021;
}

export function tableL1M11(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M11(index));
	return lookup;
}

export function linkL1M11(): Array<number> {
	const results = new Array<number>();
	const instanceL0M38 = new BaseL0M38();
	instanceL0M38.advance();
	results.push(instanceL0M38.read() + L0M38Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
