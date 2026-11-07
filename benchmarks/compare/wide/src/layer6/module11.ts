import { computeL5M03, L5M03Kind, BaseL5M03 } from "../layer5/module03";

export enum L6M11Kind {
	Idle,
	Active,
	Paused,
	Failed,
	Queued,
}

export const L6M11_FACTOR = 12;

export class BaseL6M11 {
	protected total = 20;
	public kind = L6M11Kind.Idle;

	public tally(value: number): number {
		this.total += value * L6M11_FACTOR;
		return this.total;
	}

	public advance(): L6M11Kind {
		const order = [L6M11Kind.Idle, L6M11Kind.Active, L6M11Kind.Paused, L6M11Kind.Failed, L6M11Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M11(input: number): number {
	let result = input * 12 + 20;
	result += computeL5M03(input % 9);
	return result % 74023;
}

export const L6M11_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 20,
	chain: (value) => computeL6M11(value),
};

export function linkL6M11(): Array<number> {
	const results = new Array<number>();
	const instanceL5M03 = new BaseL5M03();
	instanceL5M03.advance();
	results.push(instanceL5M03.read() + L5M03Kind.Queued);
	return results;
}
