import { computeL0M27, L0M27Kind, BaseL0M27 } from "../layer0/module27";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";

export enum L1M38Kind {
	Locked,
	Idle,
	Active,
}

export const L1M38_FACTOR = 4;

export class BaseL1M38 {
	protected total = 33;
	public kind = L1M38Kind.Locked;

	public scale(value: number): number {
		this.total += value * L1M38_FACTOR;
		return this.total;
	}

	public advance(): L1M38Kind {
		const order = [L1M38Kind.Locked, L1M38Kind.Idle, L1M38Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M38(input: number): number {
	let result = input * 4 + 33;
	result += computeL0M27(input % 8);
	result += computeL0M06(input % 5);
	return result % 57887;
}

export function tableL1M38(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M38(index));
	return lookup;
}

export function linkL1M38(): Array<number> {
	const results = new Array<number>();
	const instanceL0M27 = new BaseL0M27();
	instanceL0M27.advance();
	results.push(instanceL0M27.read() + L0M27Kind.Done);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	return results;
}
