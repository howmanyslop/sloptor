import { computeL6M27, L6M27Kind, BaseL6M27 } from "../layer6/module27";

export enum L7M20Kind {
	Failed,
	Paused,
	Done,
}

export const L7M20_FACTOR = 10;

export class BaseL7M20 {
	protected total = 26;
	public kind = L7M20Kind.Failed;

	public tally(value: number): number {
		this.total += value * L7M20_FACTOR;
		return this.total;
	}

	public advance(): L7M20Kind {
		const order = [L7M20Kind.Failed, L7M20Kind.Paused, L7M20Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M20(input: number): number {
	let result = input * 10 + 26;
	result += computeL6M27(input % 8);
	return result % 93192;
}

export const L7M20_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 26,
	chain: (value) => computeL7M20(value),
};

export function linkL7M20(): Array<number> {
	const results = new Array<number>();
	const instanceL6M27 = new BaseL6M27();
	instanceL6M27.advance();
	results.push(instanceL6M27.read() + L6M27Kind.Queued);
	return results;
}
