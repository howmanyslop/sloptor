import { computeL2M04, L2M04Kind, BaseL2M04 } from "../layer2/module04";
import { computeL0M38, L0M38Kind, BaseL0M38 } from "../layer0/module38";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M23Kind {
	Queued,
	Done,
	Idle,
}

export const L3M23_FACTOR = 9;

export class BaseL3M23 {
	protected total = 17;
	public kind = L3M23Kind.Queued;

	public tally(value: number): number {
		this.total += value * L3M23_FACTOR;
		return this.total;
	}

	public advance(): L3M23Kind {
		const order = [L3M23Kind.Queued, L3M23Kind.Done, L3M23Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL3M23 extends BaseL2M04 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 10) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M23(input: number): number {
	let result = input * 9 + 17;
	result += computeL2M04(input % 8);
	result += computeL0M38(input % 7);
	result += computeL0M01(input % 8);
	return result % 30274;
}

export const L3M23_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 17,
	chain: (value) => computeL3M23(value),
};

export function linkL3M23(): Array<number> {
	const results = new Array<number>();
	const instanceL2M04 = new BaseL2M04();
	instanceL2M04.advance();
	results.push(instanceL2M04.read() + L2M04Kind.Paused);
	const instanceL0M38 = new BaseL0M38();
	instanceL0M38.advance();
	results.push(instanceL0M38.read() + L0M38Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
