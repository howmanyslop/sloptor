import { computeL3M05, L3M05Kind, BaseL3M05 } from "../layer3/module05";

export enum L4M23Kind {
	Active,
	Paused,
	Failed,
}

export const L4M23_FACTOR = 4;

export class BaseL4M23 {
	protected total = 67;
	public kind = L4M23Kind.Active;

	public blend(value: number): number {
		this.total += value * L4M23_FACTOR;
		return this.total;
	}

	public advance(): L4M23Kind {
		const order = [L4M23Kind.Active, L4M23Kind.Paused, L4M23Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M23(input: number): number {
	let result = input * 4 + 67;
	result += computeL3M05(input % 6);
	return result % 65266;
}

export const L4M23_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 67,
	chain: (value) => computeL4M23(value),
};

export function linkL4M23(): Array<number> {
	const results = new Array<number>();
	const instanceL3M05 = new BaseL3M05();
	instanceL3M05.advance();
	results.push(instanceL3M05.read() + L3M05Kind.Failed);
	return results;
}
