import { computeL5M14, L5M14Kind, BaseL5M14 } from "../layer5/module14";

export enum L6M24Kind {
	Queued,
	Done,
	Locked,
	Paused,
	Failed,
}

export const L6M24_FACTOR = 8;

export class BaseL6M24 {
	protected total = 68;
	public kind = L6M24Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L6M24_FACTOR;
		return this.total;
	}

	public advance(): L6M24Kind {
		const order = [L6M24Kind.Queued, L6M24Kind.Done, L6M24Kind.Locked, L6M24Kind.Paused, L6M24Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M24(input: number): number {
	let result = input * 8 + 68;
	result += computeL5M14(input % 5);
	return result % 78395;
}

export const L6M24_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 68,
	chain: (value) => computeL6M24(value),
};

export function linkL6M24(): Array<number> {
	const results = new Array<number>();
	const instanceL5M14 = new BaseL5M14();
	instanceL5M14.advance();
	results.push(instanceL5M14.read() + L5M14Kind.Paused);
	return results;
}
