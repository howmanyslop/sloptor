import { computeL5M05, L5M05Kind, BaseL5M05 } from "../layer5/module05";
import { computeL1M05, L1M05Kind, BaseL1M05 } from "../layer1/module05";

export enum L6M17Kind {
	Failed,
	Queued,
	Idle,
	Locked,
	Done,
}

export const L6M17_FACTOR = 4;

export class BaseL6M17 {
	protected total = 54;
	public kind = L6M17Kind.Failed;

	public fold(value: number): number {
		this.total += value * L6M17_FACTOR;
		return this.total;
	}

	public advance(): L6M17Kind {
		const order = [L6M17Kind.Failed, L6M17Kind.Queued, L6M17Kind.Idle, L6M17Kind.Locked, L6M17Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL6M17 extends BaseL5M05 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M17(input: number): number {
	let result = input * 4 + 54;
	result += computeL5M05(input % 5);
	result += computeL1M05(input % 5);
	return result % 32537;
}

export const L6M17_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 54,
	chain: (value) => computeL6M17(value),
};

export function linkL6M17(): Array<number> {
	const results = new Array<number>();
	const instanceL5M05 = new BaseL5M05();
	instanceL5M05.advance();
	results.push(instanceL5M05.read() + L5M05Kind.Paused);
	const instanceL1M05 = new BaseL1M05();
	instanceL1M05.advance();
	results.push(instanceL1M05.read() + L1M05Kind.Idle);
	return results;
}
