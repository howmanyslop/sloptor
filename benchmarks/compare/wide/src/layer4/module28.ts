import { computeL3M00, L3M00Kind, BaseL3M00 } from "../layer3/module00";

export enum L4M28Kind {
	Locked,
	Done,
	Queued,
}

export const L4M28_FACTOR = 6;

export class BaseL4M28 {
	protected total = 92;
	public kind = L4M28Kind.Locked;

	public warp(value: number): number {
		this.total += value * L4M28_FACTOR;
		return this.total;
	}

	public advance(): L4M28Kind {
		const order = [L4M28Kind.Locked, L4M28Kind.Done, L4M28Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL4M28 extends BaseL4M28 {
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

export function computeL4M28(input: number): number {
	let result = input * 6 + 92;
	result += computeL3M00(input % 10);
	return result % 69068;
}

export const L4M28_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 92,
	chain: (value) => computeL4M28(value),
};

export function linkL4M28(): Array<number> {
	const results = new Array<number>();
	const instanceL3M00 = new BaseL3M00();
	instanceL3M00.advance();
	results.push(instanceL3M00.read() + L3M00Kind.Idle);
	return results;
}
