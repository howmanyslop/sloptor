import { computeL0M19, L0M19Kind, BaseL0M19 } from "../layer0/module19";
import { computeL0M21, L0M21Kind, BaseL0M21 } from "../layer0/module21";

export enum L1M14Kind {
	Queued,
	Done,
	Active,
	Locked,
	Failed,
}

export const L1M14_FACTOR = 14;

export class BaseL1M14 {
	protected total = 74;
	public kind = L1M14Kind.Queued;

	public tally(value: number): number {
		this.total += value * L1M14_FACTOR;
		return this.total;
	}

	public advance(): L1M14Kind {
		const order = [L1M14Kind.Queued, L1M14Kind.Done, L1M14Kind.Active, L1M14Kind.Locked, L1M14Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL1M14 extends BaseL1M14 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M14(input: number): number {
	let result = input * 14 + 74;
	result += computeL0M19(input % 7);
	result += computeL0M21(input % 8);
	return result % 23226;
}

export const L1M14_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 74,
	chain: (value) => computeL1M14(value),
};

export function linkL1M14(): Array<number> {
	const results = new Array<number>();
	const instanceL0M19 = new BaseL0M19();
	instanceL0M19.advance();
	results.push(instanceL0M19.read() + L0M19Kind.Locked);
	const instanceL0M21 = new BaseL0M21();
	instanceL0M21.advance();
	results.push(instanceL0M21.read() + L0M21Kind.Done);
	return results;
}
