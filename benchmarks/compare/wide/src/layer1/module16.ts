import { computeL0M33, L0M33Kind, BaseL0M33 } from "../layer0/module33";
import { computeL0M16, L0M16Kind, BaseL0M16 } from "../layer0/module16";
import { computeL0M38, L0M38Kind, BaseL0M38 } from "../layer0/module38";

export enum L1M16Kind {
	Queued,
	Idle,
	Active,
	Paused,
}

export const L1M16_FACTOR = 8;

export class BaseL1M16 {
	protected total = 28;
	public kind = L1M16Kind.Queued;

	public tally(value: number): number {
		this.total += value * L1M16_FACTOR;
		return this.total;
	}

	public advance(): L1M16Kind {
		const order = [L1M16Kind.Queued, L1M16Kind.Idle, L1M16Kind.Active, L1M16Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M16(input: number): number {
	let result = input * 8 + 28;
	result += computeL0M33(input % 6);
	result += computeL0M16(input % 7);
	result += computeL0M38(input % 4);
	return result % 93771;
}

export const L1M16_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 28,
	chain: (value) => computeL1M16(value),
};

export function linkL1M16(): Array<number> {
	const results = new Array<number>();
	const instanceL0M33 = new BaseL0M33();
	instanceL0M33.advance();
	results.push(instanceL0M33.read() + L0M33Kind.Idle);
	const instanceL0M16 = new BaseL0M16();
	instanceL0M16.advance();
	results.push(instanceL0M16.read() + L0M16Kind.Paused);
	const instanceL0M38 = new BaseL0M38();
	instanceL0M38.advance();
	results.push(instanceL0M38.read() + L0M38Kind.Idle);
	return results;
}
