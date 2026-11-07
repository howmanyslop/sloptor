import { computeL4M20, L4M20Kind, BaseL4M20 } from "../layer4/module20";
import { computeL3M38, L3M38Kind, BaseL3M38 } from "../layer3/module38";

export enum L5M25Kind {
	Paused,
	Done,
	Locked,
	Queued,
}

export const L5M25_FACTOR = 3;

export class BaseL5M25 {
	protected total = 11;
	public kind = L5M25Kind.Paused;

	public fold(value: number): number {
		this.total += value * L5M25_FACTOR;
		return this.total;
	}

	public advance(): L5M25Kind {
		const order = [L5M25Kind.Paused, L5M25Kind.Done, L5M25Kind.Locked, L5M25Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M25(input: number): number {
	let result = input * 3 + 11;
	result += computeL4M20(input % 11);
	result += computeL3M38(input % 7);
	return result % 29231;
}

export const L5M25_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 11,
	chain: (value) => computeL5M25(value),
};

export function linkL5M25(): Array<number> {
	const results = new Array<number>();
	const instanceL4M20 = new BaseL4M20();
	instanceL4M20.advance();
	results.push(instanceL4M20.read() + L4M20Kind.Done);
	const instanceL3M38 = new BaseL3M38();
	instanceL3M38.advance();
	results.push(instanceL3M38.read() + L3M38Kind.Active);
	return results;
}
