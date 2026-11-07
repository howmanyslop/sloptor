import { computeL3M38, L3M38Kind, BaseL3M38 } from "../layer3/module38";
import { computeL3M04, L3M04Kind, BaseL3M04 } from "../layer3/module04";
import { computeL1M35, L1M35Kind, BaseL1M35 } from "../layer1/module35";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L4M22Kind {
	Idle,
	Done,
	Locked,
	Failed,
}

export const L4M22_FACTOR = 7;

export class BaseL4M22 {
	protected total = 76;
	public kind = L4M22Kind.Idle;

	public shift(value: number): number {
		this.total += value * L4M22_FACTOR;
		return this.total;
	}

	public advance(): L4M22Kind {
		const order = [L4M22Kind.Idle, L4M22Kind.Done, L4M22Kind.Locked, L4M22Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M22(input: number): number {
	let result = input * 7 + 76;
	result += computeL3M38(input % 4);
	result += computeL3M04(input % 11);
	result += computeL1M35(input % 11);
	result += computeL0M02(input % 9);
	return result % 47345;
}

export const L4M22_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 76,
	chain: (value) => computeL4M22(value),
};

export function linkL4M22(): Array<number> {
	const results = new Array<number>();
	const instanceL3M38 = new BaseL3M38();
	instanceL3M38.advance();
	results.push(instanceL3M38.read() + L3M38Kind.Active);
	const instanceL3M04 = new BaseL3M04();
	instanceL3M04.advance();
	results.push(instanceL3M04.read() + L3M04Kind.Paused);
	const instanceL1M35 = new BaseL1M35();
	instanceL1M35.advance();
	results.push(instanceL1M35.read() + L1M35Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
