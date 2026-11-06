import { computeL3M39, L3M39Kind, BaseL3M39 } from "../layer3/module39";

export enum L4M05Kind {
	Idle,
	Queued,
	Paused,
	Failed,
	Done,
}

export const L4M05_FACTOR = 13;

export class BaseL4M05 {
	protected total = 61;
	public kind = L4M05Kind.Idle;

	public warp(value: number): number {
		this.total += value * L4M05_FACTOR;
		return this.total;
	}

	public advance(): L4M05Kind {
		const order = [L4M05Kind.Idle, L4M05Kind.Queued, L4M05Kind.Paused, L4M05Kind.Failed, L4M05Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL4M05 extends BaseL3M39 {
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

export function computeL4M05(input: number): number {
	let result = input * 13 + 61;
	result += computeL3M39(input % 5);
	return result % 84949;
}

export const L4M05_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 61,
	chain: (value) => computeL4M05(value),
};

export function linkL4M05(): Array<number> {
	const results = new Array<number>();
	const instanceL3M39 = new BaseL3M39();
	instanceL3M39.advance();
	results.push(instanceL3M39.read() + L3M39Kind.Done);
	return results;
}
