import { computeL1M37, L1M37Kind, BaseL1M37 } from "../layer1/module37";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M05Kind {
	Failed,
	Done,
	Idle,
	Active,
}

export const L2M05_FACTOR = 4;

export class BaseL2M05 {
	protected total = 20;
	public kind = L2M05Kind.Failed;

	public tally(value: number): number {
		this.total += value * L2M05_FACTOR;
		return this.total;
	}

	public advance(): L2M05Kind {
		const order = [L2M05Kind.Failed, L2M05Kind.Done, L2M05Kind.Idle, L2M05Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL2M05 extends BaseL2M05 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M05(input: number): number {
	let result = input * 4 + 20;
	result += computeL1M37(input % 8);
	result += computeL0M00(input % 4);
	return result % 17084;
}

export const L2M05_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 20,
	chain: (value) => computeL2M05(value),
};

export function linkL2M05(): Array<number> {
	const results = new Array<number>();
	const instanceL1M37 = new BaseL1M37();
	instanceL1M37.advance();
	results.push(instanceL1M37.read() + L1M37Kind.Locked);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
