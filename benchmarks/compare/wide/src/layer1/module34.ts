import { computeL0M39, L0M39Kind, BaseL0M39 } from "../layer0/module39";

export enum L1M34Kind {
	Paused,
	Queued,
	Active,
	Failed,
}

export const L1M34_FACTOR = 4;

export class BaseL1M34 {
	protected total = 78;
	public kind = L1M34Kind.Paused;

	public fold(value: number): number {
		this.total += value * L1M34_FACTOR;
		return this.total;
	}

	public advance(): L1M34Kind {
		const order = [L1M34Kind.Paused, L1M34Kind.Queued, L1M34Kind.Active, L1M34Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL1M34 extends BaseL1M34 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M34(input: number): number {
	let result = input * 4 + 78;
	result += computeL0M39(input % 9);
	return result % 10218;
}

export const L1M34_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 78,
	chain: (value) => computeL1M34(value),
};

export function linkL1M34(): Array<number> {
	const results = new Array<number>();
	const instanceL0M39 = new BaseL0M39();
	instanceL0M39.advance();
	results.push(instanceL0M39.read() + L0M39Kind.Active);
	return results;
}
