import { computeL5M01, L5M01Kind, BaseL5M01 } from "../layer5/module01";

export enum L6M05Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L6M05_FACTOR = 16;

export class BaseL6M05 {
	protected total = 61;
	public kind = L6M05Kind.Idle;

	public shift(value: number): number {
		this.total += value * L6M05_FACTOR;
		return this.total;
	}

	public advance(): L6M05Kind {
		const order = [L6M05Kind.Idle, L6M05Kind.Active, L6M05Kind.Paused, L6M05Kind.Done, L6M05Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL6M05 extends BaseL5M01 {
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

export function computeL6M05(input: number): number {
	let result = input * 16 + 61;
	result += computeL5M01(input % 10);
	return result % 1563;
}

export const L6M05_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 61,
	chain: (value) => computeL6M05(value),
};

export function linkL6M05(): Array<number> {
	const results = new Array<number>();
	const instanceL5M01 = new BaseL5M01();
	instanceL5M01.advance();
	results.push(instanceL5M01.read() + L5M01Kind.Idle);
	return results;
}
