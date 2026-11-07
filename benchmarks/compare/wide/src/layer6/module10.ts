import { computeL5M36, L5M36Kind, BaseL5M36 } from "../layer5/module36";

export enum L6M10Kind {
	Locked,
	Done,
	Idle,
	Active,
	Failed,
}

export const L6M10_FACTOR = 5;

export class BaseL6M10 {
	protected total = 81;
	public kind = L6M10Kind.Locked;

	public warp(value: number): number {
		this.total += value * L6M10_FACTOR;
		return this.total;
	}

	public advance(): L6M10Kind {
		const order = [L6M10Kind.Locked, L6M10Kind.Done, L6M10Kind.Idle, L6M10Kind.Active, L6M10Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL6M10 extends BaseL5M36 {
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

export function computeL6M10(input: number): number {
	let result = input * 5 + 81;
	result += computeL5M36(input % 9);
	return result % 44822;
}

export const L6M10_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 81,
	chain: (value) => computeL6M10(value),
};

export function linkL6M10(): Array<number> {
	const results = new Array<number>();
	const instanceL5M36 = new BaseL5M36();
	instanceL5M36.advance();
	results.push(instanceL5M36.read() + L5M36Kind.Idle);
	return results;
}
