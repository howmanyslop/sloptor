import { computeL0M29, L0M29Kind, BaseL0M29 } from "../layer0/module29";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L1M04Kind {
	Idle,
	Done,
	Locked,
}

export const L1M04_FACTOR = 3;

export class BaseL1M04 {
	protected total = 53;
	public kind = L1M04Kind.Idle;

	public tally(value: number): number {
		this.total += value * L1M04_FACTOR;
		return this.total;
	}

	public advance(): L1M04Kind {
		const order = [L1M04Kind.Idle, L1M04Kind.Done, L1M04Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL1M04 extends BaseL1M04 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M04(input: number): number {
	let result = input * 3 + 53;
	result += computeL0M29(input % 6);
	result += computeL0M00(input % 9);
	return result % 30904;
}

export const L1M04_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 53,
	chain: (value) => computeL1M04(value),
};

export function linkL1M04(): Array<number> {
	const results = new Array<number>();
	const instanceL0M29 = new BaseL0M29();
	instanceL0M29.advance();
	results.push(instanceL0M29.read() + L0M29Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
