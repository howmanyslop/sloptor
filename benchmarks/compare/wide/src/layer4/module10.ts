import { computeL3M34, L3M34Kind, BaseL3M34 } from "../layer3/module34";

export enum L4M10Kind {
	Active,
	Queued,
	Done,
	Locked,
}

export const L4M10_FACTOR = 7;

export class BaseL4M10 {
	protected total = 3;
	public kind = L4M10Kind.Active;

	public tally(value: number): number {
		this.total += value * L4M10_FACTOR;
		return this.total;
	}

	public advance(): L4M10Kind {
		const order = [L4M10Kind.Active, L4M10Kind.Queued, L4M10Kind.Done, L4M10Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL4M10 extends BaseL3M34 {
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

export function computeL4M10(input: number): number {
	let result = input * 7 + 3;
	result += computeL3M34(input % 4);
	return result % 78081;
}

export const L4M10_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 3,
	chain: (value) => computeL4M10(value),
};

export function linkL4M10(): Array<number> {
	const results = new Array<number>();
	const instanceL3M34 = new BaseL3M34();
	instanceL3M34.advance();
	results.push(instanceL3M34.read() + L3M34Kind.Failed);
	return results;
}
