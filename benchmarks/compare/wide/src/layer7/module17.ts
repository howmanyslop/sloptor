import { computeL6M06, L6M06Kind, BaseL6M06 } from "../layer6/module06";
import { computeL5M36, L5M36Kind, BaseL5M36 } from "../layer5/module36";
import { computeL0M38, L0M38Kind, BaseL0M38 } from "../layer0/module38";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L7M17Kind {
	Locked,
	Failed,
	Done,
}

export const L7M17_FACTOR = 5;

export class BaseL7M17 {
	protected total = 28;
	public kind = L7M17Kind.Locked;

	public warp(value: number): number {
		this.total += value * L7M17_FACTOR;
		return this.total;
	}

	public advance(): L7M17Kind {
		const order = [L7M17Kind.Locked, L7M17Kind.Failed, L7M17Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M17(input: number): number {
	let result = input * 5 + 28;
	result += computeL6M06(input % 11);
	result += computeL5M36(input % 6);
	result += computeL0M38(input % 6);
	result += computeL0M03(input % 4);
	return result % 81230;
}

export const L7M17_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 28,
	chain: (value) => computeL7M17(value),
};

export function linkL7M17(): Array<number> {
	const results = new Array<number>();
	const instanceL6M06 = new BaseL6M06();
	instanceL6M06.advance();
	results.push(instanceL6M06.read() + L6M06Kind.Done);
	const instanceL5M36 = new BaseL5M36();
	instanceL5M36.advance();
	results.push(instanceL5M36.read() + L5M36Kind.Idle);
	const instanceL0M38 = new BaseL0M38();
	instanceL0M38.advance();
	results.push(instanceL0M38.read() + L0M38Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
